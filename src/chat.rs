//! Chat state: messages, users and channel info, updated from [`ServerEvent`]s.
//!
//! No UI code lives here; front ends read this state and render it.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::protocol::ServerEvent;

/// Longest message the input box accepts.
pub const MAX_MESSAGE_LEN: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    /// Someone talking in the channel.
    Talk,
    /// A line we sent.
    Own,
    Whisper,
    Emote,
    JoinLeave,
    Info,
    System,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub kind: MessageKind,
    /// Who said it, for talk/whisper/emote/own lines.
    pub author: Option<String>,
    pub text: String,
    /// Unix seconds.
    pub timestamp: u64,
    /// For whispers: the other person (sender of an incoming whisper,
    /// recipient of one we sent). Replies go back to them with `/w`.
    pub peer: Option<String>,
}

impl ChatMessage {
    /// The line as one string, as shown in plain-text views and copied to the clipboard.
    pub fn display_text(&self) -> String {
        match (&self.kind, &self.author) {
            (MessageKind::Talk | MessageKind::Own, Some(a)) => format!("{a}: {}", self.text),
            (MessageKind::Emote, Some(a)) => format!("{a} {}", self.text),
            (MessageKind::Whisper, _) => match (&self.peer, &self.author) {
                (Some(peer), Some(a)) if a == peer => format!("{peer} whispers: {}", self.text),
                (Some(peer), _) => format!("You whisper to {peer}: {}", self.text),
                _ => self.text.clone(),
            },
            _ => self.text.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UserRole {
    User,
    Moderator,
    Admin,
}

impl UserRole {
    /// Role from the 4-digit hex flags field of USER/JOIN lines.
    /// `0003` and `0010` are admins on the servers this client targets, `0001`/`0002` moderators.
    pub fn from_flags(flags: &str) -> Self {
        match flags {
            "0003" | "0010" => UserRole::Admin,
            "0001" | "0002" => UserRole::Moderator,
            _ => UserRole::User,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            UserRole::Admin => "Administrator",
            UserRole::Moderator => "Moderator",
            UserRole::User => "User",
        }
    }
}

#[derive(Debug)]
pub struct ChatState {
    /// Our own account name, used to highlight mentions.
    pub me: String,
    pub messages: VecDeque<ChatMessage>,
    pub users: HashMap<String, UserRole>,
    pub channel: Option<String>,
    pub topic: Option<String>,
    pub friends: HashSet<String>,
    pub ignored: HashSet<String>,
    pub connected: bool,
    /// How many old messages have been dropped from the front so far, so
    /// virtual lists can keep their row count in sync.
    pub dropped: u64,
    max_messages: usize,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl ChatState {
    pub fn new(me: impl Into<String>) -> Self {
        Self {
            me: me.into(),
            messages: VecDeque::new(),
            users: HashMap::new(),
            channel: None,
            topic: None,
            friends: HashSet::new(),
            ignored: HashSet::new(),
            connected: true,
            dropped: 0,
            max_messages: 1000,
        }
    }

    pub fn push(&mut self, kind: MessageKind, author: Option<&str>, text: impl Into<String>) {
        self.messages.push_back(ChatMessage {
            kind,
            author: author.map(str::to_string),
            text: text.into(),
            timestamp: now_secs(),
            peer: None,
        });
        while self.messages.len() > self.max_messages {
            self.messages.pop_front();
            self.dropped += 1;
        }
    }

    fn push_whisper(&mut self, author: &str, peer: &str, text: String) {
        self.push(MessageKind::Whisper, Some(author), text);
        if let Some(last) = self.messages.back_mut() {
            last.peer = Some(peer.to_string());
        }
    }

    /// Record a line we just sent. Slash commands are not echoed: the server
    /// answers them itself (e.g. a whisper comes back as `WhisperSent`).
    pub fn record_sent(&mut self, line: &str) {
        if line.starts_with('/') {
            return;
        }
        let me = self.me.clone();
        self.push(MessageKind::Own, Some(&me), line);
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }

    /// Update state from one server event.
    pub fn apply(&mut self, event: ServerEvent) {
        match event {
            ServerEvent::User { name, flags } => {
                self.users.insert(name, UserRole::from_flags(&flags));
            }
            ServerEvent::Join { name, flags } => {
                self.users
                    .insert(name.clone(), UserRole::from_flags(&flags));
                self.push(
                    MessageKind::JoinLeave,
                    None,
                    format!("{name} joined the channel"),
                );
            }
            ServerEvent::Leave { name } => {
                self.users.remove(&name);
                self.push(
                    MessageKind::JoinLeave,
                    None,
                    format!("{name} left the channel"),
                );
            }
            ServerEvent::Talk { from, text } => {
                if !self.ignored.contains(&from) {
                    self.push(MessageKind::Talk, Some(&from), text);
                }
            }
            ServerEvent::Emote { from, text } => {
                if !self.ignored.contains(&from) {
                    self.push(MessageKind::Emote, Some(&from), text);
                }
            }
            ServerEvent::Whisper { from, text } => {
                if !self.ignored.contains(&from) {
                    self.push_whisper(&from.clone(), &from, text);
                }
            }
            ServerEvent::WhisperSent { to, text } => {
                let me = self.me.clone();
                self.push_whisper(&me, &to, text);
            }
            ServerEvent::Broadcast { text } => {
                self.push(MessageKind::System, None, format!("Broadcast: {text}"))
            }
            ServerEvent::Channel { name } => {
                self.users.clear();
                self.topic = None;
                self.push(MessageKind::System, None, format!("Joined channel {name}"));
                self.channel = Some(name);
            }
            ServerEvent::ChannelTopic { channel, topic } => {
                self.channel = Some(channel);
                self.topic = Some(topic);
            }
            // Servers send blank INFO lines as spacers in their welcome text.
            ServerEvent::Info { text } if text.trim().is_empty() => {}
            ServerEvent::Info { text } => self.push(MessageKind::Info, None, text),
            ServerEvent::Error { text } => self.push(MessageKind::Error, None, text),
            ServerEvent::Null => {}
            ServerEvent::Malformed { line } => {
                self.push(MessageKind::Error, None, format!("Malformed: {line}"))
            }
            ServerEvent::Unknown { line } => self.push(MessageKind::System, None, line),
            ServerEvent::Disconnected { reason } => {
                self.connected = false;
                self.push(MessageKind::Error, None, format!("Disconnected: {reason}"));
            }
        }
    }

    /// Users sorted by role (admins first), then case-insensitively by name.
    pub fn sorted_users(&self) -> Vec<(&str, UserRole)> {
        let mut users: Vec<_> = self.users.iter().map(|(n, r)| (n.as_str(), *r)).collect();
        users.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        });
        users
    }

    /// Whether a message should stand out: whispers, and lines naming us.
    pub fn is_highlight(&self, msg: &ChatMessage) -> bool {
        match msg.kind {
            MessageKind::Whisper => msg.author.as_deref() != Some(self.me.as_str()),
            MessageKind::Talk | MessageKind::Emote => {
                !self.me.is_empty() && msg.text.to_lowercase().contains(&self.me.to_lowercase())
            }
            _ => false,
        }
    }

    pub fn toggle_friend(&mut self, name: &str) {
        if !self.friends.remove(name) {
            self.friends.insert(name.to_string());
        }
    }

    pub fn set_ignored(&mut self, name: &str, ignored: bool) {
        if ignored {
            self.ignored.insert(name.to_string());
        } else {
            self.ignored.remove(name);
        }
    }
}

/// What to prefill the input with to reply to `msg`: `/w name ` for a
/// whisper (WC3 has no @-mentions), nothing for channel lines.
pub fn reply_prefix(msg: &ChatMessage) -> Option<String> {
    msg.peer.as_ref().map(|peer| format!("/w {peer} "))
}

/// Text to send for `input`, with a quote of `quoted` if any.
///
/// Quoting a whisper sends exactly `/w name <input>` back to that person
/// (no quote text, so bot commands like `.map` still work). Quoting a
/// channel line prefixes a short quote and sends it to the channel.
pub fn compose_outgoing(input: &str, quoted: Option<&ChatMessage>) -> String {
    let Some(q) = quoted else {
        return input.to_string();
    };

    if let Some(peer) = &q.peer {
        // Don't double the command if the input already starts with it.
        let prefix = format!("/w {peer} ");
        let rest = if input.to_lowercase().starts_with(&prefix.to_lowercase()) {
            &input[prefix.len()..]
        } else {
            input
        };
        return format!("{prefix}{rest}");
    }

    const MAX_QUOTE: usize = 60;
    let mut excerpt: String = q.text.chars().take(MAX_QUOTE).collect();
    if q.text.chars().count() > MAX_QUOTE {
        excerpt.push_str("...");
    }
    let quote = format!(
        "[{} {}: \"{}\"]",
        q.author.as_deref().unwrap_or("?"),
        format_time(q.timestamp),
        excerpt
    );

    format!("{quote} {input}")
}

/// `HH:MM` (UTC) for a Unix timestamp.
pub fn format_time(timestamp: u64) -> String {
    let secs = timestamp % (24 * 60 * 60);
    format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60)
}

/// One or two uppercase initials for an avatar.
pub fn initials(name: &str) -> String {
    let mut words = name.split_whitespace();
    let first = words.next().and_then(|w| w.chars().next());
    let second = words.next().and_then(|w| w.chars().next());
    match (first, second) {
        (Some(a), Some(b)) => format!("{}{}", a.to_uppercase(), b.to_uppercase()),
        (Some(a), None) => a.to_uppercase().collect(),
        _ => "?".to_string(),
    }
}

/// A stable index in `0..palette_len` for coloring a user's avatar.
pub fn avatar_index(name: &str, palette_len: usize) -> usize {
    let hash = name
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    hash as usize % palette_len.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::parse_line;

    fn feed(state: &mut ChatState, lines: &[&str]) {
        for l in lines {
            state.apply(parse_line(l));
        }
    }

    #[test]
    fn channel_join_resets_users_and_tracks_roster() {
        let mut s = ChatState::new("me");
        feed(
            &mut s,
            &[
                "1007 CHANNEL \"W3\"",
                "1001 USER Alice 0010 [W3XP]",
                "1001 USER bob 0000 [W3XP]",
                "1002 JOIN Carl 0002 [W3XP]",
                "1003 LEAVE bob 0000",
            ],
        );
        assert_eq!(s.channel.as_deref(), Some("W3"));
        assert_eq!(
            s.sorted_users(),
            vec![("Alice", UserRole::Admin), ("Carl", UserRole::Moderator)]
        );
    }

    #[test]
    fn ignored_users_are_hidden() {
        let mut s = ChatState::new("me");
        s.set_ignored("troll", true);
        feed(
            &mut s,
            &[
                "1005 TALK troll 0000 \"spam\"",
                "1005 TALK Ann 0000 \"hi me\"",
            ],
        );
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.messages[0].display_text(), "Ann: hi me");
        assert!(s.is_highlight(&s.messages[0]));
    }

    #[test]
    fn message_history_is_capped() {
        let mut s = ChatState::new("me");
        for i in 0..1100 {
            s.push(MessageKind::Info, None, i.to_string());
        }
        assert_eq!(s.messages.len(), 1000);
        assert_eq!(s.messages.front().unwrap().text, "100");
        assert_eq!(s.dropped, 100);
    }

    #[test]
    fn whispers_reply_and_quote_with_w_command() {
        let mut s = ChatState::new("me");
        feed(
            &mut s,
            &[
                "1004 WHISPER Bob 0000 \"want to scrim?\"",
                "1010 WHISPER Ann 0000 \"hi\"",
            ],
        );
        let incoming = &s.messages[0];
        let outgoing = &s.messages[1];
        assert_eq!(incoming.display_text(), "Bob whispers: want to scrim?");
        assert_eq!(outgoing.display_text(), "You whisper to Ann: hi");
        assert_eq!(reply_prefix(incoming).as_deref(), Some("/w Bob "));
        assert_eq!(reply_prefix(outgoing).as_deref(), Some("/w Ann "));
        assert_eq!(
            compose_outgoing(".map footmen", Some(incoming)),
            "/w Bob .map footmen"
        );
        // Typing the /w yourself does not double it.
        assert_eq!(
            compose_outgoing("/w Bob sure", Some(incoming)),
            "/w Bob sure"
        );
    }

    #[test]
    fn helpers() {
        assert_eq!(format_time(3600 * 13 + 60 * 7), "13:07");
        assert_eq!(initials("bob"), "B");
        assert_eq!(initials("big bob"), "BB");
        assert_eq!(initials(""), "?");
        let q = ChatMessage {
            kind: MessageKind::Talk,
            author: Some("Ann".into()),
            text: "x".into(),
            timestamp: 0,
            peer: None,
        };
        assert_eq!(compose_outgoing("ok", Some(&q)), "[Ann 00:00: \"x\"] ok");
        assert_eq!(reply_prefix(&q), None);
    }
}
