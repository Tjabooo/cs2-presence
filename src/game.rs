//! Finding the running game and installing the Game State Integration config.

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{CFG_NAME, TOKEN};

/// A running `cs2`: its pid and when it started (Unix seconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Game {
    pub pid: u32,
    pub started: i64,
}

/// The user's running native `cs2`, if any.
pub fn find_game() -> Option<Game> {
    let uid = unsafe { libc::getuid() };
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        if let Some(started) = check_process(&entry.path(), uid) {
            return Some(Game { pid, started });
        }
    }
    None
}

fn check_process(proc: &Path, uid: u32) -> Option<i64> {
    if fs::metadata(proc).ok()?.uid() != uid {
        return None;
    }
    if fs::read_to_string(proc.join("comm")).ok()?.trim() != "cs2" {
        return None;
    }
    let exe = fs::canonicalize(proc.join("exe")).ok()?;
    if exe.file_name()? != "cs2" {
        return None;
    }
    let stat = fs::read_to_string(proc.join("stat")).ok()?;
    let ticks: f64 = stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()?;
    let uptime: f64 = fs::read_to_string("/proc/uptime").ok()?.split_whitespace().next()?.parse().ok()?;
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs_f64();
    Some((now - uptime + ticks / hz) as i64)
}

/// The config CS2 reads at launch to post its game state to us.
pub fn cfg_text(port: u16) -> String {
    format!(
        r#""cs2-presence"
{{
    "uri"       "http://127.0.0.1:{port}/"
    "timeout"   "5.0"
    "buffer"    "0.5"
    "throttle"  "1.0"
    "heartbeat" "10.0"
    "auth"
    {{
        "token" "{TOKEN}"
    }}
    "data"
    {{
        "provider"            "1"
        "map"                 "1"
        "round"               "1"
        "player_id"           "1"
        "player_state"        "1"
        "player_match_stats"  "1"
    }}
}}
"#
    )
}

/// Library paths listed in a Steam `libraryfolders.vdf`.
pub fn vdf_paths(vdf: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut rest = vdf;
    while let Some(i) = rest.find("\"path\"") {
        rest = &rest[i + 6..];
        let trimmed = rest.trim_start();
        if trimmed.len() == rest.len() || !trimmed.starts_with('"') {
            continue;
        }
        let value = &trimmed[1..];
        if let Some(end) = value.find('"')
            && end > 0
        {
            paths.push(PathBuf::from(value[..end].replace("\\\\", "\\")));
            rest = &value[end + 1..];
        }
    }
    paths
}

/// CS2's cfg folder in every Steam library (native, Flatpak and Snap Steam).
pub fn cfg_dirs() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let roots = [
        ".local/share/Steam",
        ".steam/steam",
        ".steam/root",
        ".var/app/com.valvesoftware.Steam/.local/share/Steam",
        "snap/steam/common/.local/share/Steam",
    ];
    let mut libraries = BTreeSet::new();
    for root in roots.map(|r| home.join(r)) {
        let Ok(bytes) = fs::read(root.join("steamapps/libraryfolders.vdf")) else {
            continue;
        };
        let vdf = String::from_utf8_lossy(&bytes);
        libraries.insert(fs::canonicalize(&root).unwrap_or(root));
        for path in vdf_paths(&vdf) {
            libraries.insert(fs::canonicalize(&path).unwrap_or(path));
        }
    }
    let mut found = BTreeSet::new();
    for library in libraries {
        let cfg = library.join("steamapps/common/Counter-Strike Global Offensive/game/csgo/cfg");
        if cfg.is_dir() {
            found.insert(fs::canonicalize(&cfg).unwrap_or(cfg));
        }
    }
    found.into_iter().collect()
}

/// Put the Game State Integration config in place; CS2 reads it when it starts.
pub fn install_cfg(port: u16) {
    let cfg = cfg_text(port);
    for folder in cfg_dirs() {
        let target = folder.join(CFG_NAME);
        if fs::read_to_string(&target).is_ok_and(|current| current == cfg) {
            continue;
        }
        match fs::write(&target, &cfg) {
            Ok(()) => println!("Installed Game State Integration config in {}", folder.display()),
            Err(error) => println!("Could not write {}: {error}", target.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_library_paths() {
        let vdf = r#""libraryfolders"
{
	"0"
	{
		"path"		"/home/me/.local/share/Steam"
		"label"		""
	}
	"1"
	{
		"path"		"/mnt/games/Steam\\Library"
	}
}"#;
        assert_eq!(
            vdf_paths(vdf),
            [PathBuf::from("/home/me/.local/share/Steam"), PathBuf::from("/mnt/games/Steam\\Library")]
        );
    }

    #[test]
    fn cfg_has_port_and_token() {
        let cfg = cfg_text(4000);
        assert!(cfg.contains("\"uri\"       \"http://127.0.0.1:4000/\""));
        assert!(cfg.contains("\"token\" \"cs2-presence\""));
    }
}
