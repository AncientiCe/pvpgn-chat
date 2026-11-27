#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window in release builds

//! pvpgn-chat: desktop chat client for PvPGN servers, built on GPUI.
//!
//! Protocol, session, chat state and saved accounts come from the `bnetchat`
//! library; this crate is the UI. Run with `cargo run -p pvpgn-chat`.

mod chat_view;
mod login_view;

use bnetchat::protocol::ServerEvent;
use bnetchat::session::{Credentials, Session};
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

use crate::chat_view::{ChatView, ChatViewEvent};
use crate::login_view::{LoginEvent, LoginView};

enum Screen {
    Login(Entity<LoginView>),
    Connecting,
    Chat(Entity<ChatView>),
}

struct BnetApp {
    screen: Screen,
    _subscriptions: Vec<Subscription>,
    _tasks: Vec<Task<()>>,
}

impl BnetApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut app = Self {
            screen: Screen::Connecting,
            _subscriptions: vec![],
            _tasks: vec![],
        };
        app.show_login(None, window, cx);
        app
    }

    fn show_login(&mut self, error: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let login = cx.new(|cx| LoginView::new(error, window, cx));
        self._subscriptions =
            vec![
                cx.subscribe_in(&login, window, |this, _, event: &LoginEvent, window, cx| {
                    let LoginEvent::Connect(cred) = event;
                    this.connect(cred.clone(), window, cx);
                }),
            ];
        self.screen = Screen::Login(login);
        cx.notify();
    }

    /// Log in on the background executor, then forward server events to the
    /// chat view as they arrive.
    fn connect(&mut self, cred: Credentials, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = Screen::Connecting;
        cx.notify();
        log::info!("Connecting to {} as {}", cred.server, cred.user);

        let (tx, rx) = smol::channel::unbounded::<ServerEvent>();
        let login = cx.background_executor().spawn(async move {
            let session = Session::connect(&cred, move |event| {
                let _ = tx.try_send(event);
            });
            if session.is_ok() {
                if let Err(e) = bnetchat::accounts::AccountStore::open().remember(&cred) {
                    log::warn!("{e}");
                }
            }
            session
        });

        let task = cx.spawn_in(window, async move |this, cx| {
            let session = login.await;
            let chat = this.update_in(cx, |this, window, cx| match session {
                Ok(session) => {
                    let chat = cx.new(|cx| ChatView::new(session, window, cx));
                    this._subscriptions = vec![cx.subscribe_in(
                        &chat,
                        window,
                        |this, _, event: &ChatViewEvent, window, cx| match event {
                            ChatViewEvent::LoggedOut => this.show_login(None, window, cx),
                        },
                    )];
                    this.screen = Screen::Chat(chat.clone());
                    cx.notify();
                    Some(chat)
                }
                Err(err) => {
                    log::warn!("Login failed: {err}");
                    this.show_login(Some(err), window, cx);
                    None
                }
            });
            let Ok(Some(chat)) = chat else { return };
            let chat = chat.downgrade();

            // Apply events in batches: wait for one, then take whatever else is queued.
            while let Ok(first) = rx.recv().await {
                let mut batch = vec![first];
                while let Ok(more) = rx.try_recv() {
                    batch.push(more);
                }
                if chat
                    .update(cx, |chat, cx| chat.apply_events(batch, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        self._tasks = vec![task];
    }
}

impl Render for BnetApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(match &self.screen {
                Screen::Login(login) => login.clone().into_any_element(),
                Screen::Chat(chat) => chat.clone().into_any_element(),
                Screen::Connecting => div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(gpui_kit::component::spinner::Spinner::new())
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child("Connecting to server…"),
                    )
                    .into_any_element(),
            })
    }
}

/// Dark/light preference, remembered in `theme.json`.
pub fn load_dark_preference() -> bool {
    std::fs::read_to_string("theme.json")
        .ok()
        .and_then(|s| serde_json::from_str::<bool>(&s).ok())
        .unwrap_or(true)
}

pub fn set_dark(dark: bool, window: &mut Window, cx: &mut App) {
    let mode = if dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    Theme::change(mode, Some(window), cx);
    let _ = std::fs::write("theme.json", if dark { "true" } else { "false" });
}

fn main() {
    // Override with RUST_LOG, e.g. `RUST_LOG=bnetchat=debug` to see raw traffic.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,bnetchat=info,bnetchat_gpui=info".into()),
        )
        .init();

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let dark = load_dark_preference();
            Theme::change(
                if dark {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                },
                None,
                cx,
            );

            let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Bnet Chat".into()),
                    ..Default::default()
                }),
                window_min_size: Some(size(px(560.), px(420.))),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| BnetApp::new(window, cx))
            })
            .expect("failed to open window");
            // Closing the window quits the app (which logs out, see ChatView).
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}
