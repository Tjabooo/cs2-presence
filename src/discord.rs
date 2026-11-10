//! The Discord client's local RPC socket.

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use crate::CLIENT_ID;

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;
const CLOSE: u32 = 2;
const PING: u32 = 3;
const PONG: u32 = 4;
const MAX_FRAME: u32 = 1024 * 1024;

pub struct Rpc {
    sock: UnixStream,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn closed(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionAborted, message.into())
}

/// Entries of `dir` whose names start with `prefix`, sorted.
fn matching(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with(prefix)))
        .map(|e| e.path())
        .collect();
    found.sort();
    found
}

/// Native Discord (and Vesktop/arRPC), then the Flatpak and Snap sandboxes' sockets.
pub fn socket_paths(runtime: &Path) -> Vec<PathBuf> {
    let mut paths = matching(runtime, "discord-ipc-");
    let mut apps: Vec<PathBuf> = Vec::new();
    for app in matching(&runtime.join("app"), "") {
        apps.extend(matching(&app, "discord-ipc-"));
    }
    apps.sort();
    paths.extend(apps);
    let mut snaps: Vec<PathBuf> = Vec::new();
    for snap in matching(runtime, "snap.discord") {
        snaps.extend(matching(&snap, "discord-ipc-"));
    }
    snaps.sort();
    paths.extend(snaps);
    paths
}

impl Rpc {
    /// Connect to the first Discord IPC socket that answers.
    pub fn connect() -> io::Result<Rpc> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
        for path in socket_paths(&runtime) {
            if let Ok(rpc) = Rpc::open(&path) {
                return Ok(rpc);
            }
        }
        Err(closed("Discord RPC unavailable"))
    }

    fn open(path: &Path) -> io::Result<Rpc> {
        let sock = UnixStream::connect(path)?;
        sock.set_read_timeout(Some(Duration::from_secs(3)))?;
        sock.set_write_timeout(Some(Duration::from_secs(3)))?;
        let mut rpc = Rpc { sock };
        rpc.send(HANDSHAKE, &json!({"v": 1, "client_id": CLIENT_ID}))?;
        if rpc.receive()?["evt"] != "READY" {
            return Err(closed("RPC was not ready"));
        }
        Ok(rpc)
    }

    fn send(&mut self, opcode: u32, data: &Value) -> io::Result<()> {
        let payload = serde_json::to_vec(data)?;
        let mut frame = Vec::with_capacity(8 + payload.len());
        frame.extend_from_slice(&opcode.to_le_bytes());
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(&payload);
        self.sock.write_all(&frame)
    }

    fn receive(&mut self) -> io::Result<Value> {
        loop {
            let mut header = [0u8; 8];
            self.sock.read_exact(&mut header).map_err(eof_closed)?;
            let opcode = u32::from_le_bytes(header[..4].try_into().unwrap());
            let size = u32::from_le_bytes(header[4..].try_into().unwrap());
            if size > MAX_FRAME {
                return Err(invalid("RPC frame too large"));
            }
            let mut body = vec![0; size as usize];
            self.sock.read_exact(&mut body).map_err(eof_closed)?;
            let data: Value = serde_json::from_slice(&body)?;
            match opcode {
                PING => self.send(PONG, &data)?,
                CLOSE => return Err(closed(format!("RPC closed: {data}"))),
                _ => return Ok(data),
            }
        }
    }

    /// Show `activity` for the game process `pid`; `None` clears it.
    pub fn set_activity(&mut self, pid: u32, activity: Option<&Value>) -> io::Result<()> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nonce = format!("{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed));
        self.send(
            FRAME,
            &json!({"cmd": "SET_ACTIVITY", "nonce": nonce, "args": {"pid": pid, "activity": activity}}),
        )?;
        loop {
            let reply = self.receive()?;
            if reply["nonce"] == nonce.as_str() {
                if reply["evt"] == "ERROR" {
                    return Err(invalid(format!("Discord rejected the activity: {}", reply["data"])));
                }
                return Ok(());
            }
        }
    }
}

fn eof_closed(error: io::Error) -> io::Error {
    if error.kind() == io::ErrorKind::UnexpectedEof { closed("RPC socket closed") } else { error }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::thread;

    fn read_frame(sock: &mut UnixStream) -> (u32, Value) {
        let mut header = [0u8; 8];
        sock.read_exact(&mut header).unwrap();
        let size = u32::from_le_bytes(header[4..].try_into().unwrap());
        let mut body = vec![0; size as usize];
        sock.read_exact(&mut body).unwrap();
        (u32::from_le_bytes(header[..4].try_into().unwrap()), serde_json::from_slice(&body).unwrap())
    }

    fn write_frame(sock: &mut UnixStream, opcode: u32, data: Value) {
        let payload = serde_json::to_vec(&data).unwrap();
        sock.write_all(&opcode.to_le_bytes()).unwrap();
        sock.write_all(&(payload.len() as u32).to_le_bytes()).unwrap();
        sock.write_all(&payload).unwrap();
    }

    #[test]
    fn talks_to_a_fake_discord() {
        let dir = std::env::temp_dir().join(format!("cs2-presence-test-{}", std::process::id()));
        fs::create_dir_all(dir.join("app/com.discordapp.Discord")).unwrap();
        let path = dir.join("app/com.discordapp.Discord/discord-ipc-0");
        let listener = UnixListener::bind(&path).unwrap();
        assert_eq!(socket_paths(&dir), std::slice::from_ref(&path));
        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let (op, hello) = read_frame(&mut sock);
            assert_eq!((op, hello["client_id"].as_str()), (HANDSHAKE, Some(CLIENT_ID)));
            write_frame(&mut sock, FRAME, json!({"cmd": "DISPATCH", "evt": "READY"}));
            let (op, request) = read_frame(&mut sock);
            assert_eq!(op, FRAME);
            assert_eq!(request["args"]["activity"]["details"], "hi");
            write_frame(&mut sock, PING, json!({"x": 1}));
            assert_eq!(read_frame(&mut sock), (PONG, json!({"x": 1})));
            write_frame(&mut sock, FRAME, json!({"nonce": "other"}));
            write_frame(&mut sock, FRAME, json!({"nonce": request["nonce"], "evt": null}));
            let (_, request) = read_frame(&mut sock);
            write_frame(&mut sock, FRAME, json!({"nonce": request["nonce"], "evt": "ERROR", "data": "bad"}));
        });
        let mut rpc = Rpc::open(&path).unwrap();
        rpc.set_activity(42, Some(&json!({"details": "hi"}))).unwrap();
        let error = rpc.set_activity(42, None).unwrap_err();
        assert!(error.to_string().contains("rejected"));
        server.join().unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
}
