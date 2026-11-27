//! A logged-in connection: connects, authenticates, then streams parsed
//! events from a background reader thread.

use std::io::{BufRead, BufReader, Cursor, Read};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::connect::Connect;
use crate::protocol::{parse_line, ServerEvent};

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Credentials {
    pub server: String,
    pub user: String,
    pub password: String,
}

/// File the last successful login is remembered in (next to the working dir).
const CREDENTIALS_FILE: &str = "credentials.json";

impl Credentials {
    /// The last login that worked, if any.
    pub fn load() -> Option<Credentials> {
        let text = std::fs::read_to_string(CREDENTIALS_FILE).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Remember this login. Call only after it succeeded, so a typo never
    /// replaces a working saved password.
    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            if let Err(e) = std::fs::write(CREDENTIALS_FILE, json) {
                warn!("Could not save credentials: {e}");
            }
        }
    }
}

#[derive(Debug)]
pub struct Session {
    conn: Connect,
    username: String,
    closed: bool,
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
            debug!("<< {line}");
            on_event(parse_line(line));
        }

        thread::Builder::new()
            .name("bnet-reader".into())
            .spawn(move || read_loop(Cursor::new(pending.into_bytes()).chain(read_stream), on_event))
            .map_err(|e| format!("Failed to start reader: {e}"))?;

        info!("Session established for {}", cred.user);
        Ok(Session { conn, username: cred.user.clone(), closed: false })
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    /// Send one line (chat text or a `/command`).
    pub fn send(&mut self, line: &str) -> Result<(), String> {
        debug!(">> {line}");
        self.conn.send(line).map_err(|e| format!("Send failed: {e}"))
    }

    /// Log out with `/quit`, then close the socket; the reader thread then
    /// ends with `Disconnected`. Without the `/quit` the server can keep the
    /// account logged in for a while and refuse the next login. Safe to call
    /// more than once; also runs on drop.
    pub fn close(&mut self) {
        if std::mem::replace(&mut self.closed, true) {
            return;
        }
        info!("Logging out {}", self.username);
        let _ = self.conn.send("/quit");
        if let Err(e) = self.conn.close() {
            warn!("Error closing connection: {e}");
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
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
                    debug!("<< {line}");
                    on_event(parse_line(line));
                }
            }
            Err(e) => break format!("Read error: {e}"),
        }
    };
    info!("Reader stopped: {reason}");
    on_event(ServerEvent::Disconnected { reason });
}
