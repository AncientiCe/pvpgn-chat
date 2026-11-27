//! UI-independent core of the chat client: the PvPGN protocol, the
//! connection/session, and the chat state. The egui app (`src/main.rs`) and
//! the GPUI prototype (`gpui-app/`) are both built on this.

pub mod chat;
pub mod connect;
pub mod protocol;
pub mod session;
