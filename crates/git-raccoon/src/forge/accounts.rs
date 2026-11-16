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
    /// The OAuth app's client id, when signed in with the browser rather
    /// than with a pasted token (renewing GitLab's sign-ins needs it).
    #[serde(default)]
    pub client: Option<String>,
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

/// The forge's API address.
fn api_root(kind: Kind, base: &str) -> String {
    match (kind, base) {
        (Kind::GitHub, "https://github.com") => "https://api.github.com".into(),
        (Kind::GitHub, base) => format!("{base}/api/v3"),
        (Kind::GitLab, base) => format!("{base}/api/v4"),
    }
}

/// The API address for "who am I".
fn user_url(kind: Kind, base: &str) -> String {
    format!("{}/user", api_root(kind, base))
}

/// Checks a token, and finds whose it is. Returns the account.
pub fn sign_in(
    kind: Kind,
    base: &str,
    secret: &Secret,
    client: Option<String>,
) -> Result<Account, String> {
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
        client,
    };
    save(&account, secret)?;
    Ok(account)
}

/// A session for `project` with `account`, renewing an expired GitLab
/// sign-in first.
pub fn session(account: &Account, project: &Project) -> Result<Session, String> {
    Ok(Session::new(project.clone(), token(account)?))
}

/// An account's token, renewing an expired GitLab sign-in first.
fn token(account: &Account) -> Result<String, String> {
    let mut secret = load(account)?;
    let now = crate::settings::now();
    if let (Some(expires), Some(refresh)) = (secret.expires, secret.refresh.clone())
        && expires - 60 < now
    {
        let client = account
            .client
            .as_deref()
            .or_else(|| oauth::client_id(account.kind, &account.base))
            .ok_or("signing in again is needed")?;
        secret = oauth::refresh(account.kind, &account.base, client, &refresh)?;
        save(account, &secret)?;
    }
    Ok(secret.token)
}

/// A repository of an account's (its own, or one it's a member of).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// "owner/name", or "group/sub/name" on GitLab.
    pub path: String,
    pub description: String,
    pub private: bool,
    /// The address to clone it from (HTTPS).
    pub clone_url: String,
}

fn repo(kind: Kind, v: &serde_json::Value) -> Repo {
    match kind {
        Kind::GitHub => Repo {
            path: text(v, "full_name"),
            description: text(v, "description"),
            private: v["private"].as_bool().unwrap_or(false),
            clone_url: text(v, "clone_url"),
        },
        Kind::GitLab => Repo {
            path: text(v, "path_with_namespace"),
            description: text(v, "description"),
            private: v["visibility"].as_str() != Some("public"),
            clone_url: text(v, "http_url_to_repo"),
        },
    }
}

/// The account's repositories, most recently active first. Blocks.
pub fn repositories(account: &Account) -> Result<Vec<Repo>, String> {
    let token = token(account)?;
    let root = api_root(account.kind, &account.base);
    let url = match account.kind {
        Kind::GitHub => format!(
            "{root}/user/repos?per_page=100&sort=updated&affiliation=owner,collaborator,organization_member"
        ),
        Kind::GitLab => {
            format!("{root}/projects?membership=true&per_page=100&order_by=last_activity_at")
        }
    };
    let list = http::get(account.kind.name(), &url, &Auth::Bearer(&token))?;
    Ok(list
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| repo(account.kind, v))
        .collect())
}

/// Makes a new, empty repository on the account. Returns it. Blocks.
pub fn create_repository(
    account: &Account,
    name: &str,
    description: &str,
    private: bool,
) -> Result<Repo, String> {
    let token = token(account)?;
    let root = api_root(account.kind, &account.base);
    let (url, body) = match account.kind {
        Kind::GitHub => (
            format!("{root}/user/repos"),
            serde_json::json!({ "name": name, "description": description, "private": private }),
        ),
        Kind::GitLab => (
            format!("{root}/projects"),
            serde_json::json!({
                "name": name,
                "description": description,
                "visibility": if private { "private" } else { "public" },
            }),
        ),
    };
    let made = http::post(account.kind.name(), &url, &Auth::Bearer(&token), &body)?;
    let made = repo(account.kind, &made);
    if made.clone_url.is_empty() {
        return Err(format!("{} didn't make it", account.kind.name()));
    }
    Ok(made)
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
        let account = sign_in(Kind::GitLab, &format!("{base}/"), &secret, None).unwrap();
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

    #[test]
    fn lists_and_makes_an_accounts_repositories() {
        use_memory_store();
        let (base, seen) = mock::serve(vec![
            (
                200,
                r#"[{"path_with_namespace": "team/app", "description": "The app",
                     "visibility": "private", "http_url_to_repo": "https://git.example.com/team/app.git"},
                    {"path_with_namespace": "rocky/notes", "description": null,
                     "visibility": "public", "http_url_to_repo": "https://git.example.com/rocky/notes.git"}]"#
                    .into(),
            ),
            (
                201,
                r#"{"path_with_namespace": "rocky/new-thing", "visibility": "private",
                    "http_url_to_repo": "https://git.example.com/rocky/new-thing.git"}"#
                    .into(),
            ),
        ]);
        let account = Account {
            kind: Kind::GitLab,
            base: base.clone(),
            login: "rocky".into(),
            client: None,
        };
        save(
            &account,
            &Secret {
                token: "glpat".into(),
                refresh: None,
                expires: None,
            },
        )
        .unwrap();
        let repos = repositories(&account).unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(
            (repos[0].path.as_str(), repos[0].private, repos[1].private),
            ("team/app", true, false)
        );
        assert_eq!(repos[1].description, "");
        let made = create_repository(&account, "new-thing", "Fresh", true).unwrap();
        assert_eq!(
            made.clone_url,
            "https://git.example.com/rocky/new-thing.git"
        );
        let seen = seen.lock().unwrap();
        assert!(
            seen[0]
                .0
                .starts_with("GET /api/v4/projects?membership=true"),
            "{}",
            seen[0].0
        );
        assert_eq!(seen[1].0, "POST /api/v4/projects");
        let sent: serde_json::Value = serde_json::from_str(&seen[1].2).unwrap();
        assert_eq!(
            (sent["name"].as_str(), sent["visibility"].as_str()),
            (Some("new-thing"), Some("private"))
        );
        forget(&account);
    }

    #[test]
    fn renews_a_self_managed_sign_in_with_its_own_app() {
        use_memory_store();
        let (base, seen) = mock::serve(vec![(
            200,
            r#"{"access_token": "new", "refresh_token": "r2", "expires_in": 7200}"#.into(),
        )]);
        let account = Account {
            kind: Kind::GitLab,
            base: base.clone(),
            login: "rocky".into(),
            client: Some("server-app".into()),
        };
        let old = Secret {
            token: "old".into(),
            refresh: Some("r1".into()),
            expires: Some(crate::settings::now() - 10),
        };
        save(&account, &old).unwrap();
        let project = super::super::Project {
            kind: Kind::GitLab,
            base: base.clone(),
            host: String::new(),
            path: "team/app".into(),
        };
        assert!(session(&account, &project).is_ok());
        assert_eq!(load(&account).unwrap().token, "new");
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].0, "POST /oauth/token");
        assert!(
            seen[0].2.contains("client_id=server-app") && seen[0].2.contains("refresh_token=r1")
        );
        forget(&account);
    }
}
