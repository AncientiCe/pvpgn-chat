//! Login form.

use bnetchat::session::Credentials;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, StyledExt as _};
use gpui_kit::*;

pub enum LoginEvent {
    Connect(Credentials),
}

pub struct LoginView {
    server: Entity<InputState>,
    user: Entity<InputState>,
    password: Entity<InputState>,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<LoginEvent> for LoginView {}

impl LoginView {
    pub fn new(error: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let saved = Credentials::load().unwrap_or_default();

        let field = |placeholder: &'static str, value: String, masked: bool, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
                    .masked(masked)
            })
        };
        let server = field("host:port, e.g. 127.0.0.1:6112", saved.server, false, window, cx);
        let user = field("Username", saved.user, false, window, cx);
        let password = field("Password", saved.password, true, window, cx);

        // Enter in any field submits.
        let _subscriptions = [&server, &user, &password]
            .into_iter()
            .map(|input| {
                cx.subscribe_in(input, window, |this, _, event: &InputEvent, _window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.submit(cx);
                    }
                })
            })
            .collect();

        // Focus the first empty field, so a saved login is one Enter away.
        let first_empty = [&server, &user, &password]
            .into_iter()
            .find(|i| i.read(cx).value().is_empty())
            .unwrap_or(&password)
            .clone();
        // Select a prefilled value so typing replaces it instead of mixing
        // into the (masked) saved text.
        first_empty.update(cx, |input, cx| {
            input.focus(window, cx);
            if !input.value().is_empty() {
                input.select_all(window, cx);
            }
        });

        Self { server, user, password, error, _subscriptions }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let cred = Credentials {
            server: self.server.read(cx).value().trim().to_string(),
            user: self.user.read(cx).value().trim().to_string(),
            password: self.password.read(cx).value().to_string(),
        };
        if cred.server.is_empty() || cred.user.is_empty() || cred.password.is_empty() {
            self.error = Some("Server, username and password are all required.".into());
            cx.notify();
            return;
        }
        self.error = None;
        cx.emit(LoginEvent::Connect(cred));
    }
}

impl Render for LoginView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let label = |text: &'static str| div().text_xs().font_semibold().text_color(theme.muted_foreground).child(text);

        div().size_full().flex().items_center().justify_center().child(
            div()
                .w(px(380.))
                .flex()
                .flex_col()
                .gap_4()
                .p_8()
                .rounded(theme.radius_lg)
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .shadow_lg()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_2xl().font_bold().child("Welcome back"))
                        .child(div().text_sm().text_color(theme.muted_foreground).child("Sign in to your PvPGN server")),
                )
                .child(div().flex().flex_col().gap_1p5().child(label("SERVER")).child(Input::new(&self.server)))
                .child(div().flex().flex_col().gap_1p5().child(label("USERNAME")).child(Input::new(&self.user)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .child(label("PASSWORD"))
                        .child(Input::new(&self.password).mask_toggle()),
                )
                .children(self.error.clone().map(|e| div().text_sm().text_color(theme.danger).child(e)))
                .child(
                    Button::new("connect")
                        .primary()
                        .w_full()
                        .label("Log in")
                        .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                ),
        )
    }
}
