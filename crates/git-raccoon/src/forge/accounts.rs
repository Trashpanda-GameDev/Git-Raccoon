//! Accounts on forges. Which accounts there are is in the settings; their
//! tokens are in the system's keychain (Windows Credential Manager, the
//! macOS Keychain, or the Secret Service on Linux), never in a file.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use super::http::{self, Auth, text};
use super::{Kind, Project, Session, oauth};

/// An account on a forge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub kind: Kind,
    /// The forge's web address: "https://github.com", "https://gitlab.com"
    /// or a self-managed GitLab's.
    pub base: String,
    /// Its user name there.
    pub login: String,
    /// Signed in with the browser (OAuth), rather than a pasted token.
    pub oauth: bool,
}

impl Account {
    /// "GitHub", "GitLab.com", or a self-managed GitLab's host.
    pub fn place(&self) -> String {
        match self.base.as_str() {
            "https://github.com" => "GitHub".into(),
            "https://gitlab.com" => "GitLab.com".into(),
            base => base.split_once("://").map_or(base, |(_, h)| h).to_string(),
        }
    }

    /// Whether it's the account for `project`.
    pub fn serves(&self, project: &Project) -> bool {
        self.kind == project.kind && self.base == project.base
    }
}

/// What's kept in the keychain for an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Secret {
    pub token: String,
    /// GitLab's OAuth tokens expire; this gets a new one.
    pub refresh: Option<String>,
    /// When the token expires (seconds since the Unix epoch), if it does.
    pub expires: Option<i64>,
}

const SERVICE: &str = "Git Raccoon";

/// Tests keep tokens here instead of the keychain.
fn memory() -> &'static Mutex<Option<HashMap<String, String>>> {
    static STORE: OnceLock<Mutex<Option<HashMap<String, String>>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(None))
}

/// Keeps tokens in memory from now on, not in the keychain (for tests).
pub fn use_memory_store() {
    if let Ok(mut store) = memory().lock() {
        store.get_or_insert_with(HashMap::new);
    }
}

fn key(account: &Account) -> String {
    format!("{} {}", account.base, account.login)
}

fn entry(account: &Account) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &key(account))
        .map_err(|e| format!("the system keychain isn't available: {e}"))
}

/// Keeps an account's token.
pub fn save(account: &Account, secret: &Secret) -> Result<(), String> {
    let text = serde_json::to_string(secret).map_err(|e| e.to_string())?;
    if let Ok(mut store) = memory().lock()
        && let Some(store) = store.as_mut()
    {
        store.insert(key(account), text);
        return Ok(());
    }
    entry(account)?
        .set_password(&text)
        .map_err(|e| format!("couldn't keep the token in the system keychain: {e}"))
}

/// An account's token.
pub fn load(account: &Account) -> Result<Secret, String> {
    let text = if let Ok(store) = memory().lock()
        && let Some(store) = store.as_ref()
    {
        store
            .get(&key(account))
            .cloned()
            .ok_or("no token kept for this account")?
    } else {
        entry(account)?
            .get_password()
            .map_err(|e| format!("couldn't read the token from the system keychain: {e}"))?
    };
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Forgets an account's token.
pub fn forget(account: &Account) {
    if let Ok(mut store) = memory().lock()
        && let Some(store) = store.as_mut()
    {
        store.remove(&key(account));
        return;
    }
    if let Ok(e) = entry(account) {
        let _ = e.delete_credential();
    }
}

/// The API address for "who am I".
fn user_url(kind: Kind, base: &str) -> String {
    match (kind, base) {
        (Kind::GitHub, "https://github.com") => "https://api.github.com/user".into(),
        (Kind::GitHub, base) => format!("{base}/api/v3/user"),
        (Kind::GitLab, base) => format!("{base}/api/v4/user"),
    }
}

/// Checks a token, and finds whose it is. Returns the account.
pub fn sign_in(kind: Kind, base: &str, secret: &Secret, oauth: bool) -> Result<Account, String> {
    let base = base.trim().trim_end_matches('/');
    let me = http::get(
        kind.name(),
        &user_url(kind, base),
        &Auth::Bearer(&secret.token),
    )?;
    let login = match kind {
        Kind::GitHub => text(&me, "login"),
        Kind::GitLab => text(&me, "username"),
    };
    if login.is_empty() {
        return Err(format!("{} didn't say whose token it is", kind.name()));
    }
    let account = Account {
        kind,
        base: base.to_string(),
        login,
        oauth,
    };
    save(&account, secret)?;
    Ok(account)
}

/// A session for `project` with `account`, renewing an expired GitLab
/// sign-in first.
pub fn session(account: &Account, project: &Project) -> Result<Session, String> {
    let mut secret = load(account)?;
    let now = crate::settings::now();
    if let (Some(expires), Some(refresh)) = (secret.expires, secret.refresh.clone())
        && expires - 60 < now
    {
        secret = oauth::refresh(account.kind, &account.base, &refresh)?;
        save(account, &secret)?;
    }
    Ok(Session::new(project.clone(), secret.token))
}

#[cfg(test)]
mod tests {
    use super::super::http::mock;
    use super::*;

    #[test]
    fn signs_in_with_a_token_and_keeps_it() {
        use_memory_store();
        let (base, seen) = mock::serve(vec![(200, r#"{"username": "rocky"}"#.into())]);
        let secret = Secret {
            token: "glpat-x".into(),
            refresh: None,
            expires: None,
        };
        let account = sign_in(Kind::GitLab, &format!("{base}/"), &secret, false).unwrap();
        assert_eq!(account.login, "rocky");
        assert_eq!(account.base, base, "without the trailing slash");
        assert_eq!(load(&account).unwrap(), secret);
        assert_eq!(seen.lock().unwrap()[0].0, "GET /api/v4/user");
        forget(&account);
        assert!(load(&account).is_err());
        let place = Account {
            base: "https://git.example.com".into(),
            ..account
        };
        assert_eq!(place.place(), "git.example.com");
    }
}
