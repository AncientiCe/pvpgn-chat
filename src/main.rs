use std::io::{BufRead, BufReader};
use std::net::{TcpStream, SocketAddr};
use std::time::{Instant,Duration};

use std::io::{Read, Write};
use dotenv::dotenv;
use std::{env, io, thread};

struct Connect<'a> {
    stream: &'a TcpStream,
    errno: i32,
    errstr: String,
    reader: BufReader<&'a TcpStream>,
    message_codes_map: HashMap<&'a str, &'a str>,
    users: HashSet<String>
}

impl <'a>Connect<'a> {
    fn new(stream: &'a TcpStream, errno: i32, errstr: String, reader: BufReader<&'a TcpStream>) -> Connect<'a> {
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
            ("1027", "TALK"),
            ("1028", "BROADCAST"),
            ("1029", "CHANNEL_FULL"),
            ("1030", "CHANNEL_DOES_NOT_EXIST"),
            ("1031", "CHANNEL_RESTRICTED"),
            ("1032", "INFO_DIALOG"),
            ("1033", "ERROR_DIALOG"),
            ("1034", "AVAILABLE_USERS"),
            ("1035", "WHISPER_FAILED"),
            ("1036", "WHISPER_SENT"),
            ("1037", "FILE_RECV"),
            ("1038", "FILE_SEND"),
            ("1039", "FILE_RECV_SEND_FAILED"),
            ("1040", "FILE_RECV_SEND_DENIED"),
            ("1041", "FILE_RESUMING"),
            ("1042", "FILE_CANCELLED"),
            ("1043", "FILE_ABORTED"),
            ("1044", "FILE_FINISHED"),
        ];

        let message_codes_map: HashMap<&str, &str> = message_codes.iter().cloned().collect();
        let users = HashSet::new();
        Connect {stream, errno, errstr, reader, message_codes_map, users}
    }
    fn waitfor(&mut self, wait_s: &str) {
        let mut buffer = [0; 1024];
        let mut s: &str = "";
        while !s.contains(wait_s) {
            let n = self.stream.read(&mut buffer).unwrap();
            s = std::str::from_utf8(&buffer[..n]).expect("Found invalid utf-8");
        }
    }
    fn connect(&mut self, username: &str, password: &str) -> i32 {
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

        let mut buffer = [0; 1024];
        loop {
            let n = self.stream.read(&mut buffer).unwrap();
            let s = std::str::from_utf8(&buffer[..n]).expect("Found invalid utf-8");
            println!("Read {} bytes: {:?}", n, s);
            self.stream.write_all(format!("{}\r\n", "/join w3").as_bytes()).unwrap();
            self.parse_message(s);
        }
    }
    fn parse_message(&mut self, message: &str) {


        let lines = message.split("\r\n");

        for line in lines {
            if line.is_empty() {
                continue;
            }

            let mut parts = line.split(" ");
            let code = parts.next().unwrap();
            let message_type = self.message_codes_map.get(code).unwrap_or(&"UNKNOWN");

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
                    println!("{} has joined the channel", user)
                },
                "LEAVE" => {
                    let user = parts.next().unwrap();
                    self.users.remove(user);
                    println!("{} has left the channel", user)
                },
                "WHISPER" => {
                    let from = parts.next().unwrap();
                    let _ = parts.next(); // Skip the "to" part
                    println!("{} whispers: {}", from, parts.collect::<Vec<_>>().join(" "))
                }
                "TALK" => {
                    let from = parts.next().unwrap();
                    println!("{}: {}", from, parts.collect::<Vec<_>>().join(" "))
                }
                "BROADCAST" => println!("Broadcast: {}", parts.collect::<Vec<_>>().join(" ")),
                "ERROR" | "UNKNOWN" | "INFO" => println!("{}: {}", message_type, parts.collect::<Vec<_>>().join(" ")),
                _ => {}
            }
        }
    }
}

use std::collections::{HashMap, HashSet};

fn main() {
    dotenv().ok();
    let host = env::var("BNET_SERVER").unwrap();
    let username = env::var("BNET_USER").unwrap();
    let password = env::var("BNET_PASSWORD").unwrap();

    let timeout = 2;
    println!("Connecting chat... {}", host);
    let timeout_initial = timeout as u64;
    let timeout = std::time::Duration::from_secs(timeout_initial);
    let host: SocketAddr = host
        .parse()
        .expect("Unable to parse socket address");
    let mut stream = match TcpStream::connect_timeout(&host, timeout) {
        Ok(s) => s,
        Err(_) => {
            println!("Socket error");
            panic!("Omg");
        }
    };
    let stream2 = stream.try_clone().expect("Could not clone stream");
    let handle = thread::spawn(move || {
        let reader = BufReader::new(&stream2);
        let mut connection = Connect::new(&stream2, 0, "".to_string(), reader);
        connection.connect(&username, &password);
    });

    // To have stdin
    loop {
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(n) => {

                stream.write_all(format!("{}\r\n", input).as_bytes()).unwrap();
            }
            Err(error) => println!("error: {}", error),
        }
    }
    handle.join().unwrap();
}

