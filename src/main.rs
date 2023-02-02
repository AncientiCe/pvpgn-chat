use std::io::{BufRead, BufReader};
use std::net::{TcpStream, SocketAddr};
use std::time::{Instant,Duration};

use std::io::{Read, Write};
use dotenv::dotenv;
use std::env;

struct Connect<'a> {
    stream: &'a TcpStream,
    errno: i32,
    errstr: String,
    reader: BufReader<&'a TcpStream>,
}

impl <'a>Connect<'a> {
    fn new(stream: &'a TcpStream, errno: i32, errstr: String, reader: BufReader<&'a TcpStream>) -> Connect<'a> {
        Connect {stream, errno, errstr, reader}
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
        }

        // if buffer.contains(&2010) {
        //     println!("Bot logged in");
        //     return 0;
        // } else {
        //     println!("Error with bot login");
        //     println!("{:?}", buffer);
        //     self.stream.shutdown(std::net::Shutdown::Both);
        //     return -1;
        // }
    }
}

fn main() {
    dotenv().ok();
    let host = env::var("BNET_SERVER").unwrap();
    let username = env::var("BNET_USER").unwrap();
    let password = env::var("BNET_PASSWORD").unwrap();

    let timeout = 2;
    println!("Connecting bot... {}", host);
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
    let reader = BufReader::new(&stream);
    let mut connection = Connect::new(&stream, 0, "".to_string(), reader);
    match connection.connect( &username, &password) {
        0 => println!("Connected successfully"),
        -1 => println!("Error: empty username or password"),
        -2 => println!("Error: socket error"),
        -3 => println!("Error: socket already opened"),
        _ => println!("Unknown error"),
    }
}

