//! A tiny HTTP listener for CS2's Game State Integration posts.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::TOKEN;

const MAX_BODY: usize = 1024 * 1024;

/// The latest game state and when it arrived.
#[derive(Default)]
pub struct State {
    pub data: Option<Value>,
    pub at: Option<Instant>,
}

pub type Shared = Arc<Mutex<State>>;

/// Listen on 127.0.0.1:`port` in the background. Without the port, there are no match details.
pub fn start(port: u16, state: Shared) {
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => listener,
        Err(error) => {
            println!("Game State Integration off, port {port} unavailable: {error}");
            return;
        }
    };
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let state = state.clone();
            thread::spawn(move || {
                let _ = handle(stream, &state);
            });
        }
    });
}

fn handle(stream: TcpStream, state: &Shared) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let is_post = line.starts_with("POST ");
    let mut length = 0usize;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line.trim_end().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let status = if !is_post {
        "405 Method Not Allowed"
    } else if length > MAX_BODY {
        "413 Content Too Large"
    } else {
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        match accept(&body) {
            Some(data) => {
                let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                if state.data.is_none() {
                    println!("Receiving game state from CS2");
                }
                state.data = Some(data);
                state.at = Some(Instant::now());
                "200 OK"
            }
            None => "403 Forbidden",
        }
    };
    (&stream)
        .write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes())
}

/// The posted game state, if it is a JSON object carrying our token.
pub fn accept(body: &[u8]) -> Option<Value> {
    let data: Value = serde_json::from_slice(body).ok()?;
    (data.pointer("/auth/token")?.as_str()? == TOKEN).then_some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_the_token() {
        assert!(accept(br#"{"auth":{"token":"cs2-presence"},"map":{}}"#).is_some());
        assert!(accept(br#"{"auth":{"token":"other"}}"#).is_none());
        assert!(accept(br#"{"map":{}}"#).is_none());
        assert!(accept(b"not json").is_none());
    }

    #[test]
    fn serves_posts() {
        let state = Shared::default();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        start(port, state.clone());
        let post = |body: &str| {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(stream, "POST / HTTP/1.1\r\nHost: x\r\ncontent-length: {}\r\n\r\n{body}", body.len())
                .unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            reply
        };
        assert!(post(r#"{"auth":{"token":"nope"}}"#).starts_with("HTTP/1.1 403"));
        assert!(state.lock().unwrap().data.is_none());
        assert!(
            post(r#"{"auth":{"token":"cs2-presence"},"map":{"name":"de_dust2"}}"#)
                .starts_with("HTTP/1.1 200")
        );
        let state = state.lock().unwrap();
        assert_eq!(state.data.as_ref().unwrap()["map"]["name"], "de_dust2");
        assert!(state.at.is_some());
    }
}
