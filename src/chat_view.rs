//! The egui chat screen. All protocol and chat state lives in the `bnetchat`
//! library; this file only renders it and turns clicks into actions.

use std::sync::mpsc::Receiver;

use bnetchat::chat::{self, ChatMessage, ChatState, MessageKind, UserRole, MAX_MESSAGE_LEN};
use bnetchat::protocol::ServerEvent;
use bnetchat::session::Session;
use eframe::egui::{self, Color32, Label, RichText, Sense, TextStyle};
use tracing::warn;

use crate::theme::MessageStyle;

/// How many of the newest messages are laid out each frame.
const RENDER_LIMIT: usize = 500;

const AVATAR_COLORS: [Color32; 12] = [
    Color32::from_rgb(239, 68, 68),
    Color32::from_rgb(245, 101, 101),
    Color32::from_rgb(251, 146, 60),
    Color32::from_rgb(251, 191, 36),
    Color32::from_rgb(34, 197, 94),
    Color32::from_rgb(16, 185, 129),
    Color32::from_rgb(6, 182, 212),
    Color32::from_rgb(59, 130, 246),
    Color32::from_rgb(99, 102, 241),
    Color32::from_rgb(139, 92, 246),
    Color32::from_rgb(168, 85, 247),
    Color32::from_rgb(236, 72, 153),
];

#[derive(Debug, Clone)]
enum Confirm {
    Ignore(String),
    ClearMessages,
}

/// Something the user asked for from a menu, applied after rendering.
enum Action {
    Whisper(String),
    SendCommand(String),
    ToggleFriend(String),
    Unignore(String),
    Confirm(Confirm),
    Quote(ChatMessage),
    Reply(ChatMessage),
}

#[derive(Debug)]
pub struct ChatView {
    session: Session,
    events: Receiver<ServerEvent>,
    pub chat: ChatState,
    pub message_style: MessageStyle,
    input: String,
    sidebar_visible: bool,
    pending_confirmation: Option<Confirm>,
    quoted: Option<ChatMessage>,
    logout_requested: bool,
}

impl ChatView {
    pub fn new(session: Session, events: Receiver<ServerEvent>, message_style: MessageStyle) -> Self {
        let chat = ChatState::new(session.username());
        Self {
            session,
            events,
            chat,
            message_style,
            input: String::new(),
            sidebar_visible: true,
            pending_confirmation: None,
            quoted: None,
            logout_requested: false,
        }
    }

    pub fn close(&mut self) {
        self.session.close();
    }

    /// Render one frame. Returns `true` when the user asked to log out.
    pub fn update(&mut self, ctx: &egui::Context, dark_theme: &mut bool) -> bool {
        // Drain everything that arrived since the last frame.
        while let Ok(event) = self.events.try_recv() {
            self.chat.apply(event);
        }

        self.handle_shortcuts(ctx, dark_theme);
        self.apply_responsive_layout(ctx);
        self.confirmation_dialog(ctx);

        let input_id = egui::Id::new("chat_input");
        let mut action = None;

        self.header(ctx, dark_theme);
        self.composer(ctx, input_id);
        if self.sidebar_visible {
            self.user_list(ctx, &mut action);
        }
        self.message_list(ctx, &mut action);

        if let Some(action) = action {
            self.apply_action(action, ctx, input_id);
        }

        std::mem::take(&mut self.logout_requested)
    }

    fn send_line(&mut self, line: &str) {
        match self.session.send(line) {
            Ok(()) => self.chat.record_sent(line),
            Err(e) => self.chat.push(MessageKind::Error, None, e),
        }
    }

    fn send_input(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        let line = chat::compose_outgoing(&text, self.quoted.as_ref());
        self.send_line(&line);
        self.input.clear();
        self.quoted = None;
    }

    fn apply_action(&mut self, action: Action, ctx: &egui::Context, input_id: egui::Id) {
        match action {
            Action::Whisper(user) => {
                self.input = format!("/w {user} ");
                ctx.memory_mut(|m| m.request_focus(input_id));
            }
            Action::SendCommand(cmd) => self.send_line(&cmd),
            Action::ToggleFriend(user) => self.chat.toggle_friend(&user),
            Action::Unignore(user) => self.chat.set_ignored(&user, false),
            Action::Confirm(c) => self.pending_confirmation = Some(c),
            Action::Quote(msg) => self.quoted = Some(msg),
            Action::Reply(msg) => {
                if let Some(author) = &msg.author {
                    self.input = format!("@{author} ");
                }
                self.quoted = Some(msg);
                ctx.memory_mut(|m| m.request_focus(input_id));
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context, dark_theme: &mut bool) {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::U) && i.modifiers.ctrl {
                self.sidebar_visible = !self.sidebar_visible;
            }
            if i.key_pressed(egui::Key::T) && i.modifiers.ctrl {
                *dark_theme = !*dark_theme;
            }
            if i.key_pressed(egui::Key::F5) {
                self.pending_confirmation = Some(Confirm::ClearMessages);
            }
            if i.key_pressed(egui::Key::Escape) {
                self.pending_confirmation = None;
                self.quoted = None;
            }
        });
    }

    fn apply_responsive_layout(&mut self, ctx: &egui::Context) {
        let width = ctx.screen_rect().width();
        if width < 700.0 && self.sidebar_visible {
            self.sidebar_visible = false;
        } else if width >= 800.0 && !self.sidebar_visible {
            self.sidebar_visible = true;
        }

        let scale = if width < 600.0 {
            0.85
        } else if width < 800.0 {
            0.95
        } else {
            1.0
        };
        ctx.style_mut(|style| {
            for font in style.text_styles.values_mut() {
                font.size = (font.size * scale).max(8.0);
            }
        });
    }

    fn confirmation_dialog(&mut self, ctx: &egui::Context) {
        let Some(confirm) = self.pending_confirmation.clone() else { return };
        let style = self.message_style.clone();

        egui::Window::new("Confirm Action")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    match &confirm {
                        Confirm::Ignore(user) => {
                            ui.label(RichText::new("⚠️ Ignore User").color(style.error_color).size(16.0).strong());
                            ui.add_space(8.0);
                            ui.label(format!("Are you sure you want to ignore {user}?"));
                            ui.small("You will no longer see messages from this user.");
                        }
                        Confirm::ClearMessages => {
                            ui.label(RichText::new("🗑️ Clear Messages").color(style.error_color).size(16.0).strong());
                            ui.add_space(8.0);
                            ui.label("Are you sure you want to clear all chat messages?");
                            ui.small("This action cannot be undone.");
                        }
                    }
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        if ui.add(tinted_button("✅ Confirm", style.error_color, 1.0)).clicked() {
                            match &confirm {
                                Confirm::Ignore(user) => self.chat.set_ignored(user, true),
                                Confirm::ClearMessages => self.chat.clear_messages(),
                            }
                            self.pending_confirmation = None;
                        }
                        ui.add_space(8.0);
                        if ui.add(tinted_button("❌ Cancel", style.system_color, 1.0)).clicked() {
                            self.pending_confirmation = None;
                        }
                    });
                    ui.add_space(8.0);
                });
            });
    }

    fn connection_indicator(&self, ui: &mut egui::Ui) {
        if self.chat.connected {
            ui.label("🟢").on_hover_text("Connected to server");
        } else {
            ui.label("🔴").on_hover_text("Disconnected");
        }
    }

    fn header(&mut self, ctx: &egui::Context, dark_theme: &mut bool) {
        let style = self.message_style.clone();
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("💬 Bnet Chat").color(style.user_color).size(16.0).strong());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    let logout = tinted_button("🚪", style.error_color, 0.3).min_size([32.0, 32.0].into());
                    if ui.add(logout).on_hover_text("Logout").clicked() {
                        self.logout_requested = true;
                    }

                    let icon = if *dark_theme { "🌙" } else { "☀" };
                    let theme = tinted_button(icon, style.system_color, 0.3).min_size([32.0, 32.0].into());
                    if ui.add(theme).on_hover_text("Toggle theme (Ctrl+T)").clicked() {
                        *dark_theme = !*dark_theme;
                    }

                    self.connection_indicator(ui);
                });
            });
            ui.add_space(4.0);
        });
    }

    fn composer(&mut self, ctx: &egui::Context, input_id: egui::Id) {
        let style = self.message_style.clone();
        egui::TopBottomPanel::bottom("composer").show(ctx, |ui| {
            ui.add_space(8.0);

            if let Some(quoted) = self.quoted.clone() {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("📝").color(style.system_color).size(16.0));
                    egui::Frame::new()
                        .fill(style.system_color.gamma_multiply(0.1))
                        .corner_radius(4.0)
                        .inner_margin(6.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Quoting:").color(style.system_color).size(10.0));
                                ui.label(
                                    RichText::new(format!("[{}]", chat::format_time(quoted.timestamp)))
                                        .color(style.system_color.gamma_multiply(0.7))
                                        .size(10.0),
                                );
                            });
                            ui.label(
                                RichText::new(truncate(&quoted.display_text(), 50))
                                    .color(style.user_color)
                                    .size(11.0)
                                    .italics(),
                            );
                        });
                    let clear = tinted_button("❌", style.error_color, 0.3).min_size([20.0, 20.0].into());
                    if ui.add(clear).on_hover_text("Clear quote").clicked() {
                        self.quoted = None;
                    }
                });
                ui.add_space(4.0);
            }

            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let input = ui.add_sized(
                    [ui.available_width() - 90.0, 60.0],
                    egui::TextEdit::multiline(&mut self.input)
                        .id(input_id)
                        .hint_text("Type your message... (max 500 chars)\nPress Ctrl+Enter to send")
                        .char_limit(MAX_MESSAGE_LEN)
                        .desired_rows(2),
                );

                let count = self.input.chars().count();
                let count_color = if count > MAX_MESSAGE_LEN * 3 / 4 {
                    style.whisper_color
                } else {
                    style.system_color.gamma_multiply(0.7)
                };
                ui.small(RichText::new(format!("{count}/{MAX_MESSAGE_LEN}")).color(count_color));

                if input.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl) {
                    self.send_input();
                }
                // Keep the caret in the input unless the user clicked elsewhere.
                if input.lost_focus() && !ui.input(|i| i.pointer.any_click()) {
                    input.request_focus();
                }

                ui.add_space(4.0);
                let send = tinted_button("📤 Send", style.user_color, 0.5).min_size([80.0, 28.0].into());
                if ui.add(send).clicked() {
                    self.send_input();
                }
                ui.add_space(8.0);
            });
            ui.add_space(8.0);
        });
    }

    fn user_list(&self, ctx: &egui::Context, action: &mut Option<Action>) {
        let style = &self.message_style;
        egui::SidePanel::right("sidebar_users")
            .min_width(180.0)
            .max_width(300.0)
            .default_width(220.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.heading(format!("👥 Users ({})", self.chat.users.len()));
                    self.connection_indicator(ui);
                });
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (name, role) in self.chat.sorted_users() {
                        egui::Frame::new()
                            .fill(ui.visuals().widgets.noninteractive.weak_bg_fill)
                            .corner_radius(4.0)
                            .inner_margin(6.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    avatar(ui, name, 24.0);
                                    ui.add_space(4.0);
                                    match role {
                                        UserRole::Admin => drop(ui.label("👑")),
                                        UserRole::Moderator => drop(ui.label("🛡️")),
                                        UserRole::User => {}
                                    }
                                    let label = ui.add(
                                        Label::new(RichText::new(name).color(style.user_color)).sense(Sense::click()),
                                    );
                                    label
                                        .on_hover_ui(|ui| {
                                            ui.label(format!("👤 {name}"));
                                            ui.separator();
                                            ui.label(format!("Role: {}", role.label()));
                                        })
                                        .context_menu(|ui| self.user_menu(ui, name, action));
                                });
                            });
                        ui.add_space(2.0);
                    }
                });
            });
    }

    fn user_menu(&self, ui: &mut egui::Ui, name: &str, action: &mut Option<Action>) {
        let style = &self.message_style;
        ui.style_mut().spacing.button_padding = [8.0, 4.0].into();
        ui.style_mut().spacing.item_spacing = [4.0, 2.0].into();

        ui.label(RichText::new(format!("👤 {name}")).color(style.user_color).strong());
        ui.separator();

        let mut item = |ui: &mut egui::Ui, text: &str, color: Color32, a: Action| {
            if ui.add(tinted_button(text, color, 0.3)).clicked() {
                *action = Some(a);
                ui.close_menu();
            }
        };

        ui.label(RichText::new("💬 Communication").color(style.system_color).size(11.0));
        item(ui, "💬 Send Whisper", style.whisper_color, Action::Whisper(name.into()));
        item(ui, "📍 Ping User", style.system_color, Action::SendCommand(format!("/ping {name}")));
        ui.separator();

        ui.label(RichText::new("👁️ Monitoring").color(style.system_color).size(11.0));
        item(ui, "👁️ Watch User", style.join_leave_color, Action::SendCommand(format!("/watch {name}")));
        item(ui, "👁️‍🗨️ Unwatch User", style.system_color, Action::SendCommand(format!("/unwatch {name}")));
        ui.separator();

        ui.label(RichText::new("👥 Social").color(style.system_color).size(11.0));
        if self.chat.friends.contains(name) {
            item(ui, "💔 Remove Friend", style.error_color, Action::ToggleFriend(name.into()));
        } else {
            item(ui, "❤️ Add Friend", style.join_leave_color, Action::ToggleFriend(name.into()));
        }
        if self.chat.ignored.contains(name) {
            item(ui, "🔊 Unignore User", style.join_leave_color, Action::Unignore(name.into()));
        } else {
            item(ui, "🔇 Ignore User", style.error_color, Action::Confirm(Confirm::Ignore(name.into())));
        }
    }

    fn message_list(&self, ctx: &egui::Context, action: &mut Option<Action>) {
        let style = &self.message_style;
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(channel) = &self.chat.channel {
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill.gamma_multiply(0.8))
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .corner_radius(4.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("📺 {channel}")).strong().color(style.user_color));
                            if let Some(topic) = &self.chat.topic {
                                ui.separator();
                                ui.label(RichText::new(topic).italics().color(style.system_color));
                            }
                        });
                    });
                ui.add_space(4.0);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.small(format!("Messages: {}", self.chat.messages.len()));
            });

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let started = std::time::Instant::now();
                    let skip = self.chat.messages.len().saturating_sub(RENDER_LIMIT);
                    for (index, msg) in self.chat.messages.iter().skip(skip).enumerate() {
                        self.message_row(ui, index, msg, action);
                        ui.add_space(2.0);
                    }
                    let took = started.elapsed();
                    if took.as_millis() > 16 {
                        warn!("Chat render took {}ms", took.as_millis());
                    }
                });
        });
    }

    fn message_row(&self, ui: &mut egui::Ui, index: usize, msg: &ChatMessage, action: &mut Option<Action>) {
        let style = &self.message_style;
        let highlight = self.chat.is_highlight(msg);
        let (icon, color, text_style) = match msg.kind {
            MessageKind::Talk | MessageKind::Own => ("", style.user_color, "chat_message"),
            MessageKind::Emote => ("✨ ", style.user_color, "chat_message"),
            MessageKind::Whisper => ("🗣️ ", style.whisper_color, "chat_whisper"),
            MessageKind::JoinLeave => ("👋 ", style.join_leave_color, "chat_system"),
            MessageKind::Info | MessageKind::System => ("ℹ️ ", style.system_color, "chat_system"),
            MessageKind::Error => ("❌ ", style.error_color, "chat_system"),
        };
        let color = if highlight { style.whisper_color } else { color };
        let text_style = TextStyle::Name(text_style.into());

        let response = ui
            .horizontal_wrapped(|ui| {
                let bg = if highlight {
                    style.whisper_color.gamma_multiply(0.1)
                } else if index.is_multiple_of(2) {
                    style.system_color.gamma_multiply(0.05)
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(ui.available_rect_before_wrap(), 2.0, bg);
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("[{}]", chat::format_time(msg.timestamp)))
                        .text_style(TextStyle::Name("chat_system".into()))
                        .color(style.system_color.gamma_multiply(0.7)),
                );
                if !icon.is_empty() {
                    ui.label(RichText::new(icon).text_style(text_style.clone()).color(color));
                }
                render_rich_text(ui, &msg.display_text(), color, text_style);
            })
            .response;

        response.context_menu(|ui| {
            ui.style_mut().spacing.button_padding = [8.0, 4.0].into();
            ui.label(RichText::new("💬 Message Actions").color(style.user_color).strong());
            ui.separator();
            if ui.add(tinted_button("📝 Quote Message", style.system_color, 0.3)).clicked() {
                *action = Some(Action::Quote(msg.clone()));
                ui.close_menu();
            }
            if ui.add(tinted_button("↩️ Reply to Message", style.whisper_color, 0.3)).clicked() {
                *action = Some(Action::Reply(msg.clone()));
                ui.close_menu();
            }
            ui.separator();
            if ui.add(tinted_button("📋 Copy Message", style.join_leave_color, 0.3)).clicked() {
                ui.ctx().copy_text(msg.display_text());
                ui.close_menu();
            }
        });
    }
}

/// A button with a light fill and outline in `color`.
fn tinted_button(text: &str, color: Color32, stroke_alpha: f32) -> egui::Button<'static> {
    egui::Button::new(text.to_string())
        .fill(color.gamma_multiply(0.1))
        .stroke(egui::Stroke::new(1.0_f32, color.gamma_multiply(stroke_alpha)))
}

fn avatar(ui: &mut egui::Ui, name: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(size), Sense::hover());
    let color = AVATAR_COLORS[chat::avatar_index(name, AVATAR_COLORS.len())];
    ui.painter().circle_filled(rect.center(), size / 2.0, color);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        chat::initials(name),
        egui::FontId::proportional(10.0),
        Color32::WHITE,
    );
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() > max_chars {
        format!("{}...", s.chars().take(max_chars).collect::<String>())
    } else {
        s.to_string()
    }
}

/// Light formatting: emoticons, a leading URL, and one `**bold**` span.
fn render_rich_text(ui: &mut egui::Ui, text: &str, color: Color32, text_style: TextStyle) {
    let plain = |ui: &mut egui::Ui, t: &str| {
        ui.label(RichText::new(t).text_style(text_style.clone()).color(color));
    };
    if text.len() > 200 {
        plain(ui, text);
        return;
    }

    let mut text = text.to_string();
    for (emoticon, emoji) in [
        (":)", "😊"), (":(", "😢"), (":D", "😃"), (";)", "😉"),
        ("</3", "💔"), ("<3", "❤️"), (":P", "😛"), (":p", "😛"),
    ] {
        text = text.replace(emoticon, emoji);
    }

    if text.starts_with("http://") || text.starts_with("https://") {
        let (url, rest) = text.split_at(text.find(' ').unwrap_or(text.len()));
        ui.hyperlink_to(url, url);
        if !rest.trim().is_empty() {
            plain(ui, rest);
        }
        return;
    }

    if let Some(start) = text.find("**") {
        if let Some(len) = text[start + 2..].find("**") {
            let (before, bold, after) = (&text[..start], &text[start + 2..start + 2 + len], &text[start + 4 + len..]);
            if !before.is_empty() {
                plain(ui, before);
            }
            ui.label(RichText::new(bold).text_style(text_style.clone()).color(color).strong());
            if !after.is_empty() {
                plain(ui, after);
            }
            return;
        }
    }

    plain(ui, &text);
}
