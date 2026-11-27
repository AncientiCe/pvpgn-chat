//! Saved logins. The list of accounts (server + username, most recent first)
//! lives in `accounts.json`; passwords live in the OS credential store
//! (Windows Credential Manager, macOS Keychain, Secret Service on Linux),
//! never in a plain file.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::session::Credentials;

const ACCOUNTS_FILE: &str = "accounts.json";
/// Older versions kept one login, password included, in this file.
const LEGACY_CREDENTIALS_FILE: &str = "credentials.json";
const KEYRING_SERVICE: &str = "bnetchat";

/// Servers offered by name in the login form (name, `host:port`).
pub const KNOWN_SERVERS: &[(&str, &str)] = &[("Eurobattle.net", "116.203.95.137:6112")];

/// The friendly name of a known server address, if it is one.
pub fn server_name(address: &str) -> Option<&'static str> {
    KNOWN_SERVERS.iter().find(|(_, a)| *a == address).map(|(name, _)| *name)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SavedAccount {
    pub server: String,
    pub user: String,
}

impl SavedAccount {
    /// Stable id, also the credential-store entry name.
    pub fn key(&self) -> String {
        format!("{}@{}", self.user.to_lowercase(), self.server)
    }

    pub fn label(&self) -> String {
        format!("{}  ·  {}", self.user, server_name(&self.server).unwrap_or(&self.server))
    }
}

/// Where passwords are kept. The real one is the OS credential store; tests
/// use an in-memory map.
pub trait SecretStore {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, key: &str);
}

pub struct OsSecretStore;

impl SecretStore for OsSecretStore {
    fn get(&self, key: &str) -> Option<String> {
        keyring::Entry::new(KEYRING_SERVICE, key).ok()?.get_password().ok()
    }

    fn set(&self, key: &str, secret: &str) -> Result<(), String> {
        keyring::Entry::new(KEYRING_SERVICE, key)
            .and_then(|e| e.set_password(secret))
            .map_err(|e| format!("Could not store password: {e}"))
    }

    fn delete(&self, key: &str) {
        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, key) {
            let _ = entry.delete_credential();
        }
    }
}

pub struct AccountStore<S: SecretStore = OsSecretStore> {
    dir: PathBuf,
    secrets: S,
}

impl AccountStore<OsSecretStore> {
    /// The store in the working directory, backed by the OS credential store.
    pub fn open() -> Self {
        let store = Self { dir: PathBuf::from("."), secrets: OsSecretStore };
        store.migrate_legacy();
        store
    }
}

impl<S: SecretStore> AccountStore<S> {
    pub fn with(dir: impl Into<PathBuf>, secrets: S) -> Self {
        Self { dir: dir.into(), secrets }
    }

    /// Saved accounts, most recently used first.
    pub fn accounts(&self) -> Vec<SavedAccount> {
        std::fs::read_to_string(self.dir.join(ACCOUNTS_FILE))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Full login for a saved account, if its password is in the store.
    pub fn credentials(&self, account: &SavedAccount) -> Option<Credentials> {
        let password = self.secrets.get(&account.key())?;
        Some(Credentials { server: account.server.clone(), user: account.user.clone(), password })
    }

    /// Remember a login that just worked: password into the credential
    /// store, account to the front of the list.
    pub fn remember(&self, cred: &Credentials) -> Result<(), String> {
        let account = SavedAccount { server: cred.server.clone(), user: cred.user.clone() };
        self.secrets.set(&account.key(), &cred.password)?;
        let mut accounts = self.accounts();
        accounts.retain(|a| a.key() != account.key());
        accounts.insert(0, account);
        self.write(&accounts)
    }

    pub fn forget(&self, account: &SavedAccount) -> Result<(), String> {
        self.secrets.delete(&account.key());
        let mut accounts = self.accounts();
        accounts.retain(|a| a.key() != account.key());
        self.write(&accounts)
    }

    fn write(&self, accounts: &[SavedAccount]) -> Result<(), String> {
        let json = serde_json::to_string_pretty(accounts).map_err(|e| e.to_string())?;
        std::fs::write(self.dir.join(ACCOUNTS_FILE), json).map_err(|e| format!("Could not save accounts: {e}"))
    }

    /// Move a plain-text `credentials.json` from older versions into the
    /// store, then delete it so the password is no longer on disk.
    fn migrate_legacy(&self) {
        let path = self.dir.join(LEGACY_CREDENTIALS_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let Ok(cred) = serde_json::from_str::<Credentials>(&text) else { return };
        if cred.server.is_empty() || cred.user.is_empty() {
            return;
        }
        let already_saved = self.accounts().iter().any(|a| a.user.eq_ignore_ascii_case(&cred.user) && a.server == cred.server);
        let result = if already_saved { Ok(()) } else { self.remember(&cred) };
        match result {
            Ok(()) => {
                if std::fs::remove_file(&path).is_ok() {
                    info!("Moved saved login for {} into the credential store", cred.user);
                }
            }
            Err(e) => warn!("Could not migrate {LEGACY_CREDENTIALS_FILE}: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemorySecrets(RefCell<HashMap<String, String>>);

    impl SecretStore for MemorySecrets {
        fn get(&self, key: &str) -> Option<String> {
            self.0.borrow().get(key).cloned()
        }
        fn set(&self, key: &str, secret: &str) -> Result<(), String> {
            self.0.borrow_mut().insert(key.into(), secret.into());
            Ok(())
        }
        fn delete(&self, key: &str) {
            self.0.borrow_mut().remove(key);
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bnetchat-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cred(user: &str, pw: &str) -> Credentials {
        Credentials { server: "host:6112".into(), user: user.into(), password: pw.into() }
    }

    #[test]
    fn remember_orders_by_recency_and_keeps_passwords_out_of_the_file() {
        let dir = temp_dir("order");
        let store = AccountStore::with(&dir, MemorySecrets::default());
        store.remember(&cred("Ann", "pw1")).unwrap();
        store.remember(&cred("Bob", "pw2")).unwrap();
        store.remember(&cred("ann", "pw3")).unwrap(); // same account, new password

        let users: Vec<_> = store.accounts().into_iter().map(|a| a.user).collect();
        assert_eq!(users, vec!["ann", "Bob"]);
        assert_eq!(store.credentials(&store.accounts()[0]).unwrap().password, "pw3");

        let file = std::fs::read_to_string(dir.join(ACCOUNTS_FILE)).unwrap();
        assert!(!file.contains("pw"), "passwords must not be written to accounts.json");
    }

    #[test]
    fn labels_use_known_server_names() {
        let known = SavedAccount { server: "116.203.95.137:6112".into(), user: "Ann".into() };
        let other = SavedAccount { server: "10.0.0.1:6112".into(), user: "Bob".into() };
        assert_eq!(known.label(), "Ann  ·  Eurobattle.net");
        assert_eq!(other.label(), "Bob  ·  10.0.0.1:6112");
    }

    #[test]
    fn forget_removes_account_and_password() {
        let dir = temp_dir("forget");
        let store = AccountStore::with(&dir, MemorySecrets::default());
        store.remember(&cred("Ann", "pw1")).unwrap();
        let ann = store.accounts()[0].clone();
        store.forget(&ann).unwrap();
        assert!(store.accounts().is_empty());
        assert!(store.credentials(&ann).is_none());
    }

    #[test]
    fn legacy_credentials_file_is_imported_and_deleted() {
        let dir = temp_dir("legacy");
        std::fs::write(
            dir.join(LEGACY_CREDENTIALS_FILE),
            r#"{"server":"host:6112","user":"Old","password":"secret"}"#,
        )
        .unwrap();
        let store = AccountStore::with(&dir, MemorySecrets::default());
        store.migrate_legacy();
        assert_eq!(store.accounts()[0].user, "Old");
        assert_eq!(store.credentials(&store.accounts()[0]).unwrap().password, "secret");
        assert!(!dir.join(LEGACY_CREDENTIALS_FILE).exists());
    }
}
