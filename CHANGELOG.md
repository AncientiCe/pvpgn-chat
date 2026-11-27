# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-11-27

A new desktop client built on [GPUI](https://www.gpui.rs/), the GPU-rendered UI framework from
the Zed editor. It replaces the egui client. Release downloads are now `pvpgn-chat` for
Windows, macOS (Apple silicon) and Linux.

### Upgrading

1. Your saved login moves into the OS credential store on first start (Windows Credential
   Manager, macOS Keychain, Linux kernel keyring). The old plain-text `credentials.json` is
   deleted afterwards.
2. The chat gateway only accepts accounts with `BNET\auth\botlogin` set to `true` on the server.
   Ask a server admin if login says the account has no bot access.

### Added

- **GPUI client.** Channel sidebar, topic header, grouped messages with avatars and
  timestamps, a member list grouped by role, dark and light themes.
- **Saved accounts.** Pick an account from a dropdown and press Enter to log in. Passwords are
  kept in the OS credential store; `accounts.json` lists only server and username. A login is
  saved only after the server accepts it.
- **Server picker.** Eurobattle.net is built in; *Other…* takes any `host:port`.
- **Right-click menus.** On members: whisper, ping, watch, unwatch, friend, ignore. On messages:
  reply, quote, copy.
- **Selectable message text.** Drag to select and copy with Ctrl+C.
- **Debug logging.** Set `RUST_LOG=warn,bnetchat=debug` to see raw server traffic. Passwords
  are never logged.
- MIT license.

### Changed

- **Whisper replies use `/w`.** Reply and Quote on a whisper send `/w name <message>`, so bot
  commands like `.map` work. WC3 has no @-mentions, so the client no longer adds any.
- **Clean logout.** The client sends `/quit` when you log out or close the window, so the server
  frees the account at once instead of keeping it logged in.
- **Clearer login errors.** "Account has no bot access" fails at once instead of after a 10 s
  timeout. "Login failed" now says it can mean a wrong password or a session still open
  elsewhere, since PvPGN sends the same reply for both.
- **Protocol parsing follows PvPGN's line formats.** Chat text no longer shows the flags field
  or quotes. Emotes, broadcasts and channel-full/restricted errors are handled, and blank
  welcome lines are skipped.
- Ignored users' messages are hidden; mentions of your name are highlighted.
- The protocol, session, chat state and saved accounts are a separate `bnetchat` library with
  unit tests and an end-to-end test against a fake chat gateway.

### Fixed

- New messages now show up as soon as they arrive, without moving the mouse first.
- A chat line split across two network reads, including the first one after login, is no
  longer cut in half.
- Short or malformed server lines no longer crash the client.
- A mistyped password no longer overwrites a working saved login.

### Removed

- The egui client.

### Security

- Passwords are no longer stored in plain text.
- Updated `bytes`, `tracing-subscriber` and `url` to fix RUSTSEC-2026-0007, RUSTSEC-2025-0055 and
  RUSTSEC-2026-0592.

## [0.1.0] - 2025-07-05

- egui client: light and dark themes, user list with roles and right-click actions, message
  quoting, logout, and login error handling.

## [0.0.1] - 2025-05-17

- First egui client and cross-platform release builds.

[Unreleased]: https://github.com/AncientiCe/pvpgn-chat/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/AncientiCe/pvpgn-chat/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/AncientiCe/pvpgn-chat/compare/v0.0.1...v0.1.0
[0.0.1]: https://github.com/AncientiCe/pvpgn-chat/releases/tag/v0.0.1
