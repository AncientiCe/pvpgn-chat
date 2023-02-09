#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

mod login;
mod connect;

use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};

use crate::connect::Connect;

use eframe::egui;
use serde::{Deserialize, Serialize};
use crate::Connected::Done;

fn main() {
    // Log to stdout (if you run with `RUST_LOG=debug`).
    tracing_subscriber::fmt::init();

    let options = eframe::NativeOptions {
        // initial_window_size: Some(egui::vec2(320.0, 240.0)),
        ..Default::default()
    };
    eframe::run_native(
        "Bnet chat",
        options,
        Box::new(|_cc| Box::new(MyApp::default())),
    );
}

/// Which view is currectly open
#[derive(Debug)]
pub enum View {
    Login(login::Login),
    Main(Main)
}
impl Default for View {
    fn default() -> Self {
        View::Login(login::Login::default())
    }
}

#[derive(Debug, Default)]
struct MyApp {
    view: View,
}

#[derive(Debug)]
pub struct Main {
    message: String,
    messages: Vec<String>,
    stream: Connect,
    users: HashSet<String>,
    response: Receiver<String>,
    message_codes: HashMap<String, String>,
}

impl Main {
    fn new(stream: Connect, req_rx: Receiver<String>) -> Self {
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
            ("1010", "WHISPER_TO"),
        ];

        let message_codes_map: HashMap<String, String> = HashMap::from_iter(message_codes.iter().map(|(k,v)| (k.to_string(), v.to_string())));
        Self {
            message: "".to_string(),
            messages: vec![],
            stream,
            users: HashSet::new(),
            response: req_rx,
            message_codes: message_codes_map,
        }

    }

    fn update(&mut self, ctx: &egui::Context) {
        if let Ok(response) = self.response.try_recv() {
            self.parse_message(response);
        }
        egui::SidePanel::right("sidebar_users").show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for x in self.users.clone() {
                    ui.horizontal(|ui| {
                        ui.label(x);
                    });
                }
            });
        });
        egui::TopBottomPanel::bottom("actions").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let input = ui.text_edit_singleline(&mut self.message);
                if input.lost_focus() && input.ctx.input().key_pressed(egui::Key::Enter) {
                    self.send_input();
                }
                let button = egui::Button::new("Submit");
                if ui.add(button).clicked() {
                    self.send_input();
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            for x in self.messages.clone() {
                ui.horizontal(|ui| {
                    ui.label(x);
                });
            }
        });
    }

    fn send_input(&mut self) {
        self.send(self.message.clone());
        self.messages.push(format!("You: {}", self.message));
        self.message.clear();
    }

    pub fn send(&mut self, msg: String) {
        self.stream.send(msg);
    }

    fn parse_message(&mut self, line: String) {
        let mut parts = line.split(" ");
        let code = parts.next().unwrap();
        let x1 = &"UNKNOWN".to_string();
        let message_type = match self.message_codes.get(code) {
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
                self.messages.push(format!("{} has joined the channel", user));
            },
            "LEAVE" => {
                let user = parts.next().unwrap();
                self.users.remove(user);
                self.messages.push(format!("{} has left the channel", user));
            },
            "WHISPER" => {
                let from = parts.next().unwrap();
                let _ = parts.next(); // Skip the "to" part
                self.messages.push(format!("{} whispers: {}", from, parts.collect::<Vec<_>>().join(" ")));
            }
            "WHISPER_TO" => {
                let from = parts.next().unwrap();
                let _ = parts.next(); // Skip the "to" part
                self.messages.push(format!("You whisper {}: {}", from, parts.collect::<Vec<_>>().join(" ")));
            }
            "TALK" => {
                let from = parts.next().unwrap();
                self.messages.push(format!("{}: {}", from, parts.collect::<Vec<_>>().join(" ")));
            }
            "BROADCAST" => {
                self.messages.push(format!("Broadcast: {}", parts.collect::<Vec<_>>().join(" ")));
            }
            "ERROR" | "UNKNOWN" | "INFO" => {
                self.messages.push(format!("{}: {}", message_type, parts.collect::<Vec<_>>().join(" ")));
            },
            _ => {}
        }
    }
}

fn read(mut stream: TcpStream, req_tx: Sender<String>) {
    let mut buffer = [0; 1024];
    loop {
        let n = stream.read(&mut buffer).unwrap();
        let s = std::str::from_utf8(&buffer[..n]).expect("Found invalid utf-8");
        println!("Read {} bytes: {:?}", n, s);
        let lines = s.split("\r\n");
        for line in lines {
            if line.is_empty() {
                continue;
            }
            println!("{}", line.to_string());
            req_tx.send(line.to_string());
        }
    }
}

enum Connected {
    Done(Credentials),
    None,
}

#[derive(Serialize, Deserialize, Default)]
struct Credentials {
    server: String,
    user: String,
    password: String,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let credentials = match self.view {
            View::Login(ref mut login) => {
                if login.update(ctx)
                {
                    let connected = Done(Credentials {
                        server: login.server.to_string(),
                        user: login.user.to_string(),
                        password: login.password.to_string()
                    });
                    connected
                } else {
                    Connected::None
                }
            }
            View::Main(ref mut view) => {
                view.update(ctx);
                Connected::None
            }
        };
        if let Done(cred) = credentials {
            self.view.make_main(cred);
        }
    }
}

impl View {
    fn make_main(&mut self, cred: Credentials) -> &mut Main {
        std::fs::write(
            "credentials.json",
            serde_json::to_string_pretty(&cred).unwrap(),
        )
            .unwrap();
        let host: SocketAddr = cred.server
            .parse()
            .expect("Unable to parse socket address");
        let timeout = 2;
        println!("Connecting chat... {}", host);
        let timeout_initial = timeout as u64;
        let timeout = std::time::Duration::from_secs(timeout_initial);
        let stream = match TcpStream::connect_timeout(&host, timeout) {
            Ok(s) => s,
            Err(_) => {
                println!("Socket error");
                panic!("Omg");
            }
        };
        let stream3 = stream.try_clone().unwrap();
        let mut connection = Connect::new(
            stream3
        );
        connection.connect(&cred.user, &cred.password);
        let (req_tx, req_rx) = channel();
        let handle = std::thread::spawn(move || {
            read(stream, req_tx);
        });

        let view = Main::new(connection, req_rx);
        *self = View::Main(view);
        match *self {
            View::Main(ref mut main) => {
                main
            },
            _ => unreachable!(),
        }
    }
}