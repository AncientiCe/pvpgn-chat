#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

mod login;
mod connect;

use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{channel, Receiver};

use std::thread;
use crate::connect::Connect;

use eframe::egui;
use serde::de::Unexpected::Str;
use crate::Connected::Done;

fn main() {
    // Log to stdout (if you run with `RUST_LOG=debug`).
    tracing_subscriber::fmt::init();

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(320.0, 240.0)),
        ..Default::default()
    };
    eframe::run_native(
        "My chat app",
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
    connection: Connect,
    response: Receiver<String>,
}

impl Main {
    fn new(p0: Connect, req_rx: Receiver<String>) -> Self {
        Self {
            message: "".to_string(),
            messages: vec![],
            connection: p0,
            response: req_rx,
        }

    }

    fn update(&mut self, ctx: &egui::Context) {
        if let Ok(response) = self.response.try_recv() {
            println!("Received on channel: {}", response);
            self.messages.push(response);
            true;
        }
        egui::SidePanel::right("sidebar_users").show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for x in self.connection.users.clone() {
                    ui.horizontal(|ui| {
                        ui.label(x);
                    });
                }
            });
        });
        egui::TopBottomPanel::bottom("actions").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.message);
                let button = egui::Button::new("Submit");
                if ui.add(button).clicked() { self.connection.send(self.message.clone()) }
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
}
enum Connected {
    Done(Credentials),
    None,
}

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
        let host: SocketAddr = cred.server
            .parse()
            .expect("Unable to parse socket address");
        let timeout = 2;
        println!("Connecting chat... {}", host);
        let timeout_initial = timeout as u64;
        let timeout = std::time::Duration::from_secs(timeout_initial);
        let mut stream = match TcpStream::connect_timeout(&host, timeout) {
            Ok(s) => s,
            Err(_) => {
                println!("Socket error");
                panic!("Omg");
            }
        };
        let stream2 = stream.try_clone().unwrap();
        let reader = BufReader::new(stream2);
        let mut connection = Connect::new(
            stream,
            reader
        );
        connection.connect(&cred.user, &cred.password);
        let (req_tx, req_rx) = channel();
        let mut connection2 = connection.clone();
        let handle = std::thread::spawn(move || {
            connection2.read(req_tx);
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