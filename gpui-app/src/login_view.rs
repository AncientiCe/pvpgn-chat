//! Login screen: pick a saved account (password comes from the OS credential
//! store), or enter a new login.

use bnetchat::accounts::{AccountStore, SavedAccount, KNOWN_SERVERS};
use bnetchat::session::Credentials;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectItem, SelectState};
use gpui_kit::component::{ActiveTheme as _, IndexPath, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub enum LoginEvent {
    Connect(Credentials),
}

/// One row in the saved-accounts dropdown.
#[derive(Clone)]
struct AccountItem {
    account: SavedAccount,
    key: String,
}

impl SelectItem for AccountItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.account.label().into()
    }

    fn value(&self) -> &String {
        &self.key
    }
}

/// Value of the "Other" entry in the server dropdown.
const OTHER_SERVER: &str = "other";

/// One row in the server dropdown: a known server, or "Other".
#[derive(Clone)]
struct ServerItem {
    name: String,
    /// `host:port`, or [`OTHER_SERVER`].
    value: String,
}

impl SelectItem for ServerItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.name.clone().into()
    }

    fn value(&self) -> &String {
        &self.value
    }
}

fn server_items() -> Vec<ServerItem> {
    KNOWN_SERVERS
        .iter()
        .map(|(name, address)| ServerItem { name: format!("{name}  ({address})"), value: address.to_string() })
        .chain(std::iter::once(ServerItem { name: "Other…".into(), value: OTHER_SERVER.into() }))
        .collect()
}

pub struct LoginView {
    store: AccountStore,
    accounts: Vec<SavedAccount>,
    picker: Entity<SelectState<Vec<AccountItem>>>,
    /// Show the server/username/password form instead of the dropdown.
    show_form: bool,
    server_picker: Entity<SelectState<Vec<ServerItem>>>,
    /// "Other" is picked: the address is typed into `server`.
    manual_server: bool,
    server: Entity<InputState>,
    user: Entity<InputState>,
    password: Entity<InputState>,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<LoginEvent> for LoginView {}

impl LoginView {
    pub fn new(error: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = AccountStore::open();
        let accounts = store.accounts();

        let items = account_items(&accounts);
        let picker = cx.new(|cx| {
            SelectState::new(items, (!accounts.is_empty()).then(|| IndexPath::new(0)), window, cx)
        });

        let server_picker = cx.new(|cx| SelectState::new(server_items(), Some(IndexPath::new(0)), window, cx));

        let field = |placeholder: &'static str, masked: bool, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).masked(masked))
        };
        let server = field("host:port, e.g. 127.0.0.1:6112", false, window, cx);
        let user = field("Username", false, window, cx);
        let password = field("Password", true, window, cx);

        // Enter in any field submits the form.
        let mut _subscriptions: Vec<Subscription> = [&server, &user, &password]
            .into_iter()
            .map(|input| {
                cx.subscribe_in(input, window, |this, _, event: &InputEvent, _window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.submit_form(cx);
                    }
                })
            })
            .collect();
        _subscriptions.push(cx.subscribe_in(
            &server_picker,
            window,
            |this, _, event: &SelectEvent<Vec<ServerItem>>, window, cx| {
                let SelectEvent::Confirm(value) = event;
                this.manual_server = value.as_deref() == Some(OTHER_SERVER);
                if this.manual_server {
                    this.server.update(cx, |i, cx| i.focus(window, cx));
                }
                cx.notify();
            },
        ));

        let mut view = Self {
            store,
            show_form: accounts.is_empty(),
            accounts,
            picker,
            server_picker,
            manual_server: false,
            server,
            user,
            password,
            error,
            _subscriptions,
        };
        if view.show_form {
            view.open_form(None, window, cx);
        }
        view
    }

    fn selected_account(&self, cx: &App) -> Option<SavedAccount> {
        let key = self.picker.read(cx).selected_value()?.clone();
        self.accounts.iter().find(|a| a.key() == key).cloned()
    }

    /// Log in with the account picked in the dropdown.
    fn login_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(account) = self.selected_account(cx) else { return };
        match self.store.credentials(&account) {
            Some(cred) => {
                self.error = None;
                cx.emit(LoginEvent::Connect(cred));
            }
            None => {
                self.error = Some("No saved password for this account. Enter it once.".into());
                self.open_form(Some(&account), window, cx);
            }
        }
    }

    fn forget_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(account) = self.selected_account(cx) else { return };
        if let Err(e) = self.store.forget(&account) {
            self.error = Some(e);
        }
        self.accounts = self.store.accounts();
        let items = account_items(&self.accounts);
        let has_any = !self.accounts.is_empty();
        self.picker.update(cx, |picker, cx| {
            picker.set_items(items, window, cx);
            if has_any {
                picker.set_selected_index(Some(IndexPath::new(0)), window, cx);
            }
        });
        if !has_any {
            self.open_form(None, window, cx);
        }
        cx.notify();
    }

    /// Show the manual form, prefilled from `account` if given.
    fn open_form(&mut self, account: Option<&SavedAccount>, window: &mut Window, cx: &mut Context<Self>) {
        self.show_form = true;
        let (server, user) = account.map(|a| (a.server.clone(), a.user.clone())).unwrap_or_default();

        // A known server is picked in the dropdown; anything else is "Other".
        let known = KNOWN_SERVERS.iter().position(|(_, address)| *address == server);
        let index = match (known, server.is_empty()) {
            (Some(i), _) => i,
            (None, true) => 0,
            (None, false) => KNOWN_SERVERS.len(),
        };
        self.manual_server = index == KNOWN_SERVERS.len();
        self.server_picker
            .update(cx, |p, cx| p.set_selected_index(Some(IndexPath::new(index)), window, cx));
        let manual = if self.manual_server { server } else { String::new() };
        self.server.update(cx, |i, cx| i.set_value(manual, window, cx));

        self.user.update(cx, |i, cx| i.set_value(user, window, cx));
        self.password.update(cx, |i, cx| i.set_value("", window, cx));
        let manual_server = self.manual_server;
        let first_empty = [&self.server, &self.user, &self.password]
            .into_iter()
            .filter(|i| manual_server || i.entity_id() != self.server.entity_id())
            .find(|i| i.read(cx).value().is_empty())
            .unwrap_or(&self.password)
            .clone();
        first_empty.update(cx, |i, cx| i.focus(window, cx));
        cx.notify();
    }

    fn submit_form(&mut self, cx: &mut Context<Self>) {
        let server = if self.manual_server {
            self.server.read(cx).value().trim().to_string()
        } else {
            self.server_picker.read(cx).selected_value().cloned().unwrap_or_default()
        };
        let cred = Credentials {
            server,
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

    fn render_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xs().font_semibold().text_color(theme.muted_foreground).child("ACCOUNT"))
            .child(Select::new(&self.picker).placeholder("Choose an account").w_full())
            .child(
                Button::new("login-saved")
                    .primary()
                    .w_full()
                    .label("Log in")
                    .on_click(cx.listener(|this, _, window, cx| this.login_selected(window, cx))),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(
                        Button::new("other-account")
                            .ghost()
                            .small()
                            .label("Use another account")
                            .on_click(cx.listener(|this, _, window, cx| this.open_form(None, window, cx))),
                    )
                    .child(
                        Button::new("forget")
                            .ghost()
                            .small()
                            .label("Forget this account")
                            .on_click(cx.listener(|this, _, window, cx| this.forget_selected(window, cx))),
                    ),
            )
    }

    fn render_form(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let label = |text: &'static str| div().text_xs().font_semibold().text_color(theme.muted_foreground).child(text);
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(label("SERVER"))
                    .child(Select::new(&self.server_picker).w_full())
                    .when(self.manual_server, |el| el.child(Input::new(&self.server))),
            )
            .child(div().flex().flex_col().gap_1p5().child(label("USERNAME")).child(Input::new(&self.user)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(label("PASSWORD"))
                    .child(Input::new(&self.password).mask_toggle()),
            )
            .child(
                Button::new("connect")
                    .primary()
                    .w_full()
                    .label("Log in")
                    .on_click(cx.listener(|this, _, _, cx| this.submit_form(cx))),
            )
            .when(!self.accounts.is_empty(), |el| {
                el.child(
                    Button::new("back-to-saved")
                        .ghost()
                        .small()
                        .label("Back to saved accounts")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_form = false;
                            this.error = None;
                            cx.notify();
                        })),
                )
            })
    }
}

fn account_items(accounts: &[SavedAccount]) -> Vec<AccountItem> {
    accounts.iter().map(|a| AccountItem { key: a.key(), account: a.clone() }).collect()
}

impl Render for LoginView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let subtitle = if self.show_form {
            "Sign in to your PvPGN server. The login is saved once it works."
        } else {
            "Pick a saved account."
        };
        let header = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_2xl().font_bold().child("Welcome back"))
            .child(div().text_sm().text_color(theme.muted_foreground).child(subtitle));
        let error = self.error.clone().map(|e| div().text_sm().text_color(theme.danger).child(e));
        let card = div()
            .w(px(400.))
            .flex()
            .flex_col()
            .gap_4()
            .p_8()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_lg()
            .child(header)
            .children(error);

        let card = if self.show_form {
            card.child(self.render_form(cx).into_any_element())
        } else {
            card.child(self.render_picker(cx).into_any_element())
        };

        div().size_full().flex().items_center().justify_center().child(card)
    }
}
