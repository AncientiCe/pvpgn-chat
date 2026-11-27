//! Parsing of PvPGN chat-gateway ("bot protocol") lines into typed events.
//!
//! Line formats follow PvPGN's `bnetd/message.cpp`, e.g.
//! `1005 TALK <name> <flags:04x> "<text>"`. This module has no UI code so any
//! front end can reuse it.

/// One event coming from the server, already parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    /// A user present in the channel (1001), or a flags update (1009).
    User {
        name: String,
        flags: String,
    },
    Join {
        name: String,
        flags: String,
    },
    Leave {
        name: String,
    },
    /// Someone whispered to us.
    Whisper {
        from: String,
        text: String,
    },
    /// Echo of a whisper we sent.
    WhisperSent {
        to: String,
        text: String,
    },
    Talk {
        from: String,
        text: String,
    },
    Emote {
        from: String,
        text: String,
    },
    Broadcast {
        text: String,
    },
    /// We entered a channel; the user list is about to be resent.
    Channel {
        name: String,
    },
    ChannelTopic {
        channel: String,
        topic: String,
    },
    Info {
        text: String,
    },
    Error {
        text: String,
    },
    /// Keep-alive from the server; nothing to show.
    Null,
    /// A known code whose arguments could not be parsed.
    Malformed {
        line: String,
    },
    /// A code this client does not handle.
    Unknown {
        line: String,
    },
    /// The connection ended (server closed it or a read failed).
    Disconnected {
        reason: String,
    },
}

/// Remove one pair of surrounding double quotes, if present.
fn unquote(s: &str) -> String {
    let s = s.trim();
    s.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s)
        .to_string()
}

/// Parse one protocol line (without the trailing `\r\n`).
pub fn parse_line(line: &str) -> ServerEvent {
    let mut parts = line.splitn(3, ' ');
    let code = parts.next().unwrap_or("");
    // The second token repeats the type as text; we dispatch on the numeric code.
    let _kind = parts.next();
    let args = parts.next().unwrap_or("");

    let malformed = || ServerEvent::Malformed {
        line: line.to_string(),
    };
    // `<name> <flags> [rest]`
    let name_flags_rest = || {
        let mut it = args.splitn(3, ' ');
        match (it.next(), it.next()) {
            (Some(name), Some(flags)) if !name.is_empty() => {
                Some((name.to_string(), flags.to_string(), it.next().unwrap_or("")))
            }
            _ => None,
        }
    };

    match code {
        "1001" | "1009" => match name_flags_rest() {
            Some((name, flags, _tag)) => ServerEvent::User { name, flags },
            None => malformed(),
        },
        "1002" => match name_flags_rest() {
            Some((name, flags, _tag)) => ServerEvent::Join { name, flags },
            None => malformed(),
        },
        "1003" => match args.split(' ').next() {
            Some(name) if !name.is_empty() => ServerEvent::Leave {
                name: name.to_string(),
            },
            _ => malformed(),
        },
        "1004" | "1005" | "1010" | "1023" => match name_flags_rest() {
            Some((name, _flags, text)) => {
                let text = unquote(text);
                match code {
                    "1004" => ServerEvent::Whisper { from: name, text },
                    "1005" => ServerEvent::Talk { from: name, text },
                    "1010" => ServerEvent::WhisperSent { to: name, text },
                    _ => ServerEvent::Emote { from: name, text },
                }
            }
            None => malformed(),
        },
        "1006" => ServerEvent::Broadcast {
            text: unquote(args),
        },
        "1007" => ServerEvent::Channel {
            name: unquote(args),
        },
        "1013" => ServerEvent::Error {
            text: "Channel is full".into(),
        },
        "1014" => ServerEvent::Error {
            text: "Channel does not exist".into(),
        },
        "1015" => ServerEvent::Error {
            text: "Channel is restricted".into(),
        },
        "1018" => {
            let text = unquote(args);
            // e.g. `W3 topic: You can now .pub and .gopub from channel`
            match text.find(" topic: ") {
                Some(i) => ServerEvent::ChannelTopic {
                    channel: text[..i].trim().to_string(),
                    topic: text[i + " topic: ".len()..].trim().to_string(),
                },
                None => ServerEvent::Info { text },
            }
        }
        "1019" => ServerEvent::Error {
            text: unquote(args),
        },
        "2000" => ServerEvent::Null,
        _ => ServerEvent::Unknown {
            line: line.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_with_flags_and_tag() {
        assert_eq!(
            parse_line("1001 USER Bob 0010 [W3XP]"),
            ServerEvent::User {
                name: "Bob".into(),
                flags: "0010".into()
            }
        );
        assert_eq!(
            parse_line("1009 USER Bob 0002"),
            ServerEvent::User {
                name: "Bob".into(),
                flags: "0002".into()
            }
        );
    }

    #[test]
    fn parses_talk_and_strips_flags_and_quotes() {
        assert_eq!(
            parse_line("1005 TALK Bob 0000 \"hello \"there\"\""),
            ServerEvent::Talk {
                from: "Bob".into(),
                text: "hello \"there\"".into()
            }
        );
    }

    #[test]
    fn parses_whispers_and_emote() {
        assert_eq!(
            parse_line("1004 WHISPER Bob 0000 \"psst\""),
            ServerEvent::Whisper {
                from: "Bob".into(),
                text: "psst".into()
            }
        );
        assert_eq!(
            parse_line("1010 WHISPER Bob 0000 \"hi\""),
            ServerEvent::WhisperSent {
                to: "Bob".into(),
                text: "hi".into()
            }
        );
        assert_eq!(
            parse_line("1023 EMOTE Bob 0000 \"waves\""),
            ServerEvent::Emote {
                from: "Bob".into(),
                text: "waves".into()
            }
        );
    }

    #[test]
    fn parses_topic_and_plain_info() {
        assert_eq!(
            parse_line("1018 INFO \"W3 topic: no need for whisper\""),
            ServerEvent::ChannelTopic {
                channel: "W3".into(),
                topic: "no need for whisper".into()
            }
        );
        assert_eq!(
            parse_line("1018 INFO \"hello\""),
            ServerEvent::Info {
                text: "hello".into()
            }
        );
    }

    #[test]
    fn parses_channel_name() {
        assert_eq!(
            parse_line("1007 CHANNEL \"W3\""),
            ServerEvent::Channel { name: "W3".into() }
        );
    }

    #[test]
    fn missing_args_are_malformed_not_panics() {
        assert!(matches!(
            parse_line("1002 JOIN"),
            ServerEvent::Malformed { .. }
        ));
        assert!(matches!(
            parse_line("1004 WHISPER Bob"),
            ServerEvent::Malformed { .. }
        ));
        assert!(matches!(parse_line(""), ServerEvent::Unknown { .. }));
        assert_eq!(parse_line("2000 NULL"), ServerEvent::Null);
    }
}
