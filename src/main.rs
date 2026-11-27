#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

mod chat_view;
mod login;
mod theme;

use std::sync::mpsc::{channel, Receiver};
use std::thread;

use bnetchat::protocol::ServerEvent;
use bnetchat::session::{Credentials, Session};
use eframe::egui;
use tracing::{error, info};

use crate::chat_view::ChatView;
use crate::theme::{AppTheme, MessageStyle};

fn main() {
    tracing_subscriber::fmt().with_max_level(tracing::Level::WARN).init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([500.0, 400.0]),
        ..Default::default()
    };
    let _ = eframe::run_native("Bnet chat", options, Box::new(|_cc| Ok(Box::new(App::default()))));
}

/// Which screen is open.
enum View {
    Login(login::Login),
    Connecting,
    Chat(Box<ChatView>),
}

type ConnectResult = Result<(Session, Receiver<ServerEvent>), String>;

struct App {
    view: View,
    theme: AppTheme,
    message_style: MessageStyle,
    is_dark_theme: bool,
    connection_receiver: Option<Receiver<ConnectResult>>,
}

impl Default for App {
    fn default() -> Self {
        let is_dark_theme = load_theme_preference();
        let theme = if is_dark_theme { AppTheme::dark() } else { AppTheme::light() };
        Self {
            view: View::Login(login::Login::default()),
            message_style: MessageStyle::for_theme(&theme),
            theme,
            is_dark_theme,
            connection_receiver: None,
        }
    }
}

fn load_theme_preference() -> bool {
    std::fs::read_to_string("theme.json")
        .ok()
        .and_then(|s| serde_json::from_str::<bool>(&s).ok())
        .unwrap_or(false)
}

impl App {
    fn save_theme_preference(&self) {
        let _ = std::fs::write("theme.json", serde_json::to_string(&self.is_dark_theme).unwrap_or_default());
    }

    fn set_theme(&mut self, dark: bool) {
        self.is_dark_theme = dark;
        self.theme = if dark { AppTheme::dark() } else { AppTheme::light() };
        self.message_style = MessageStyle::for_theme(&self.theme);
        if let View::Chat(chat) = &mut self.view {
            chat.message_style = self.message_style.clone();
        }
        self.save_theme_preference();
    }

    /// Connect on a background thread. Incoming events wake the UI so new
    /// messages show up immediately, not on the next mouse move.
    fn start_connection(&mut self, cred: Credentials, ctx: &egui::Context) {
        let (result_tx, result_rx) = channel();
        self.connection_receiver = Some(result_rx);
        self.view = View::Connecting;
        let ctx = ctx.clone();

        thread::spawn(move || {
            if let Ok(json) = serde_json::to_string_pretty(&cred) {
                let _ = std::fs::write("credentials.json", json);
            }
            let (event_tx, event_rx) = channel();
            let wake = ctx.clone();
            let result = Session::connect(&cred, move |event| {
                let _ = event_tx.send(event);
                wake.request_repaint();
            })
            .map(|session| (session, event_rx));
            let _ = result_tx.send(result);
            ctx.request_repaint();
        });
    }

    fn poll_connection(&mut self) {
        let Some(rx) = &self.connection_receiver else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.connection_receiver = None;
        match result {
            Ok((session, events)) => {
                info!("Connected");
                self.view = View::Chat(Box::new(ChatView::new(session, events, self.message_style.clone())));
            }
            Err(err) => {
                error!("Connection failed: {err}");
                self.view = View::Login(login::Login {
                    error: Some(format!("Connection failed: {err}")),
                    ..Default::default()
                });
            }
        }
    }

    fn show_connection_splash(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                ui.heading("BNetChat");
                ui.add_space(30.0);
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .corner_radius(10.0)
                    .inner_margin(40.0)
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.heading("Connecting to server...");
                            ui.add_space(20.0);
                            ui.add(egui::Spinner::new().size(40.0).color(self.theme.primary));
                            ui.add_space(20.0);
                            ui.small("Please wait while we establish connection...");
                        });
                    });
            });
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.style_mut(|style| self.theme.apply_to_style(style));
        self.poll_connection();

        let mut start = None;
        let mut logout = false;
        let mut dark = self.is_dark_theme;

        match &mut self.view {
            View::Login(login) => {
                if login.update(ctx) {
                    start = Some(Credentials {
                        server: login.server.clone(),
                        user: login.user.clone(),
                        password: login.password.clone(),
                    });
                }
            }
            View::Connecting => self.show_connection_splash(ctx),
            View::Chat(chat) => {
                if chat.update(ctx, &mut dark) {
                    chat.close();
                    logout = true;
                }
            }
        }

        if dark != self.is_dark_theme {
            self.set_theme(dark);
        }
        if logout {
            self.view = View::Login(login::Login::default());
        }
        if let Some(cred) = start {
            self.start_connection(cred, ctx);
        }
    }
}
