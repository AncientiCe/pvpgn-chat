use std::io::{BufRead, BufReader};
use std::net::{TcpStream, SocketAddr};
use std::time::{Instant,Duration};

use std::io::{Read, Write};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

#[derive(Debug)]
pub struct Connect {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    message_codes_map: HashMap<String, String>,
    pub(crate) users: HashSet<String>
}

impl Clone for Connect {
    fn clone(&self) -> Self {
        Self {
            stream: self.stream.try_clone().unwrap(),
            reader: BufReader::new(self.stream.try_clone().unwrap()),
            message_codes_map: self.message_codes_map.clone(),
            users: self.users.clone()
        }
    }
}

impl Connect {
    pub(crate) fn new(stream: TcpStream, reader: BufReader<TcpStream>) -> Connect {
        // These are fucked up
        let message_codes = [
            ("1001", "USER"),
            ("1007", "CHANNEL"),
            ("1009", "USER"),
            ("1018", "INFO"),
            ("1019", "ERROR"),
            ("1020", "STATS"),
            ("1021", "INGAME"),
            ("1022", "LOGGED_IN"),
            ("1023", "LOGGED_OUT"),
            ("1002", "JOIN"),
            ("1003", "LEAVE"),
            ("1004", "WHISPER"),
        ];

        let message_codes_map: HashMap<String, String> = HashMap::from_iter(message_codes.iter().map(|(k,v)| (k.to_string(), v.to_string())));
        let users = HashSet::new();

        Connect {stream, reader, message_codes_map, users}
    }

    fn waitfor(&mut self, wait_s: &str) {
        let mut buffer = [0; 1024];
        let mut s: &str = "";
        while !s.contains(wait_s) {
            let n = self.stream.read(&mut buffer).unwrap();
            s = std::str::from_utf8(&buffer[..n]).expect("Found invalid utf-8");
        }
    }
    pub(crate) fn connect(&mut self, username: &str, password: &str) -> i32 {
        if username.is_empty() || password.is_empty() {
            return -1;
        }


        self.stream.write_all(&[3]).unwrap();

        println!("Sending username... {}", username);
        self.waitfor("Username:");
        self.stream.write_all(format!("{}\r\n", username).as_bytes()).unwrap();

        println!("Sending password...");
        self.waitfor("Password:");
        self.stream.write_all(format!("{}\r\n", password).as_bytes()).unwrap();

        self.stream.write_all(format!("{}\r\n", "/join w3").as_bytes()).unwrap();
        return 0;
    }

    pub fn send(&mut self, msg: String) {
        println!("Sending: {}", msg);
        self.stream.write_all(msg.as_bytes()).unwrap();
    }

    pub(crate) fn read(&mut self, req_tx: Sender<String>) {
        let mut buffer = [0; 1024];
        loop {
            let n = self.stream.read(&mut buffer).unwrap();
            let s = std::str::from_utf8(&buffer[..n]).expect("Found invalid utf-8");
            // println!("Read {} bytes: {:?}", n, s);
            let lines = s.split("\r\n");
            for line in lines {
                if line.is_empty() {
                    continue;
                }

                let mut parts = line.split(" ");
                let code = parts.next().unwrap();
                let x1 = &"UNKNOWN".to_string();
                let message_type = match self.message_codes_map.get(code) {
                    Some(x) => x,
                    _ => x1
                };

                // skip text type as we validate on code
                parts.next().unwrap();
                match message_type.as_ref() {
                    "USER" => {
                        let user = parts.next().unwrap();
                        self.users.insert(user.to_string().to_owned());
                    }
                    "JOIN" => {
                        let user = parts.next().unwrap();
                        self.users.insert(user.to_string().to_owned());
                        req_tx.send(format!("{} has joined the channel", user));
                    },
                    "LEAVE" => {
                        let user = parts.next().unwrap();
                        self.users.remove(user);
                        req_tx.send(format!("{} has left the channel", user));
                    },
                    "WHISPER" => {
                        let from = parts.next().unwrap();
                        let _ = parts.next(); // Skip the "to" part
                        req_tx.send(format!("{} whispers: {}", from, parts.collect::<Vec<_>>().join(" ")));
                    }
                    "TALK" => {
                        let from = parts.next().unwrap();
                        req_tx.send(format!("{}: {}", from, parts.collect::<Vec<_>>().join(" ")));
                    }
                    "BROADCAST" => {
                        req_tx.send(format!("Broadcast: {}", parts.collect::<Vec<_>>().join(" ")));
                    }
                    "ERROR" | "UNKNOWN" | "INFO" => {
                        req_tx.send(format!("{}: {}", message_type, parts.collect::<Vec<_>>().join(" ")));
                    },
                    _ => {}
                }
            }
        }
    }
}

