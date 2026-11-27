# pvpgn-chat

A native desktop chat client for [PvPGN](https://github.com/pvpgn/pvpgn-server) servers
(Warcraft III / Battle.net emulators such as Eurobattle.net), written in Rust.

It logs in through the server's chat gateway, joins the channel and gives you a modern
chat window: live channel messages, whispers, the member list, and saved logins.

![GPUI client demo](demo-gpui.gif)

## Features

- **Live channel chat** with messages grouped by author, timestamps, emotes, joins and leaves.
- **Whispers** are highlighted. Reply or Quote on a whisper answers with `/w name …`, so bot
  commands like `.map footmen` work. WC3 has no @-mentions, so there are none here.
- **Member list** grouped by role (admins, moderators, members), with avatars.
- **Right-click menus**:
  - On a member: whisper, ping, watch/unwatch, add friend, ignore.
  - On a message: reply, quote, copy.
- **Selectable text**: drag to select message text and copy it with Ctrl+C.
- **Saved accounts**: pick an account from a dropdown and press Enter to log in.
  - Passwords are kept in the OS credential store (Windows Credential Manager, macOS Keychain,
    the Linux kernel keyring), never in a file. On Linux the kernel keyring is cleared at reboot.
  - A login is saved only after the server accepts it.
- **Server picker**: Eurobattle.net is built in; choose *Other…* for any `host:port`.
- **Clean logout**: the client sends `/quit` when you log out or close the window, so the
  server frees the account right away.
- **Dark and light themes**.
- **Fast and light**: GPU-rendered UI built on [GPUI](https://www.gpui.rs/), the framework behind
  the Zed editor (and the one Discord chose for its native client).

## Getting started

You need a recent stable [Rust toolchain](https://rustup.rs/).

```sh
git clone https://github.com/AncientiCe/pvpgn-chat.git
cd pvpgn-chat
cargo run -p bnetchat-gpui --release
```

The first build takes a few minutes (GPUI is a large dependency); later builds are quick.

**Logging in:**
1. Pick **Eurobattle.net** (or *Other…* and type `host:port`).
2. Enter your username and password, and press Enter.
3. Next time, your account is in the dropdown: just press Enter.

### Account needs bot access

The client uses PvPGN's chat gateway (the "BOT or Telnet" connection), not the game protocol.
Servers only allow that for accounts with `BNET\auth\botlogin` set to `true`. If you see
*"This account has no chat-gateway (bot) access"*, ask a server admin to enable it.

## Two front ends

The repository has two apps on the same core:

| App | Run | Notes |
|---|---|---|
| GPUI client (`gpui-app/`) | `cargo run -p bnetchat-gpui` | The modern client shown above. Recommended. |
| egui client (`src/main.rs`) | `cargo run` | The original client. Release binaries are built from it. |

Protocol, connection, chat state and saved accounts live in the `bnetchat` library
(`src/lib.rs`), so both apps behave the same.

<details>
<summary>egui client screenshot</summary>

![egui client demo](demo.gif)

</details>

## Troubleshooting

| You see | Meaning |
|---|---|
| *Login failed: wrong username or password, or this account is still logged in elsewhere* | PvPGN sends the same reply for both. If the password is right, an old session is still open: wait for it to time out, or ask an admin to `/kill` it. |
| *This account has no chat-gateway (bot) access* | See [Account needs bot access](#account-needs-bot-access). |
| `0x887A002D` / *Failed to get DXGI debug interface* in the terminal | Harmless: the DirectX debug layer isn't installed. |

To see raw server traffic, run with debug logging (passwords are never logged):

```sh
# PowerShell
$env:RUST_LOG = "warn,bnetchat=debug"; cargo run -p bnetchat-gpui
# bash / zsh
RUST_LOG=warn,bnetchat=debug cargo run -p bnetchat-gpui
```

## Project layout

```text
src/
  lib.rs         bnetchat library: everything UI-independent
  protocol.rs    PvPGN chat-gateway lines -> typed events
  connect.rs     TCP connection and login handshake
  session.rs     logged-in session, background reader, /quit on close
  chat.rs        chat state: messages, users, channel, whisper replies
  accounts.rs    saved accounts + OS credential store
  main.rs ...    egui client (chat_view.rs, login.rs, theme.rs)
gpui-app/        GPUI client (login_view.rs, chat_view.rs)
tests/           end-to-end test against a fake chat gateway
```

## Development

```sh
cargo test                       # unit tests + end-to-end session test
cargo clippy --all-targets       # lints for the library and egui app
cargo build -p bnetchat-gpui     # GPUI client
```

Release binaries for Windows, macOS and Linux are built by GitHub Actions when a `v*` tag is
pushed (`.github/workflows/release.yml`).

## Community

Questions and ideas: [W3 Developers Discord](https://discord.gg/jq6rPHc95D).
