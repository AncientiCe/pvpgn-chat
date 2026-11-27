//! A logged-in connection: connects, authenticates, then streams parsed
//! events from a background reader thread.

use std::io::{BufRead, BufReader, Cursor, Read};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::connect::Connect;
use crate::protocol::{parse_line, ServerEvent};

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Credentials {
    pub server: String,
    pub user: String,
    pub password: String,
}

#[derive(Debug)]
pub struct Session {
    conn: Connect,
    username: String,
}

/// Resolve `host:port`, accepting both IPs and host names.
fn resolve(server: &str) -> Result<SocketAddr, String> {
    server
        .to_socket_addrs()
        .map_err(|e| format!("Invalid server address: {e}"))?
        .next()
        .ok_or_else(|| format!("Could not resolve {server}"))
}

impl Session {
    /// Connect and log in. Blocks, so call it off the UI thread.
    ///
    /// Every server event after login is passed to `on_event`, on a background
    /// thread, in order. The last event is always [`ServerEvent::Disconnected`].
    pub fn connect<F>(cred: &Credentials, on_event: F) -> Result<Session, String>
    where
        F: Fn(ServerEvent) + Send + 'static,
    {
        let addr = resolve(&cred.server)?;
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
            .map_err(|e| format!("Connection failed: {e}"))?;

        let mut conn = Connect::new(stream);
        let backlog = conn
            .connect(&cred.user, &cred.password)
            .map_err(|e| format!("Authentication failed: {e}"))?;
        let pending = conn.take_pending();
        let read_stream = conn
            .get_stream_clone()
            .map_err(|e| format!("Failed to clone stream: {e}"))?;

        // Lines that arrived during login go first, so order is preserved.
        for line in backlog.iter().filter(|l| !l.is_empty()) {
            on_event(parse_line(line));
        }

        thread::Builder::new()
            .name("bnet-reader".into())
            .spawn(move || read_loop(Cursor::new(pending.into_bytes()).chain(read_stream), on_event))
            .map_err(|e| format!("Failed to start reader: {e}"))?;

        info!("Session established for {}", cred.user);
        Ok(Session { conn, username: cred.user.clone() })
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    /// Send one line (chat text or a `/command`).
    pub fn send(&mut self, line: &str) -> Result<(), String> {
        self.conn.send(line).map_err(|e| format!("Send failed: {e}"))
    }

    /// Close the socket; the reader thread then ends with `Disconnected`.
    pub fn close(&mut self) {
        if let Err(e) = self.conn.close() {
            warn!("Error closing connection: {e}");
        }
    }
}

/// Blocking line reader. Lines can span TCP reads, so buffer until `\n`.
fn read_loop<R: Read, F: Fn(ServerEvent)>(stream: R, on_event: F) {
    let mut reader = BufReader::new(stream);
    let mut buf = Vec::with_capacity(512);
    let reason = loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => break "Connection closed by server".to_string(),
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf);
                let line = line.trim_end_matches(['\r', '\n']);
                if !line.is_empty() {
                    on_event(parse_line(line));
                }
            }
            Err(e) => break format!("Read error: {e}"),
        }
    };
    on_event(ServerEvent::Disconnected { reason });
}
