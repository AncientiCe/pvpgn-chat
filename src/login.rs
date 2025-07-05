use eframe::egui::{self, Color32, TextEdit, Button};
use serde::{Deserialize, Serialize};
use crate::Credentials;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Login {
    pub server: String,
    pub user: String,
    #[serde(skip)]
    pub password: String,
    pub error: Option<String>,
    #[serde(skip)]
    pub remember_me: bool,
    #[serde(skip)]
    pub show_password: bool,
    #[serde(skip)]
    pub is_loading: bool,
    #[serde(skip)]
    pub server_error: Option<String>,
    #[serde(skip)]
    pub user_error: Option<String>,
    #[serde(skip)]
    pub password_error: Option<String>,
}

impl Default for Login {
    fn default() -> Self {
        let mut login = Login {
            server: "".to_string(),
            user: "".to_string(),
            password: "".to_string(),
            error: None,
            remember_me: false,
            show_password: false,
            is_loading: false,
            server_error: None,
            user_error: None,
            password_error: None,
        };
        if let Ok(text) = std::fs::read_to_string("credentials.json") {

                // Parse the string into a dynamically-typed JSON structure.
            let credentials = serde_json::from_str::<Credentials>(&text).unwrap();
            login.user.push_str(&credentials.user);
            login.server.push_str(&credentials.server);
            login.password.push_str(&credentials.password);
        };

        login
    }
}

impl Login {
    pub fn update(&mut self, ctx: &egui::Context) -> bool {
        let mut update = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);

                // Modern centered login form
                ui.allocate_ui_with_layout(
                    egui::vec2(400.0, 500.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        // Application title
                        ui.add_space(20.0);
                        ui.heading("BNetChat");
                        ui.add_space(30.0);

                        // Login form container with modern styling
                        egui::Frame::new()
                            .fill(ui.visuals().panel_fill)
                            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                            .corner_radius(10.0)
                            .inner_margin(30.0)
                            .show(ui, |ui| {
                                ui.vertical_centered(|ui| {
                                    ui.heading("Sign In");
                                    ui.add_space(20.0);

                                    // General error message
                                    if let Some(ref error) = self.error {
                                        ui.colored_label(Color32::from_rgb(220, 53, 69), format!("⚠ {}", error));
                                        ui.add_space(10.0);
                                    }

                                    // Server input with validation
                                    ui.label("Server Address");
                                    let server_response = ui.add(
                                        TextEdit::singleline(&mut self.server)
                                            .hint_text("localhost:6112")
                                            .desired_width(300.0)
                                    );

                                    // Auto-focus on first input field
                                    if server_response.has_focus() || self.server.is_empty() {
                                        server_response.request_focus();
                                    }

                                    if let Some(ref error) = self.server_error {
                                        ui.colored_label(Color32::from_rgb(220, 53, 69), error);
                                    }
                                    ui.add_space(10.0);

                                    // Username input with validation
                                    ui.label("Username");
                                    ui.add(
                                        TextEdit::singleline(&mut self.user)
                                            .hint_text("Enter your username")
                                            .desired_width(300.0)
                                    );
                                    if let Some(ref error) = self.user_error {
                                        ui.colored_label(Color32::from_rgb(220, 53, 69), error);
                                    }
                                    ui.add_space(10.0);

                                    // Password input with visibility toggle
                                    ui.horizontal(|ui| {
                                        ui.label("Password");
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.small_button(if self.show_password { "🙈" } else { "👁" }).clicked() {
                                                self.show_password = !self.show_password;
                                            }
                                        });
                                    });

                                    let password_response = ui.add(
                                        TextEdit::singleline(&mut self.password)
                                            .password(!self.show_password)
                                            .hint_text("Enter your password")
                                            .desired_width(300.0)
                                    );

                                    if let Some(ref error) = self.password_error {
                                        ui.colored_label(Color32::from_rgb(220, 53, 69), error);
                                    }
                                    ui.add_space(15.0);

                                    // Remember me checkbox
                                    ui.checkbox(&mut self.remember_me, "Remember me");
                                    ui.add_space(20.0);

                                    // Login button with loading state
                                    let login_button = if self.is_loading {
                                        ui.add_enabled(false, Button::new("🔄 Connecting...").min_size(egui::vec2(300.0, 40.0)))
                                    } else {
                                        ui.add(Button::new("Sign In").min_size(egui::vec2(300.0, 40.0)))
                                    };

                                    // Handle Enter key for login
                                    if (password_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || login_button.clicked() {
                                        if self.validate_form() {
                                            self.is_loading = true;
                                            update = true;
                                        }
                                    }

                                    ui.add_space(10.0);
                                    ui.small("Press Enter to sign in");
                                });
                            });
                    },
                );
            });
        });

        update
    }

    fn validate_form(&mut self) -> bool {
        let mut is_valid = true;

        // Clear previous errors
        self.server_error = None;
        self.user_error = None;
        self.password_error = None;

        // Validate server
        if self.server.trim().is_empty() {
            self.server_error = Some("Server address is required".to_string());
            is_valid = false;
        } else if !self.server.contains(':') {
            self.server_error = Some("Server address must include port (e.g., localhost:6112)".to_string());
            is_valid = false;
        } else {
            // Test server connection
            match self.test_server_connection() {
                Ok(_) => {
                    // Connection successful
                }
                Err(err) => {
                    self.server_error = Some(format!("Cannot connect to server: {}", err));
                    is_valid = false;
                }
            }
        }

        // Validate username
        if self.user.trim().is_empty() {
            self.user_error = Some("Username is required".to_string());
            is_valid = false;
        } else if self.user.len() < 2 {
            self.user_error = Some("Username must be at least 2 characters".to_string());
            is_valid = false;
        }

        // Validate password
        if self.password.trim().is_empty() {
            self.password_error = Some("Password is required".to_string());
            is_valid = false;
        }

        is_valid
    }

    fn test_server_connection(&self) -> Result<(), String> {
        let host: SocketAddr = self.server.parse()
            .map_err(|_| "Invalid server address format".to_string())?;

        let timeout = Duration::from_secs(3);
        TcpStream::connect_timeout(&host, timeout)
            .map_err(|e| format!("Connection failed: {}", e))?;

        Ok(())
    }
}
