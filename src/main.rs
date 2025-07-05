#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

mod login;
mod connect;
mod theme;

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};
use std::thread;
use tracing::{info, debug, error, warn};

use crate::connect::Connect;
use crate::theme::{AppTheme, MessageStyle};

use eframe::egui;
use eframe::egui::{Label, Sense};
use serde::{Deserialize, Serialize};
use crate::Connected::Done;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([500.0, 400.0]),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "Bnet chat",
        options,
        Box::new(|_cc| Ok(Box::new(MyApp::default())))
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

#[derive(Debug)]
struct MyApp {
    view: View,
    theme: AppTheme,
    message_style: MessageStyle,
    is_dark_theme: bool,
    connection_state: ConnectionState,
    connection_retry_count: u32,
    connection_error: Option<String>,
    connection_receiver: Option<Receiver<Result<(Connect, Receiver<String>), String>>>,
}

#[derive(Debug, Clone)]
enum ConnectionState {
    Idle,
    Connecting,
    Connected,
    Failed,
}

impl Default for MyApp {
    fn default() -> Self {
        // Load theme preference from file
        let is_dark_theme = Self::load_theme_preference();
        let theme = if is_dark_theme {
            AppTheme::dark()
        } else {
            AppTheme::light()
        };
        let message_style = MessageStyle::for_theme(&theme);
        Self {
            view: View::default(),
            theme,
            message_style,
            is_dark_theme,
            connection_state: ConnectionState::Idle,
            connection_retry_count: 0,
            connection_error: None,
            connection_receiver: None,
        }
    }
}

impl MyApp {
    #[allow(dead_code)]
    fn toggle_theme(&mut self) {
        self.is_dark_theme = !self.is_dark_theme;
        self.theme = if self.is_dark_theme {
            AppTheme::dark()
        } else {
            AppTheme::light()
        };
        self.message_style = MessageStyle::for_theme(&self.theme);
    }

    fn load_theme_preference() -> bool {
        if let Ok(content) = std::fs::read_to_string("theme.json") {
            if let Ok(is_dark) = serde_json::from_str::<bool>(&content) {
                return is_dark;
            }
        }
        false // Default to light theme
    }

    fn save_theme_preference(&self) {
        let _ = std::fs::write(
            "theme.json",
            serde_json::to_string(&self.is_dark_theme).unwrap_or_default(),
        );
    }

    fn start_background_connection(&mut self, cred: Credentials) {
        let (tx, rx) = channel();
        self.connection_receiver = Some(rx);
        let message_style = self.message_style.clone();

        thread::spawn(move || {
            let result = Self::try_connect_background(cred, message_style);
            let _ = tx.send(result);
        });
    }

    fn try_connect_background(cred: Credentials, _message_style: MessageStyle) -> Result<(Connect, Receiver<String>), String> {
        // Save credentials
        if let Err(e) = std::fs::write(
            "credentials.json",
            serde_json::to_string_pretty(&cred).unwrap(),
        ) {
            return Err(format!("Failed to save credentials: {}", e));
        }

        // Parse server address
        let host: SocketAddr = cred.server
            .parse()
            .map_err(|e| format!("Invalid server address: {}", e))?;

        // Connect with timeout
        let timeout = std::time::Duration::from_secs(5);
        let stream = TcpStream::connect_timeout(&host, timeout)
            .map_err(|e| format!("Connection failed: {}", e))?;

        // Initialize connection using the main stream
        let mut connection = Connect::new(stream);
        info!("About to call connection.connect()...");
        let extra_messages = connection.connect(&cred.user, &cred.password)
            .map_err(|e| format!("Authentication failed: {}", e))?;

        info!("Connection.connect() returned successfully with {} extra messages!", extra_messages.len());

        // After successful connection, clone stream for reading thread
        let stream_for_read = connection.get_stream_clone()
            .map_err(|e| format!("Failed to clone stream: {}", e))?;

        info!("Stream cloned successfully, starting read thread...");

        // Set up message channel
        let (req_tx, req_rx) = channel();

        // Send any extra messages that were collected during connection to the message channel
        let req_tx_clone = req_tx.clone();
        if !extra_messages.is_empty() {
            info!("Injecting {} extra messages into message channel", extra_messages.len());
            thread::spawn(move || {
                for message in extra_messages {
                    if !message.is_empty() {
                        debug!("Injecting message: {}", message);
                        if req_tx_clone.send(message).is_err() {
                            break;
                        }
                    }
                }
            });
        }

        thread::spawn(move || {
            read(stream_for_read, req_tx);
        });

        info!("Background connection setup complete, returning success!");
        Ok((connection, req_rx))
    }

    fn show_connection_splash(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);

                // Application logo/title
                ui.heading("BNetChat");
                ui.add_space(30.0);

                // Connection status container
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .corner_radius(10.0)
                    .inner_margin(40.0)
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            // Connection status message
                            let status_text = match self.connection_state {
                                ConnectionState::Connecting => "🔄 Connecting to server...",
                                _ => "🔄 Connecting...",
                            };

                            ui.heading(status_text);
                            ui.add_space(20.0);

                            // Animated loading spinner
                            let time = ui.input(|i| i.time);
                            let spinner_angle = time as f32 * 2.0;
                            let spinner_size = 40.0;
                            let (rect, _) = ui.allocate_exact_size(
                                egui::Vec2::splat(spinner_size), 
                                egui::Sense::hover()
                            );

                            // Draw spinning circle background
                            ui.painter().circle_stroke(
                                rect.center(),
                                spinner_size / 2.0 - 2.0,
                                egui::Stroke::new(3.0, self.theme.primary.gamma_multiply(0.3))
                            );

                            // Draw spinning arc that rotates
                            let arc_length = std::f32::consts::PI;
                            let center = rect.center();
                            let radius = spinner_size / 2.0 - 2.0;

                            // Create points for the rotating arc
                            let mut arc_points = Vec::new();
                            let segments = 20;
                            for i in 0..=segments {
                                let t = i as f32 / segments as f32;
                                let angle = spinner_angle + arc_length * t;
                                let pos = center + egui::Vec2::angled(angle) * radius;
                                arc_points.push(pos);
                            }

                            // Draw the rotating arc using PathShape::line
                            let path = egui::epaint::PathShape::line(
                                arc_points,
                                egui::Stroke::new(3.0, self.theme.primary)
                            );
                            ui.painter().add(egui::epaint::Shape::Path(path));

                            ui.add_space(20.0);

                            // Progress information
                            if let Some(ref error) = self.connection_error {
                                ui.colored_label(
                                    egui::Color32::from_rgb(220, 53, 69), 
                                    format!("⚠ {}", error)
                                );
                                ui.add_space(10.0);
                            }

                            ui.small("Please wait while we establish connection...");
                        });
                    });
            });
        });

        // Request repaint for animation
        ctx.request_repaint();
    }
}

#[derive(Debug)]
pub struct Main {
    message: String,
    messages: Vec<(String, String, u64)>, // (message_type, content, timestamp)
    stream: Connect,
    users: HashSet<String>,
    response: Receiver<String>,
    message_codes: HashMap<String, String>,
    message_style: MessageStyle,
    user_info: HashMap<String, UserInfo>,
    // Responsive design
    sidebar_visible: bool,
    // Performance optimization
    max_messages: usize,
    last_render_time: std::time::Instant,
    // User interactions
    friends: HashSet<String>,
    ignored_users: HashSet<String>,
    pending_confirmation: Option<(String, String)>, // (action, username)
    // Message quoting/reply functionality
    quoted_message: Option<(String, String, u64)>, // (message_type, content, timestamp)
    // Channel information
    current_channel: Option<String>,
    channel_topic: Option<String>,
    // Logout functionality
    logout_requested: bool,
}

#[derive(Debug, Clone)]
struct UserInfo {
    role: UserRole,
}


#[derive(Debug, Clone, PartialEq)]
enum UserRole {
    User,
    Moderator,
    Admin,
}

impl Main {
    fn get_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn get_user_role_from_code(username: &str, role_code: &str) -> UserRole {
        info!("Analyzing role for user: {} with role code: {}", username, role_code);

        // Role detection based on the role code from USER messages
        // Updated based on latest issue findings:
        // - 0003 and 0010 indicate admin roles
        // - 0000 indicates normal user
        // - Other non-zero codes might indicate special roles

        match role_code {
            "0003" | "0010" => {
                info!("Detected Admin role for user: {} (role code: {})", username, role_code);
                UserRole::Admin
            },
            "0001" | "0002" => {
                info!("Detected Moderator role for user: {} (role code: {})", username, role_code);
                UserRole::Moderator
            },
            "0000" | _ => {
                info!("Detected User role for user: {} (role code: {})", username, role_code);
                UserRole::User
            }
        }
    }

    fn get_user_role(username: &str) -> UserRole {
        // Fallback method for cases where role code is not available
        // This maintains backward compatibility for JOIN messages and other cases
        info!("Analyzing role for user: {} (fallback method)", username);
        UserRole::User
    }

    /// Parse and render rich text with formatting support (optimized for performance)
    fn render_rich_text(ui: &mut egui::Ui, text: &str, base_color: egui::Color32, base_style: egui::TextStyle) {
        // For performance, only do rich text processing for short messages
        // Long messages will use simple rendering to avoid UI freezing
        if text.len() > 200 {
            ui.label(egui::RichText::new(text)
                .text_style(base_style)
                .color(base_color));
            return;
        }

        // Simple emoticon replacement without expensive char vector allocation
        let mut processed_text = text.to_string();

        // Only process most common emoticons for performance
        let common_emoticons = [
            (":)", "😊"), (":(", "😢"), (":D", "😃"), (";)", "😉"),
            ("<3", "❤️"), ("</3", "💔"), (":P", "😛"), (":p", "😛"),
        ];

        for (emoticon, emoji) in &common_emoticons {
            processed_text = processed_text.replace(emoticon, emoji);
        }

        // Simple URL detection and replacement
        if processed_text.contains("http://") || processed_text.contains("https://") {
            // For URLs, just render as hyperlink if it's a simple case
            if processed_text.starts_with("http://") || processed_text.starts_with("https://") {
                if let Some(space_pos) = processed_text.find(' ') {
                    let url = &processed_text[..space_pos];
                    let rest = &processed_text[space_pos..];
                    ui.hyperlink_to(url, url);
                    if !rest.trim().is_empty() {
                        ui.label(egui::RichText::new(rest)
                            .text_style(base_style)
                            .color(base_color));
                    }
                    return;
                } else {
                    ui.hyperlink_to(&processed_text, &processed_text);
                    return;
                }
            }
        }

        // Simple bold/italic detection without complex parsing
        if processed_text.contains("**") && processed_text.matches("**").count() >= 2 {
            // Very basic bold processing for common cases
            if let Some(start) = processed_text.find("**") {
                if let Some(end) = processed_text[start + 2..].find("**") {
                    let before = &processed_text[..start];
                    let bold_text = &processed_text[start + 2..start + 2 + end];
                    let after = &processed_text[start + 4 + end..];

                    if !before.is_empty() {
                        ui.label(egui::RichText::new(before)
                            .text_style(base_style.clone())
                            .color(base_color));
                    }
                    ui.label(egui::RichText::new(bold_text)
                        .text_style(base_style.clone())
                        .color(base_color)
                        .strong());
                    if !after.is_empty() {
                        ui.label(egui::RichText::new(after)
                            .text_style(base_style)
                            .color(base_color));
                    }
                    return;
                }
            }
        }

        // Default: just render the processed text normally
        ui.label(egui::RichText::new(processed_text)
            .text_style(base_style)
            .color(base_color));
    }


    fn format_timestamp(timestamp: u64) -> String {
        // Simple time formatting - just show hours:minutes
        let secs_since_midnight = timestamp % (24 * 60 * 60);
        let hours = secs_since_midnight / 3600;
        let minutes = (secs_since_midnight % 3600) / 60;
        format!("{:02}:{:02}", hours, minutes)
    }

    fn get_user_initials(username: &str) -> String {
        let parts: Vec<&str> = username.split_whitespace().collect();
        if parts.len() >= 2 {
            format!("{}{}", 
                parts[0].chars().next().unwrap_or('?').to_uppercase(),
                parts[1].chars().next().unwrap_or('?').to_uppercase()
            )
        } else if let Some(first_char) = username.chars().next() {
            first_char.to_uppercase().collect()
        } else {
            "?".to_string()
        }
    }

    fn get_user_avatar_color(username: &str) -> egui::Color32 {
        // Generate a consistent color based on username hash
        let mut hash: u32 = 0;
        for byte in username.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
        }

        // Define a set of pleasant colors for avatars
        let colors = [
            egui::Color32::from_rgb(239, 68, 68),   // Red
            egui::Color32::from_rgb(245, 101, 101), // Light Red
            egui::Color32::from_rgb(251, 146, 60),  // Orange
            egui::Color32::from_rgb(251, 191, 36),  // Yellow
            egui::Color32::from_rgb(34, 197, 94),   // Green
            egui::Color32::from_rgb(16, 185, 129),  // Emerald
            egui::Color32::from_rgb(6, 182, 212),   // Cyan
            egui::Color32::from_rgb(59, 130, 246),  // Blue
            egui::Color32::from_rgb(99, 102, 241),  // Indigo
            egui::Color32::from_rgb(139, 92, 246),  // Violet
            egui::Color32::from_rgb(168, 85, 247),  // Purple
            egui::Color32::from_rgb(236, 72, 153),  // Pink
        ];

        colors[(hash as usize) % colors.len()]
    }

    fn add_message(&mut self, message_type: String, content: String, timestamp: u64) {
        self.messages.push((message_type, content, timestamp));

        // Limit messages to prevent memory issues
        if self.messages.len() > self.max_messages {
            // Remove oldest messages, keeping the most recent ones
            let excess = self.messages.len() - self.max_messages;
            self.messages.drain(0..excess);
        }
    }


    fn new(stream: Connect, req_rx: Receiver<String>, message_style: MessageStyle) -> Self {
        // These are fucked up
        let message_codes = [
            ("1001", "USER"),
            ("1007", "CHANNEL"),
            ("1009", "USER"),
            ("1018", "INFO"),
            ("1019", "ERROR"),
            ("1020", "STATS"),
            ("1005", "TALK"),
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
            message_style,
            user_info: HashMap::new(),
            sidebar_visible: true,
            max_messages: 1000, // Limit to 1000 messages for performance
            last_render_time: std::time::Instant::now(),
            friends: HashSet::new(),
            ignored_users: HashSet::new(),
            pending_confirmation: None,
            quoted_message: None,
            current_channel: None,
            channel_topic: None,
            logout_requested: false,
        }

    }

    fn update(&mut self, ctx: &egui::Context, theme_toggle: &mut bool) -> bool {
        if let Ok(response) = self.response.try_recv() {
            self.parse_message(response);
        }

        // Handle keyboard shortcuts
        ctx.input(|i| {
            // Ctrl+U: Toggle sidebar
            if i.key_pressed(egui::Key::U) && i.modifiers.ctrl {
                self.sidebar_visible = !self.sidebar_visible;
            }

            // Ctrl+T: Toggle theme
            if i.key_pressed(egui::Key::T) && i.modifiers.ctrl {
                *theme_toggle = !*theme_toggle;
            }


            // F5: Refresh/clear messages (with confirmation)
            if i.key_pressed(egui::Key::F5) {
                self.pending_confirmation = Some(("clear_messages".to_string(), "all messages".to_string()));
            }

            // Escape: Close any open dialogs and clear quotes
            if i.key_pressed(egui::Key::Escape) {
                self.pending_confirmation = None;
                self.quoted_message = None;
            }
        });

        // Responsive design: auto-collapse sidebar on smaller windows
        let window_width = ctx.screen_rect().width();
        if window_width < 700.0 && self.sidebar_visible {
            self.sidebar_visible = false;
        } else if window_width >= 800.0 && !self.sidebar_visible {
            self.sidebar_visible = true;
        }

        // Responsive text scaling based on window size
        ctx.style_mut(|style| {
            let scale_factor = if window_width < 600.0 {
                0.85 // Smaller text for very small windows
            } else if window_width < 800.0 {
                0.95 // Slightly smaller text for medium windows
            } else {
                1.0 // Normal text for larger windows
            };

            // Apply scaling to text styles
            for (_, font_id) in style.text_styles.iter_mut() {
                font_id.size = (font_id.size * scale_factor).max(8.0); // Minimum font size of 8
            }
        });

        // Handle confirmation dialogs
        if let Some((action, username)) = &self.pending_confirmation.clone() {
            egui::Window::new("Confirm Action")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);

                        match action.as_str() {
                            "ignore" => {
                                ui.label(egui::RichText::new("⚠️ Ignore User")
                                    .color(self.message_style.error_color)
                                    .size(16.0)
                                    .strong());
                                ui.add_space(8.0);
                                ui.label(format!("Are you sure you want to ignore {}?", username));
                                ui.small("You will no longer see messages from this user.");
                            }
                            "clear_messages" => {
                                ui.label(egui::RichText::new("🗑️ Clear Messages")
                                    .color(self.message_style.error_color)
                                    .size(16.0)
                                    .strong());
                                ui.add_space(8.0);
                                ui.label("Are you sure you want to clear all chat messages?");
                                ui.small("This action cannot be undone.");
                            }
                            _ => {
                                ui.label("Confirm this action?");
                            }
                        }

                        ui.add_space(12.0);

                        ui.horizontal(|ui| {
                            let confirm_button = egui::Button::new("✅ Confirm")
                                .fill(self.message_style.error_color.gamma_multiply(0.1))
                                .stroke(egui::Stroke::new(1.0, self.message_style.error_color));
                            if ui.add(confirm_button).clicked() {
                                match action.as_str() {
                                    "ignore" => {
                                        self.ignored_users.insert(username.clone());
                                    }
                                    "clear_messages" => {
                                        self.messages.clear();
                                    }
                                    _ => {}
                                }
                                self.pending_confirmation = None;
                            }

                            ui.add_space(8.0);

                            let cancel_button = egui::Button::new("❌ Cancel")
                                .fill(self.message_style.system_color.gamma_multiply(0.1))
                                .stroke(egui::Stroke::new(1.0, self.message_style.system_color));
                            if ui.add(cancel_button).clicked() {
                                self.pending_confirmation = None;
                            }
                        });

                        ui.add_space(8.0);
                    });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let input_id = ui.make_persistent_id("input_text_id");

            // Top panel with theme toggle and controls - always visible
            egui::TopBottomPanel::top("header").show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);

                    // App title/logo
                    ui.label(egui::RichText::new("💬 Bnet Chat")
                        .color(self.message_style.user_color)
                        .size(16.0)
                        .strong());

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);

                        // Logout button
                        let logout_button = egui::Button::new("🚪")
                            .min_size([32.0, 32.0].into())
                            .fill(self.message_style.error_color.gamma_multiply(0.1))
                            .stroke(egui::Stroke::new(1.0, self.message_style.error_color.gamma_multiply(0.3)));
                        if ui.add(logout_button).on_hover_text("Logout").clicked() {
                            self.logout_requested = true;
                        }

                        // Theme toggle button - always visible
                        let theme_button_text = if *theme_toggle { "🌙" } else { "☀" };
                        let theme_button = egui::Button::new(theme_button_text)
                            .min_size([32.0, 32.0].into())
                            .fill(self.message_style.system_color.gamma_multiply(0.1))
                            .stroke(egui::Stroke::new(1.0, self.message_style.system_color.gamma_multiply(0.3)));
                        if ui.add(theme_button).on_hover_text("Toggle theme (Ctrl+T)").clicked() {
                            *theme_toggle = !*theme_toggle;
                        }


                        // Connection status indicator
                        ui.label("🟢").on_hover_text("Connected to server");
                    });
                });
                ui.add_space(4.0);
                ui.separator();
            });

            egui::TopBottomPanel::bottom("actions").show(ctx, |ui| {
                ui.add_space(8.0);

                // Show quoted message if any
                if let Some((_quoted_type, quoted_content, quoted_timestamp)) = self.quoted_message.clone() {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);

                        // Quote indicator
                        ui.label(egui::RichText::new("📝")
                            .color(self.message_style.system_color)
                            .size(16.0));

                        // Quoted message preview
                        egui::Frame::new()
                            .fill(self.message_style.system_color.gamma_multiply(0.1))
                            .corner_radius(4.0)
                            .inner_margin(6.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Quoting:")
                                        .color(self.message_style.system_color)
                                        .size(10.0));
                                    ui.label(egui::RichText::new(format!("[{}]", Self::format_timestamp(quoted_timestamp)))
                                        .color(self.message_style.system_color.gamma_multiply(0.7))
                                        .size(10.0));
                                });

                                // Truncate long messages for preview
                                let preview_content = if quoted_content.len() > 50 {
                                    format!("{}...", &quoted_content[..50])
                                } else {
                                    quoted_content.clone()
                                };

                                ui.label(egui::RichText::new(preview_content)
                                    .color(self.message_style.user_color)
                                    .size(11.0)
                                    .italics());
                            });

                        // Clear quote button
                        let clear_button = egui::Button::new("❌")
                            .min_size([20.0, 20.0].into())
                            .fill(self.message_style.error_color.gamma_multiply(0.1))
                            .stroke(egui::Stroke::new(1.0, self.message_style.error_color.gamma_multiply(0.3)));
                        if ui.add(clear_button).on_hover_text("Clear quote").clicked() {
                            self.quoted_message = None;
                        }

                        ui.add_space(8.0);
                    });
                    ui.add_space(4.0);
                }

                ui.horizontal(|ui| {
                    ui.add_space(8.0);

                    // Character limit for messages
                    const MAX_MESSAGE_LENGTH: usize = 500;
                    let char_count = self.message.len();
                    let is_over_limit = char_count > MAX_MESSAGE_LENGTH;

                    let input = ui.add_sized(
                        [ui.available_width() - 90.0, 60.0],
                        egui::TextEdit::multiline(&mut self.message)
                            .id(input_id)
                            .hint_text("Type your message... (max 500 chars)\nPress Ctrl+Enter to send")
                            .char_limit(MAX_MESSAGE_LENGTH)
                            .desired_rows(2)
                    );

                    // Show character count and typing indicator
                    ui.horizontal(|ui| {
                        // Show typing indicator when user is actively typing
                        if input.has_focus() && !self.message.is_empty() {
                            ui.ctx().request_repaint();
                            ui.small("💭");
                        }

                        // Show character count with color feedback
                        let count_color = if is_over_limit {
                            self.message_style.error_color
                        } else if char_count > MAX_MESSAGE_LENGTH * 3 / 4 {
                            self.message_style.whisper_color // Warning color
                        } else {
                            self.message_style.system_color.gamma_multiply(0.7)
                        };

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.small(egui::RichText::new(format!("{}/{}", char_count, MAX_MESSAGE_LENGTH))
                                .color(count_color));
                        });
                    });

                    // Handle Ctrl+Enter for sending messages
                    if input.has_focus() && input.ctx.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl) {
                        self.send_input();
                    }

                    // Keep focus on input after losing focus (unless clicking elsewhere)
                    if input.lost_focus() && !input.ctx.input(|i| i.pointer.any_click()) {
                        input.request_focus();
                    }

                    ui.add_space(4.0);
                    let button = egui::Button::new("📤 Send")
                        .min_size([80.0, 28.0].into())
                        .fill(self.message_style.user_color.gamma_multiply(0.1))
                        .stroke(egui::Stroke::new(1.0, self.message_style.user_color.gamma_multiply(0.5)));
                    let button_response = ui.add(button);

                    // Add visual feedback for button interaction
                    if button_response.clicked() {
                        self.send_input();
                    }

                    // Show visual feedback when button is pressed
                    if button_response.is_pointer_button_down_on() {
                        ui.ctx().request_repaint();
                    }
                    ui.add_space(8.0);
                });
                ui.add_space(8.0);
            });
            // Conditionally show sidebar based on visibility state
            if self.sidebar_visible {
                egui::SidePanel::right("sidebar_users")
                    .min_width(180.0)
                    .max_width(300.0)
                    .default_width(220.0)
                    .resizable(true)
                    .show(ctx, |ui| {
                ui.add_space(8.0);

                // Header with user count and controls
                ui.horizontal(|ui| {
                    ui.vertical_centered_justified(|ui| {
                        ui.heading(format!("👥 Users ({})", self.users.len()));
                    });
                    // Enhanced connection status indicator
                    ui.label("🟢").on_hover_text("Connected to server");
                });
                ui.add_space(4.0);

                ui.separator();
                ui.add_space(4.0);

                // User list
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut users: Vec<String> = self.users.iter().cloned().collect();
                    users.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

                    for user_name in &users {
                        let user_info = self.user_info.get(user_name).cloned().unwrap_or_else(|| {
                            // Create default user info if not exists
                            UserInfo {
                                role: UserRole::User,
                            }
                        });

                        // Enhanced user item with better styling
                        egui::Frame::new()
                            .fill(ui.visuals().widgets.noninteractive.weak_bg_fill)
                            .corner_radius(4.0)
                            .inner_margin(6.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // User avatar - circular with initials
                                    let avatar_size = 24.0;
                                    let initials = Self::get_user_initials(user_name);
                                    let avatar_color = Self::get_user_avatar_color(user_name);

                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::Vec2::splat(avatar_size), 
                                        egui::Sense::hover()
                                    );

                                    // Draw circular avatar background
                                    ui.painter().circle_filled(
                                        rect.center(),
                                        avatar_size / 2.0,
                                        avatar_color
                                    );

                                    // Draw initials in the center
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        &initials,
                                        egui::FontId::proportional(10.0),
                                        egui::Color32::WHITE
                                    );

                                    ui.add_space(4.0);

                                    // Role indicator
                                    let role_icon = match user_info.role {
                                        UserRole::Admin => "👑",
                                        UserRole::Moderator => "🛡️",
                                        UserRole::User => "",
                                    };
                                    if !role_icon.is_empty() {
                                        ui.label(role_icon);
                                    }

                                    // User name with enhanced styling
                                    let user_name_clone = user_name.clone();
                                    let response = ui.add(
                                        Label::new(egui::RichText::new(user_name.as_str())
                                            .color(self.message_style.user_color))
                                            .sense(Sense::click())
                                    );

                                    // Enhanced hover tooltip with user information and context menu
                                    response.on_hover_ui(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(format!("👤 {}", user_name));
                                            ui.separator();
                                            ui.label(format!("Role: {}", match user_info.role {
                                                UserRole::Admin => "Administrator",
                                                UserRole::Moderator => "Moderator",
                                                UserRole::User => "User",
                                            }));
                                        });
                                    }).context_menu(|ui| {
                                        // Enhanced context menu with better styling
                                        ui.style_mut().spacing.button_padding = [8.0, 4.0].into();
                                        ui.style_mut().spacing.item_spacing = [4.0, 2.0].into();

                                        // Header with user info
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("👤")
                                                .color(self.message_style.user_color)
                                                .size(16.0));
                                            ui.label(egui::RichText::new(&user_name_clone)
                                                .color(self.message_style.user_color)
                                                .strong());
                                        });
                                        ui.separator();

                                        // Communication actions
                                        ui.label(egui::RichText::new("💬 Communication")
                                            .color(self.message_style.system_color)
                                            .size(11.0));

                                        let whisper_button = egui::Button::new("💬 Send Whisper")
                                            .fill(self.message_style.whisper_color.gamma_multiply(0.1))
                                            .stroke(egui::Stroke::new(1.0, self.message_style.whisper_color.gamma_multiply(0.3)));
                                        if ui.add(whisper_button).clicked() {
                                            self.message = format!("/w {} ", user_name_clone);
                                            ctx.memory_mut(|mem| mem.request_focus(input_id));
                                            ui.close_menu();
                                        }

                                        let ping_button = egui::Button::new("📍 Ping User")
                                            .fill(self.message_style.system_color.gamma_multiply(0.1))
                                            .stroke(egui::Stroke::new(1.0, self.message_style.system_color.gamma_multiply(0.3)));
                                        if ui.add(ping_button).clicked() {
                                            self.message = format!("/ping {}", user_name_clone);
                                            ctx.memory_mut(|mem| mem.request_focus(input_id));
                                            self.send_input();
                                            ui.close_menu();
                                        }

                                        ui.separator();

                                        // Monitoring actions
                                        ui.label(egui::RichText::new("👁️ Monitoring")
                                            .color(self.message_style.system_color)
                                            .size(11.0));

                                        let watch_button = egui::Button::new("👁️ Watch User")
                                            .fill(self.message_style.join_leave_color.gamma_multiply(0.1))
                                            .stroke(egui::Stroke::new(1.0, self.message_style.join_leave_color.gamma_multiply(0.3)));
                                        if ui.add(watch_button).clicked() {
                                            self.message = format!("/watch {}", user_name_clone);
                                            ctx.memory_mut(|mem| mem.request_focus(input_id));
                                            self.send_input();
                                            ui.close_menu();
                                        }

                                        let unwatch_button = egui::Button::new("👁️‍🗨️ Unwatch User")
                                            .fill(self.message_style.system_color.gamma_multiply(0.1))
                                            .stroke(egui::Stroke::new(1.0, self.message_style.system_color.gamma_multiply(0.3)));
                                        if ui.add(unwatch_button).clicked() {
                                            self.message = format!("/unwatch {}", user_name_clone);
                                            ctx.memory_mut(|mem| mem.request_focus(input_id));
                                            self.send_input();
                                            ui.close_menu();
                                        }

                                        ui.separator();

                                        // Social actions
                                        ui.label(egui::RichText::new("👥 Social")
                                            .color(self.message_style.system_color)
                                            .size(11.0));

                                        let is_friend = self.friends.contains(&user_name_clone);
                                        let friend_button_text = if is_friend { "💔 Remove Friend" } else { "❤️ Add Friend" };
                                        let friend_button_color = if is_friend { 
                                            self.message_style.error_color 
                                        } else { 
                                            self.message_style.join_leave_color 
                                        };

                                        let friend_button = egui::Button::new(friend_button_text)
                                            .fill(friend_button_color.gamma_multiply(0.1))
                                            .stroke(egui::Stroke::new(1.0, friend_button_color.gamma_multiply(0.3)));
                                        if ui.add(friend_button).clicked() {
                                            if is_friend {
                                                self.friends.remove(&user_name_clone);
                                            } else {
                                                self.friends.insert(user_name_clone.clone());
                                            }
                                            ui.close_menu();
                                        }

                                        let is_ignored = self.ignored_users.contains(&user_name_clone);
                                        let ignore_button_text = if is_ignored { "🔊 Unignore User" } else { "🔇 Ignore User" };
                                        let ignore_button_color = if is_ignored { 
                                            self.message_style.join_leave_color 
                                        } else { 
                                            self.message_style.error_color 
                                        };

                                        let ignore_button = egui::Button::new(ignore_button_text)
                                            .fill(ignore_button_color.gamma_multiply(0.1))
                                            .stroke(egui::Stroke::new(1.0, ignore_button_color.gamma_multiply(0.3)));
                                        if ui.add(ignore_button).clicked() {
                                            if is_ignored {
                                                self.ignored_users.remove(&user_name_clone);
                                            } else {
                                                // Show confirmation for ignore action
                                                self.pending_confirmation = Some(("ignore".to_string(), user_name_clone.clone()));
                                            }
                                            ui.close_menu();
                                        }

                                    });
                                });
                            });
                        ui.add_space(2.0);
                    }

                });
            });
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                // Chat header
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("💬 Chat Messages"));
                    });
                });
                ui.separator();

                // Channel information display
                if let (Some(channel), Some(topic)) = (&self.current_channel, &self.channel_topic) {
                    ui.add_space(4.0);
                    egui::Frame::new()
                        .fill(ui.visuals().panel_fill.gamma_multiply(0.8))
                        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                        .corner_radius(4.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("📺 {}", channel))
                                    .strong()
                                    .color(self.message_style.user_color));
                                ui.separator();
                                ui.label(egui::RichText::new(topic)
                                    .italics()
                                    .color(self.message_style.system_color));
                            });
                        });
                    ui.add_space(4.0);
                } else if let Some(channel) = &self.current_channel {
                    ui.add_space(4.0);
                    egui::Frame::new()
                        .fill(ui.visuals().panel_fill.gamma_multiply(0.8))
                        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                        .corner_radius(4.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(format!("📺 {}", channel))
                                .strong()
                                .color(self.message_style.user_color));
                        });
                    ui.add_space(4.0);
                }

                ui.add_space(8.0);

                // Message count display
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small(format!("Messages: {}", self.messages.len()));
                    });
                });
                ui.add_space(4.0);

                // Create scroll area
                egui::ScrollArea::vertical()
                    .max_width(f32::INFINITY)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                    ui.add_space(4.0);

                    // Performance monitoring
                    let render_start = std::time::Instant::now();

                    // Optimize by avoiding clone and limiting rendered messages
                    let messages_to_render = if self.messages.len() > 500 {
                        // Only render last 500 messages for performance
                        &self.messages[self.messages.len() - 500..]
                    } else {
                        &self.messages[..]
                    };

                    for (index, (message_type, content, timestamp)) in messages_to_render.iter().enumerate() {
                        // Add alternating background colors for better message separation
                        let bg_color = if index % 2 == 0 {
                            self.message_style.system_color.gamma_multiply(0.05)
                        } else {
                            egui::Color32::TRANSPARENT
                        };

                        ui.horizontal_wrapped(|ui| {
                            // Add background color
                            ui.painter().rect_filled(
                                ui.available_rect_before_wrap(),
                                0.0,
                                bg_color,
                            );

                            // Add horizontal spacing
                            ui.add_space(8.0);

                            // Add timestamp
                            ui.label(egui::RichText::new(format!("[{}]", Self::format_timestamp(*timestamp)))
                                .text_style(egui::TextStyle::Name("chat_system".into()))
                                .color(self.message_style.system_color.gamma_multiply(0.7)));
                            ui.add_space(4.0);

                            // Check for mentions or keywords for highlighting
                            let is_mention = content.contains("You") || content.contains("whisper");
                            let highlight_color = if is_mention {
                                self.message_style.whisper_color
                            } else {
                                egui::Color32::TRANSPARENT
                            };

                            // Add highlight background for mentions
                            if is_mention {
                                ui.painter().rect_filled(
                                    ui.available_rect_before_wrap(),
                                    2.0,
                                    highlight_color.gamma_multiply(0.1),
                                );
                            }

                            // Use different styling based on message type with rich text formatting support
                            ui.horizontal_wrapped(|ui| {
                                match message_type.as_str() {
                                    "user" => {
                                        let color = if is_mention {
                                            self.message_style.whisper_color // Use whisper color for mentions
                                        } else {
                                            self.message_style.user_color
                                        };
                                        Self::render_rich_text(ui, content, color, egui::TextStyle::Name("chat_message".into()));
                                    }
                                    "system" => {
                                        // For system messages, add italic styling to the base text
                                        ui.label(egui::RichText::new("ℹ️ ")
                                            .text_style(egui::TextStyle::Name("chat_system".into()))
                                            .color(self.message_style.system_color));
                                        Self::render_rich_text(ui, content, self.message_style.system_color, egui::TextStyle::Name("chat_system".into()));
                                    }
                                    "whisper" => {
                                        // Add whisper icon and use rich text
                                        ui.label(egui::RichText::new("🗣️ ")
                                            .text_style(egui::TextStyle::Name("chat_whisper".into()))
                                            .color(self.message_style.whisper_color));
                                        Self::render_rich_text(ui, content, self.message_style.whisper_color, egui::TextStyle::Name("chat_whisper".into()));
                                    }
                                    "join_leave" => {
                                        // Add join/leave icon
                                        ui.label(egui::RichText::new("👋 ")
                                            .text_style(egui::TextStyle::Name("chat_system".into()))
                                            .color(self.message_style.join_leave_color));
                                        Self::render_rich_text(ui, content, self.message_style.join_leave_color, egui::TextStyle::Name("chat_system".into()));
                                    }
                                    "error" => {
                                        // Add error icon
                                        ui.label(egui::RichText::new("❌ ")
                                            .text_style(egui::TextStyle::Name("chat_system".into()))
                                            .color(self.message_style.error_color));
                                        Self::render_rich_text(ui, content, self.message_style.error_color, egui::TextStyle::Name("chat_system".into()));
                                    }
                                    _ => {
                                        Self::render_rich_text(ui, content, self.message_style.user_color, egui::TextStyle::Name("chat_message".into()));
                                    }
                                }
                            });
                        }).response.context_menu(|ui| {
                            // Message context menu for quoting/replying
                            ui.style_mut().spacing.button_padding = [8.0, 4.0].into();
                            ui.style_mut().spacing.item_spacing = [4.0, 2.0].into();

                            // Header
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("💬")
                                    .color(self.message_style.user_color)
                                    .size(16.0));
                                ui.label(egui::RichText::new("Message Actions")
                                    .color(self.message_style.user_color)
                                    .strong());
                            });
                            ui.separator();

                            // Quote message button
                            let quote_button = egui::Button::new("📝 Quote Message")
                                .fill(self.message_style.system_color.gamma_multiply(0.1))
                                .stroke(egui::Stroke::new(1.0, self.message_style.system_color.gamma_multiply(0.3)));
                            if ui.add(quote_button).clicked() {
                                self.quoted_message = Some((message_type.clone(), content.clone(), *timestamp));
                                ui.close_menu();
                            }

                            // Reply to message button
                            let reply_button = egui::Button::new("↩️ Reply to Message")
                                .fill(self.message_style.whisper_color.gamma_multiply(0.1))
                                .stroke(egui::Stroke::new(1.0, self.message_style.whisper_color.gamma_multiply(0.3)));
                            if ui.add(reply_button).clicked() {
                                self.quoted_message = Some((message_type.clone(), content.clone(), *timestamp));
                                // Pre-fill input with reply format
                                if let Some(username) = content.split(':').next() {
                                    self.message = format!("@{} ", username.trim());
                                }
                                ui.close_menu();
                            }

                            ui.separator();

                            // Copy message button
                            let copy_button = egui::Button::new("📋 Copy Message")
                                .fill(self.message_style.join_leave_color.gamma_multiply(0.1))
                                .stroke(egui::Stroke::new(1.0, self.message_style.join_leave_color.gamma_multiply(0.3)));
                            if ui.add(copy_button).clicked() {
                                ui.ctx().copy_text(content.clone());
                                ui.close_menu();
                            }
                        });
                        // Add vertical spacing between messages
                        ui.add_space(2.0);
                    }

                    // Performance monitoring - log render time if significant
                    let render_duration = render_start.elapsed();
                    if render_duration.as_millis() > 16 { // More than 16ms (60fps threshold)
                        warn!("Chat render took {}ms for {} messages", 
                                render_duration.as_millis(), messages_to_render.len());
                    }
                    self.last_render_time = std::time::Instant::now();

                    ui.add_space(4.0);
                });
            });
        });

        // Return logout status and reset the flag
        if self.logout_requested {
            self.logout_requested = false;
            true
        } else {
            false
        }
    }

    fn send_input(&mut self) {
        let mut final_message = self.message.clone();

        // If there's a quoted message, prepend it to the message
        if let Some((_quoted_type, quoted_content, quoted_timestamp)) = &self.quoted_message {
            let quote_prefix = format!("[Quote {}] {}: ", 
                Self::format_timestamp(*quoted_timestamp), 
                quoted_content.split(':').next().unwrap_or("Unknown").trim()
            );
            final_message = format!("{}{}", quote_prefix, final_message);
        }

        self.send(final_message.clone());
        // Add message status indicator for sent messages
        self.add_message("user".to_string(), format!("You: {} ✓", final_message), Self::get_timestamp());
        self.message.clear();

        // Clear quoted message after sending
        self.quoted_message = None;
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
        parts.next().unwrap_or("");
        let timestamp = Self::get_timestamp();
        match message_type.as_ref() {
            "USER" => {
                if let Some(user) = parts.next() {
                    info!("{}", line);

                    // Extract role code from USER message
                    if let Some(role_code) = parts.next() {
                        self.users.insert(user.to_string());

                        // Assign user info if not already exists
                        if !self.user_info.contains_key(user) {
                            let user_role = Self::get_user_role_from_code(user, role_code);

                            self.user_info.insert(user.to_string(), UserInfo {
                                role: user_role,
                            });
                        }
                    } else {
                        self.add_message("error".to_string(), format!("Malformed USER message (missing role code): {}", line), timestamp);
                    }
                } else {
                    self.add_message("error".to_string(), format!("Malformed USER message: {}", line), timestamp);
                }
            }
            "JOIN" => {
                let user = parts.next().unwrap();
                self.users.insert(user.to_string().to_owned());

                let user_role = Self::get_user_role(user);

                self.user_info.insert(user.to_string(), UserInfo {
                    role: user_role,
                });

                self.add_message("join_leave".to_string(), format!("{} has joined the channel", user), timestamp);
            },
            "LEAVE" => {
                let user = parts.next().unwrap();
                self.users.remove(user);
                self.add_message("join_leave".to_string(), format!("{} has left the channel", user), timestamp);
            },
            "WHISPER" => {
                let from = parts.next().unwrap();
                let _ = parts.next(); // Skip the "to" part
                self.add_message("whisper".to_string(), format!("{} whispers: {}", from, parts.collect::<Vec<_>>().join(" ")), timestamp);
            }
            "WHISPER_TO" => {
                let from = parts.next().unwrap();
                let _ = parts.next(); // Skip the "to" part
                self.add_message("whisper".to_string(), format!("You whisper {}: {}", from, parts.collect::<Vec<_>>().join(" ")), timestamp);
            }
            "TALK" => {
                let from = parts.next().unwrap();
                self.add_message("user".to_string(), format!("{}: {}", from, parts.collect::<Vec<_>>().join(" ")), timestamp);
            }
            "BROADCAST" => {
                self.add_message("system".to_string(), format!("Broadcast: {}", parts.collect::<Vec<_>>().join(" ")), timestamp);
            }
            "INFO" => {
                // Handle 1018 INFO messages for channel topics
                if code == "1018" {
                    let info_text = parts.collect::<Vec<_>>().join(" ");
                    // Check if this is a channel topic message
                    // Format: "W3 topic: You can now .pub and .gopub from channel, no need for whisper"
                    if let Some(topic_start) = info_text.find(" topic: ") {
                        let channel_name = info_text[..topic_start].trim();
                        let topic = info_text[topic_start + 8..].trim(); // Skip " topic: "

                        self.current_channel = Some(channel_name.to_string());
                        self.channel_topic = Some(topic.to_string());

                        info!("Channel topic updated - Channel: {}, Topic: {}", channel_name, topic);
                    } else {
                        // Other INFO messages
                        self.add_message("info".to_string(), format!("INFO: {}", info_text), timestamp);
                    }
                } else {
                    // Other INFO messages
                    let info_text = parts.collect::<Vec<_>>().join(" ");
                    self.add_message("info".to_string(), format!("INFO: {}", info_text), timestamp);
                }
            },
            "ERROR" | "UNKNOWN" => {
                self.add_message("error".to_string(), format!("{}: {}", message_type, parts.collect::<Vec<_>>().join(" ")), timestamp);
                self.add_message("error".to_string(), format!("Unknown: {}", line), timestamp);
            },
            "CHANNEL" => {
                self.users.clear();
            },
            _ => self.add_message("error".to_string(), format!("Unknown: {}", line), timestamp),
        }
    }
}

fn read(mut stream: TcpStream, req_tx: Sender<String>) {
    let mut buffer = [0; 1024];

    // Set stream to non-blocking mode to prevent hanging
    if let Err(e) = stream.set_nonblocking(true) {
        error!("Failed to set stream to non-blocking: {}", e);
        return;
    }

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                // Connection closed by server
                info!("Connection closed by server");
                let _ = req_tx.send("1019 ERROR Connection closed by server".to_string());
                break;
            }
            Ok(n) => {
                match std::str::from_utf8(&buffer[..n]) {
                    Ok(s) => {
                        // debug!("Read {} bytes: {:?}", n, s);
                        let lines = s.split("\r\n");
                        for line in lines {
                            if line.is_empty() {
                                continue;
                            }
                            // debug!("{}", line.to_string());
                            if req_tx.send(line.to_string()).is_err() {
                                // Main thread has disconnected, exit gracefully
                                info!("Main thread disconnected, stopping read loop");
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Invalid UTF-8 received: {}", e);
                        // Continue reading, don't break on invalid UTF-8
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No data available, sleep briefly and continue
                std::thread::sleep(std::time::Duration::from_millis(50));
                continue;
            }
            Err(e) => {
                error!("Read error: {}", e);
                let _ = req_tx.send(format!("1019 ERROR Read error: {}", e));
                break;
            }
        }
    }

    info!("Read thread exiting");
}

enum Connected {
    Done(Credentials),
    None,
}

#[derive(Serialize, Deserialize, Default, Debug)]
struct Credentials {
    server: String,
    user: String,
    password: String,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply theme to the context
        ctx.style_mut(|style| {
            self.theme.apply_to_style(style);
        });

        // Check if theme needs to be updated
        let mut theme_changed = false;

        // Handle background connection attempts FIRST, before showing splash screen
        if let Some(receiver) = &self.connection_receiver {
            if let Ok(result) = receiver.try_recv() {
                info!("Main thread received result from background connection!");
                self.connection_receiver = None;
                match result {
                    Ok((connection, req_rx)) => {
                        // Connection successful
                        info!("Background connection was successful! Transitioning to main view...");
                        self.connection_state = ConnectionState::Connected;
                        self.connection_retry_count = 0;
                        self.connection_error = None;
                        let view = Main::new(connection, req_rx, self.message_style.clone());
                        self.view = View::Main(view);
                        info!("Successfully transitioned to main view!");
                    }
                    Err(err) => {
                        // Connection failed
                        error!("Background connection failed: {}", err);
                        self.connection_state = ConnectionState::Failed;
                        self.connection_error = Some(err.clone());

                        // Reset login form loading state to allow retry
                        if let View::Login(ref mut login) = self.view {
                            login.is_loading = false;
                        }

                        // Auto-retry logic (max 3 attempts)
                        if self.connection_retry_count < 3 {
                            self.connection_retry_count += 1;
                            // Go back to idle state to show login with error message
                            // User can manually retry by clicking connect again
                            self.connection_state = ConnectionState::Idle;
                            // Reset login to allow retry
                            if let View::Main(_) = self.view {
                                self.view = View::Login(login::Login::default());
                            }
                        } else {
                            // Max retries reached, reset to idle to show login with error
                            self.connection_state = ConnectionState::Idle;
                            // Reset retry count for future attempts
                            self.connection_retry_count = 0;
                        }
                    }
                }
            }
        }

        // Show connection splash screen if connecting
        if matches!(self.connection_state, ConnectionState::Connecting) {
            self.show_connection_splash(ctx);
            return;
        }

        let credentials = match self.view {
            View::Login(ref mut login) => {
                // Show connection error if any
                if let Some(ref error) = self.connection_error {
                    login.error = Some(format!("Connection failed: {} (Attempt {}/3)", error, self.connection_retry_count));
                }

                if login.update(ctx)
                {
                    Done(Credentials {
                        server: login.server.to_string(),
                        user: login.user.to_string(),
                        password: login.password.to_string()
                    })
                } else {
                    Connected::None
                }
            }
            View::Main(ref mut view) => {
                let old_theme = self.is_dark_theme;
                let logout_requested = view.update(ctx, &mut self.is_dark_theme);
                if old_theme != self.is_dark_theme {
                    theme_changed = true;
                }

                // Handle logout request
                if logout_requested {
                    // Close the connection properly before logging out
                    if let View::Main(ref mut main_view) = self.view {
                        if let Err(e) = main_view.stream.close() {
                            error!("Failed to close connection during logout: {}", e);
                        }
                    }

                    self.view = View::Login(login::Login::default());
                    self.connection_state = ConnectionState::Idle;
                    self.connection_error = None;
                    self.connection_retry_count = 0;
                }

                Connected::None
            }
        };

        // Update theme if it was changed
        if theme_changed {
            self.theme = if self.is_dark_theme {
                AppTheme::dark()
            } else {
                AppTheme::light()
            };
            self.message_style = MessageStyle::for_theme(&self.theme);

            // Update the Main view's message_style if it exists
            if let View::Main(ref mut main_view) = self.view {
                main_view.message_style = self.message_style.clone();
            }

            self.save_theme_preference();
        }

        if let Done(cred) = credentials {
            self.connection_state = ConnectionState::Connecting;
            self.start_background_connection(cred);
        }
    }
}
