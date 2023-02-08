use std::net::{TcpStream};

use std::io::{BufWriter, Read, Write};

#[derive(Debug)]
pub struct Connect {
    stream: TcpStream,
    writer: BufWriter<TcpStream>,
}

impl Connect {
    pub(crate) fn new(stream: TcpStream) -> Connect {
        let stream_clone = stream.try_clone().unwrap();
        let writer = BufWriter::new(stream_clone);
        Connect {stream, writer}
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
        self.writer.write(msg.as_bytes()).unwrap();
        self.writer.flush().unwrap();
    }
}

