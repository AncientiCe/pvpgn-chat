//! Chat screen: channel sidebar, message transcript, composer and member list.

use bnetchat::chat::{self, ChatMessage, ChatState, MessageKind, UserRole};
use bnetchat::protocol::ServerEvent;
use bnetchat::session::Session;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Consecutive lines from one author within this many seconds are grouped.
const GROUP_WINDOW_SECS: u64 = 5 * 60;

pub enum ChatViewEvent {
    LoggedOut,
}

pub struct ChatView {
    session: Session,
    chat: ChatState,
    input: Entity<InputState>,
    scroller: Entity<MessageScrollerState>,
    /// Rows the scroller knows about, and how many drops it has seen.
    list_len: usize,
    seen_dropped: u64,
    quoted: Option<ChatMessage>,
    show_members: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ChatViewEvent> for ChatView {}

impl ChatView {
    pub fn new(session: Session, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let chat = ChatState::new(session.username());
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Message the channel"));
        let scroller = cx.new(|cx| MessageScrollerState::new(0, cx));
        let sub = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.send_input(window, cx);
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        // Log out cleanly when the app quits, so the server frees the account.
        let quit = cx.on_app_quit(|this, _cx| {
            this.session.close();
            async {}
        });

        Self {
            session,
            chat,
            input,
            scroller,
            list_len: 0,
            seen_dropped: 0,
            quoted: None,
            show_members: true,
            _subscriptions: vec![sub, quit],
        }
    }

    pub fn apply_events(&mut self, events: Vec<ServerEvent>, cx: &mut Context<Self>) {
        for event in events {
            self.chat.apply(event);
        }
        self.sync_scroller(cx);
        cx.notify();
    }

    /// Keep the virtual list's row count in step with the message buffer,
    /// which drops old messages from the front once it is full.
    fn sync_scroller(&mut self, cx: &mut Context<Self>) {
        let removed = ((self.chat.dropped - self.seen_dropped) as usize).min(self.list_len);
        self.seen_dropped = self.chat.dropped;
        let len = self.chat.messages.len();
        let kept = self.list_len - removed;
        self.scroller.update(cx, |s, cx| {
            if removed > 0 {
                s.splice(0..removed, 0, cx);
            }
            if len > kept {
                s.append(len - kept, cx);
            } else if len < kept {
                s.reset(len, cx);
            }
        });
        self.list_len = len;
    }

    fn send_line(&mut self, line: &str, cx: &mut Context<Self>) {
        match self.session.send(line) {
            Ok(()) => self.chat.record_sent(line),
            Err(e) => self.chat.push(MessageKind::Error, None, e),
        }
        self.sync_scroller(cx);
        cx.notify();
    }

    fn send_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        let line = chat::compose_outgoing(&text, self.quoted.as_ref());
        self.send_line(&line, cx);
        self.quoted = None;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn set_input(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.focus(window, cx);
        });
    }

    fn logout(&mut self, cx: &mut Context<Self>) {
        self.session.close();
        cx.emit(ChatViewEvent::LoggedOut);
    }

    fn render_sidebar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let dark = theme.mode.is_dark();
        let channel = self
            .chat
            .channel
            .clone()
            .unwrap_or_else(|| "Not in a channel".into());

        div()
            .w(px(240.))
            .h_full()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.sidebar_border)
            .child(
                div()
                    .h(px(52.))
                    .px_4()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(theme.sidebar_border)
                    .font_bold()
                    .child("Bnet Chat"),
            )
            .child(
                div()
                    .flex_1()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .px_2()
                            .pt_2()
                            .pb_1()
                            .text_xs()
                            .font_semibold()
                            .text_color(theme.muted_foreground)
                            .child("CHANNELS"),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1p5()
                            .rounded(theme.radius)
                            .bg(theme.sidebar_accent)
                            .text_color(theme.sidebar_accent_foreground)
                            .flex()
                            .gap_2()
                            .child(div().text_color(theme.muted_foreground).child("#"))
                            .child(channel),
                    ),
            )
            .child(
                // Account footer.
                div()
                    .p_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_t_1()
                    .border_color(theme.sidebar_border)
                    .child(Avatar::new().name(self.chat.me.clone()).small())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .truncate()
                                    .child(self.chat.me.clone()),
                            )
                            .child(status_line(self.chat.connected, cx)),
                    )
                    .child(
                        Button::new("theme")
                            .ghost()
                            .small()
                            .icon(if dark { IconName::Sun } else { IconName::Moon })
                            .tooltip("Toggle theme")
                            .on_click(move |_, window, cx| crate::set_dark(!dark, window, cx)),
                    )
                    .child(
                        Button::new("logout")
                            .ghost()
                            .small()
                            .icon(IconName::CircleX)
                            .tooltip("Log out")
                            .on_click(cx.listener(|this, _, _, cx| this.logout(cx))),
                    ),
            )
            .when(window.bounds().size.width < px(700.), |el| el.hidden())
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .h(px(52.))
            .px_4()
            .flex()
            .items_center()
            .gap_3()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_xl()
                    .text_color(theme.muted_foreground)
                    .child("#"),
            )
            .child(
                div()
                    .font_semibold()
                    .child(self.chat.channel.clone().unwrap_or_default()),
            )
            .children(self.chat.topic.clone().map(|topic| {
                div()
                    .flex_1()
                    .min_w_0()
                    .pl_3()
                    .border_l_1()
                    .border_color(theme.border)
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .truncate()
                    .child(topic)
            }))
            .when(self.chat.topic.is_none(), |el| el.child(div().flex_1()))
            .child(
                Button::new("members")
                    .ghost()
                    .small()
                    .icon(if self.show_members {
                        IconName::PanelRightClose
                    } else {
                        IconName::PanelRightOpen
                    })
                    .tooltip("Toggle member list")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_members = !this.show_members;
                        cx.notify();
                    })),
            )
    }

    fn render_transcript(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let surface = cx.theme().background;
        MessageScroller::new(
            "transcript",
            self.scroller.clone(),
            move |index, _window, cx| {
                let Some(entity) = weak.upgrade() else {
                    return div().into_any_element();
                };
                let this = entity.read(cx);
                let Some(msg) = this.chat.messages.get(index) else {
                    return div().into_any_element();
                };
                let prev = index.checked_sub(1).and_then(|i| this.chat.messages.get(i));
                render_message(
                    msg,
                    prev,
                    this.chat.is_highlight(msg),
                    index,
                    weak.clone(),
                    cx,
                )
            },
        )
        .with_bottom_fade(surface)
        // Rows space themselves (grouped lines sit tight, new groups get top padding).
        .with_row_style(StyleRefinement::default().pb_0())
        .flex_1()
    }

    fn render_composer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .px_4()
            .pb_4()
            .flex()
            .flex_col()
            .gap_2()
            .children(self.quoted.as_ref().map(|q| {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1p5()
                    .rounded(theme.radius)
                    .bg(theme.muted)
                    .text_sm()
                    .child(div().text_color(theme.muted_foreground).child("Quoting"))
                    .child(div().flex_1().min_w_0().truncate().child(q.display_text()))
                    .child(
                        Button::new("clear-quote")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.quoted = None;
                                cx.notify();
                            })),
                    )
            }))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().child(Input::new(&self.input)))
                    .child(
                        Button::new("send").primary().label("Send").on_click(
                            cx.listener(|this, _, window, cx| this.send_input(window, cx)),
                        ),
                    ),
            )
    }

    fn render_members(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let users = self.chat.sorted_users();
        let mut list = div().flex().flex_col().gap_0p5().p_2();

        let mut last_role = None;
        for (name, role) in users {
            if last_role != Some(role) {
                last_role = Some(role);
                let count = self.chat.users.values().filter(|r| **r == role).count();
                let title = match role {
                    UserRole::Admin => "ADMINS",
                    UserRole::Moderator => "MODERATORS",
                    UserRole::User => "MEMBERS",
                };
                list = list.child(
                    div()
                        .px_2()
                        .pt_3()
                        .pb_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(format!("{title} — {count}")),
                );
            }
            list = list.child(self.render_member(name, role, cx));
        }

        let theme = cx.theme();
        div()
            .id("members")
            .w(px(220.))
            .h_full()
            .flex_shrink_0()
            .overflow_y_scroll()
            .bg(theme.sidebar)
            .border_l_1()
            .border_color(theme.sidebar_border)
            .child(list)
    }

    fn render_member(
        &self,
        name: &str,
        role: UserRole,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let is_friend = self.chat.friends.contains(name);
        let is_ignored = self.chat.ignored.contains(name);
        let name_color = match role {
            UserRole::Admin => theme.warning,
            UserRole::Moderator => theme.info,
            UserRole::User => theme.sidebar_foreground,
        };
        let view = cx.entity().downgrade();
        let user = name.to_string();

        div()
            .id(SharedString::from(format!("member-{name}")))
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .gap_2()
            .rounded(theme.radius)
            .hover(|s| s.bg(theme.sidebar_accent))
            .when(is_ignored, |el| el.opacity(0.5))
            .child(Avatar::new().name(name.to_string()).small())
            .child(div().flex_1().min_w_0().truncate().text_sm().text_color(name_color).child(name.to_string()))
            .when(is_friend, |el| el.child(div().text_xs().text_color(theme.danger).child("♥")))
            .context_menu(move |menu, _window, _cx| {
                let item = |label: &str, f: fn(&mut ChatView, &str, &mut Window, &mut Context<ChatView>)| {
                    let (view, user) = (view.clone(), user.clone());
                    PopupMenuItem::new(label.to_string()).on_click(move |_, window, cx| {
                        let _ = view.update(cx, |this, cx| f(this, &user, window, cx));
                    })
                };
                menu.item(item("Whisper", |this, u, w, cx| this.set_input(format!("/w {u} "), w, cx)))
                    .item(item("Ping", |this, u, _, cx| this.send_line(&format!("/ping {u}"), cx)))
                    .separator()
                    .item(item("Watch", |this, u, _, cx| this.send_line(&format!("/watch {u}"), cx)))
                    .item(item("Unwatch", |this, u, _, cx| this.send_line(&format!("/unwatch {u}"), cx)))
                    .separator()
                    .item(item(if is_friend { "Remove friend" } else { "Add friend" }, |this, u, _, cx| {
                        this.chat.toggle_friend(u);
                        cx.notify();
                    }))
                    .item(item(if is_ignored { "Unignore" } else { "Ignore" }, |this, u, _, cx| {
                        let ignored = this.chat.ignored.contains(u);
                        this.chat.set_ignored(u, !ignored);
                        cx.notify();
                    }))
            })
    }

    /// Reply or quote. Whispers are answered with a plain `/w name ` (WC3 has
    /// no @-mentions); channel lines get a quote above the composer.
    fn quote(&mut self, msg: ChatMessage, window: &mut Window, cx: &mut Context<Self>) {
        match chat::reply_prefix(&msg) {
            Some(prefix) => self.set_input(prefix, window, cx),
            None => self.quoted = Some(msg),
        }
        self.input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let show_members = self.show_members && window.bounds().size.width >= px(900.);
        div()
            .size_full()
            .flex()
            .child(self.render_sidebar(window, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(self.render_header(cx))
                    .child(self.render_transcript(cx))
                    .child(self.render_composer(cx)),
            )
            .when(show_members, |el| el.child(self.render_members(cx)))
    }
}

fn status_line(connected: bool, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let (color, text) = if connected {
        (theme.success, "Online")
    } else {
        (theme.danger, "Disconnected")
    };
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(div().size(px(8.)).rounded_full().bg(color))
        .child(text)
}

/// Chat text the user can select with the mouse and copy (Ctrl+C).
///
/// TextView renders Markdown, so escape it: chat lines are shown literally.
fn selectable_text(index: usize, text: &str) -> TextView {
    TextView::markdown(("msg-text", index), escape_markdown(text)).selectable(true)
}

fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if matches!(
            c,
            '\\' | '`'
                | '*'
                | '_'
                | '['
                | ']'
                | '<'
                | '>'
                | '#'
                | '|'
                | '~'
                | '-'
                | '+'
                | '.'
                | '!'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Whether `msg` continues a group started by `prev` (same author, close in time).
fn continues_group(msg: &ChatMessage, prev: Option<&ChatMessage>) -> bool {
    let talkish = |m: &ChatMessage| matches!(m.kind, MessageKind::Talk | MessageKind::Own);
    match prev {
        Some(p) => {
            talkish(msg)
                && talkish(p)
                && p.author == msg.author
                && msg.timestamp.saturating_sub(p.timestamp) < GROUP_WINDOW_SECS
        }
        None => false,
    }
}

fn render_message(
    msg: &ChatMessage,
    prev: Option<&ChatMessage>,
    highlight: bool,
    index: usize,
    view: WeakEntity<ChatView>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let time = chat::format_time(msg.timestamp);

    let row = div()
        .id(("msg", index))
        .w_full()
        .px_4()
        .flex()
        .gap_3()
        .hover(|s| s.bg(theme.list_hover))
        .when(highlight, |el| {
            el.bg(theme.warning.opacity(0.08))
                .border_l_2()
                .border_color(theme.warning)
        });

    let row = match msg.kind {
        MessageKind::Talk | MessageKind::Own => {
            let author = msg.author.clone().unwrap_or_default();
            if continues_group(msg, prev) {
                row.py_0p5()
                    .child(
                        div()
                            .w(px(48.))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(theme.transparent)
                            .child(time),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(selectable_text(index, &msg.text)),
                    )
            } else {
                let name_color = if msg.kind == MessageKind::Own {
                    theme.primary
                } else {
                    theme.foreground
                };
                row.pt_3()
                    .pb_0p5()
                    .child(Avatar::new().name(author.clone()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap_2()
                                    .child(
                                        div().font_semibold().text_color(name_color).child(author),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(time),
                                    ),
                            )
                            .child(selectable_text(index, &msg.text)),
                    )
            }
        }
        kind => {
            let (icon, color) = match kind {
                MessageKind::Whisper => ("✉", theme.magenta),
                MessageKind::Emote => ("✦", theme.foreground),
                MessageKind::JoinLeave => ("→", theme.success),
                MessageKind::Error => ("!", theme.danger),
                _ => ("i", theme.muted_foreground),
            };
            row.py_1()
                .text_sm()
                .child(
                    div()
                        .w(px(48.))
                        .flex_shrink_0()
                        .flex()
                        .justify_center()
                        .text_color(color)
                        .child(icon),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(if kind == MessageKind::Emote {
                            theme.foreground
                        } else {
                            color
                        })
                        .when(kind == MessageKind::Emote, |el| el.italic())
                        .child(selectable_text(index, &msg.display_text())),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(time),
                )
        }
    };

    let msg = msg.clone();
    row.context_menu(move |menu, _window, _cx| {
        let (v1, v2, m1, m2) = (view.clone(), view.clone(), msg.clone(), msg.clone());
        let text = msg.display_text();
        menu.item(PopupMenuItem::new("Reply").on_click(move |_, window, cx| {
            let _ = v1.update(cx, |this, cx| this.quote(m1.clone(), window, cx));
        }))
        .item(PopupMenuItem::new("Quote").on_click(move |_, window, cx| {
            let _ = v2.update(cx, |this, cx| this.quote(m2.clone(), window, cx));
        }))
        .separator()
        .item(PopupMenuItem::new("Copy text").on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        }))
    })
    .into_any_element()
}
