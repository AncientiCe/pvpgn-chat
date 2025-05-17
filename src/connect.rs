use std::net::{TcpStream};

use std::io::{Read, Write};

#[derive(Debug)]
pub struct Connect {
    stream: TcpStream,
}

impl Connect {
    pub(crate) fn new(stream: TcpStream) -> Connect {
        stream.set_nodelay(true).unwrap();
        Connect {stream}
    }

    fn waitfor(&mut self, wait_s: &str) {
        let mut buffer = [0; 1024];
        let mut collected = String::new();
        while !collected.contains(wait_s) {
            let n = self.stream.read(&mut buffer).unwrap();
            let part = std::str::from_utf8(&buffer[..n]).expect("Found invalid utf-8");
            collected.push_str(part);
        }
    }

    pub(crate) fn connect(&mut self, username: &str, password: &str) {
        if username.is_empty() || password.is_empty() {
            return
        }

        self.stream.write_all(&[3]).unwrap();
        self.stream.flush().unwrap();
        println!("Sending username... {}", username);
        self.waitfor("Username:");
        self.stream.write_all(format!("{}\r\n", username).as_bytes()).unwrap();
        self.stream.flush().unwrap();
        println!("Sending password...");
        self.waitfor("Password:");
        self.stream.write_all(format!("{}\r\n", password).as_bytes()).unwrap();
        self.stream.flush().unwrap();
        self.stream.write_all(format!("{}\r\n", "/join w3").as_bytes()).unwrap();
        self.stream.flush().unwrap();
    }

    pub fn send(&mut self, msg: String) {
        // println!("Sending: {}", msg);
        self.stream.write_all(format!("{}\r\n", msg).as_bytes()).unwrap();
        self.stream.flush().unwrap();
    }
}

