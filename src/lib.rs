//! UI-independent core of pvpgn-chat: the PvPGN chat-gateway protocol, the
//! connection/session, the chat state and saved accounts. The desktop app in
//! `gpui-app/` is built on this.

pub mod accounts;
pub mod chat;
pub mod connect;
pub mod protocol;
pub mod session;
