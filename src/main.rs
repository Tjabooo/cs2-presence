//! cs2-presence: native Linux Counter-Strike 2 as Discord Rich Presence.
//!
//! Discord doesn't detect CS2's native Linux build. This finds the running `cs2` process and
//! publishes an activity through the Discord client's local RPC socket. With CS2's Game State
//! Integration (a config file this installs in the game's cfg folder) the activity also shows the
//! mode, map, score, round, your team, K/A/D, MVPs, health, armour and money. Runs as a systemd
//! user service.

mod activity;
mod discord;
mod game;
mod gsi;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use discord::Rpc;

pub const CLIENT_ID: &str = "1158877933042143272"; // Discord's own Counter-Strike 2 application.
pub const TOKEN: &str = "cs2-presence";
pub const CFG_NAME: &str = "gamestate_integration_cs2_presence.cfg";
const DEFAULT_PORT: u16 = 3713;
const MIN_SEND_INTERVAL: Duration = Duration::from_secs(4); // Discord allows 5 updates per 20 s.
const RESEND_INTERVAL: Duration = Duration::from_secs(30); // Re-send an unchanged activity this often.
const GSI_STALE_AFTER: Duration = Duration::from_secs(45); // The game sends a heartbeat every 10 s.
const SCAN_INTERVAL: Duration = Duration::from_secs(5);
const CFG_INTERVAL: Duration = Duration::from_secs(300);

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn main() {
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-V" | "--version" => println!("cs2-presence {}", env!("CARGO_PKG_VERSION")),
            _ => println!(
                "cs2-presence {}: native Linux CS2 as Discord Rich Presence.\n\
                 Run it without arguments (normally as the cs2-presence systemd user service).\n\
                 CS2_PRESENCE_PORT sets the Game State Integration port (default {DEFAULT_PORT}).",
                env!("CARGO_PKG_VERSION")
            ),
        }
        return;
    }

    let port = match std::env::var("CS2_PRESENCE_PORT") {
        Ok(value) => value.trim().parse().unwrap_or_else(|_| {
            println!("CS2_PRESENCE_PORT={value:?} is not a port, using {DEFAULT_PORT}");
            DEFAULT_PORT
        }),
        Err(_) => DEFAULT_PORT,
    };
    // SIGTERM (systemd stopping the service) or Ctrl+C: clear the activity and exit.
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        if let Err(error) = signal_hook::flag::register(signal, stop.clone()) {
            println!("Could not handle signal {signal}: {error}");
        }
    }

    game::install_cfg(port);
    let state = gsi::Shared::default();
    gsi::start(port, state.clone());
    run(port, &state, &stop);
}

/// The last activity Discord accepted, for which pid.
struct Sent {
    pid: u32,
    activity: Value,
}

fn run(port: u16, state: &gsi::Shared, stop: &AtomicBool) {
    let mut tracker = activity::Tracker::default();
    let mut rpc: Option<Rpc> = None;
    let mut game = None;
    let mut sent: Option<Sent> = None;
    let start = Instant::now();
    let mut sent_at: Option<Instant> = None;
    let mut next_scan = start;
    let mut next_cfg = start;
    let mut last_error: Option<String> = None;

    while !stop.load(Ordering::SeqCst) {
        let now = Instant::now();
        if now >= next_scan {
            game = game::find_game();
            next_scan = now + SCAN_INTERVAL;
        }
        if game.is_none() && now >= next_cfg {
            game::install_cfg(port); // Steam may have been (re)installed or a library added.
            next_cfg = now + CFG_INTERVAL;
        }
        let result = (|| -> std::io::Result<()> {
            if let Some(game) = game {
                let data = {
                    let state = state.lock().unwrap_or_else(|e| e.into_inner());
                    state.at.filter(|at| now - *at < GSI_STALE_AFTER).and(state.data.clone())
                };
                let activity = activity::build(game.started, data.as_ref(), &mut tracker, unix_now());
                let changed = sent.as_ref().is_none_or(|s| s.pid != game.pid || s.activity != activity);
                let since = sent_at.map(|at| now - at);
                let due = since
                    .is_none_or(|since| (changed && since >= MIN_SEND_INTERVAL) || since >= RESEND_INTERVAL);
                if due {
                    if rpc.is_none() {
                        rpc = Some(Rpc::connect()?);
                    }
                    rpc.as_mut().unwrap().set_activity(game.pid, Some(&activity))?;
                    if sent.as_ref().is_none_or(|s| s.pid != game.pid) {
                        println!("CS2 activity started");
                    }
                    if changed {
                        let field = |key: &str| activity[key].as_str().unwrap_or("-").to_string();
                        println!("Activity: {} | {}", field("details"), field("state"));
                    }
                    sent = Some(Sent { pid: game.pid, activity });
                    sent_at = Some(now);
                    last_error = None;
                }
            } else if rpc.is_some() || sent.is_some() {
                state.lock().unwrap_or_else(|e| e.into_inner()).data = None;
                tracker.reset();
                if let (Some(rpc), Some(sent)) = (rpc.as_mut(), sent.as_ref()) {
                    rpc.set_activity(sent.pid, None)?;
                    println!("CS2 activity cleared");
                }
                rpc = None;
                sent = None;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let message = error.to_string();
            if last_error.as_deref() != Some(&message) {
                println!("Discord: {message}");
                last_error = Some(message);
            }
            rpc = None;
            sent = None;
            sent_at = Some(now);
        }
        thread::sleep(Duration::from_secs(1));
    }

    if let (Some(mut rpc), Some(sent)) = (rpc, sent) {
        let _ = rpc.set_activity(sent.pid, None);
    }
}
