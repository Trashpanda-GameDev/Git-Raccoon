//! The SSH keys on a signed-in account: listing them (to match them to
//! the keys on this computer by fingerprint), and adding one.

use serde_json::{Value, json};

use super::Kind;
use super::accounts::{Account, api_root, token};
use super::http::{Auth, call_answered};

/// A key the account knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountKey {
    pub id: u64,
    pub title: String,
    /// Its public half, as OpenSSH writes it (without the comment).
    pub key: String,
    /// "SHA256:…", to match against local keys.
    pub fingerprint: Option<String>,
}

/// Why the account's keys couldn't be read or added to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// GitHub: this sign-in can't manage keys (it was made before Git
    /// Raccoon asked for that); signing in again fixes it.
    NeedsPermission,
    /// The key is already on an account (this one, or someone else's).
    AlreadyUsed,
    /// The forge refused, saying why (an organisation's rules, SSO…).
    Refused(String),
    /// Couldn't ask at all.
    Failed(String),
}

impl KeyError {
    /// A plain sentence for it.
    pub fn describe(&self, account: &Account) -> String {
        let place = account.place();
        match self {
            KeyError::NeedsPermission => format!(
                "Your {place} sign-in can't manage SSH keys yet: it was made before Git Raccoon asked for that. Sign in to {place} again to allow it."
            ),
            KeyError::AlreadyUsed => format!(
                "{place} already has this key, on this account or another one. A key can only be on one account: if it's on another, use a different key here."
            ),
            KeyError::Refused(why) => format!("{place} refused: {why}"),
            KeyError::Failed(why) => format!("Couldn't reach {place}: {why}"),
        }
    }
}

/// The account's SSH keys.
pub fn keys(account: &Account) -> Result<Vec<AccountKey>, KeyError> {
    let token = token(account).map_err(KeyError::Failed)?;
    let url = format!(
        "{}/user/keys?per_page=100",
        api_root(account.kind, &account.base)
    );
    let (status, v) = call_answered(&url, &Auth::Bearer(&token), None).map_err(KeyError::Failed)?;
    if status >= 400 {
        return Err(refusal(account.kind, status, &v));
    }
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .map(account_key)
        .collect())
}

/// Adds a public key (an OpenSSH line) to the account, titled `title`.
pub fn add_key(account: &Account, title: &str, public: &str) -> Result<AccountKey, KeyError> {
    let token = token(account).map_err(KeyError::Failed)?;
    let url = format!("{}/user/keys", api_root(account.kind, &account.base));
    // Without the comment: forges show the title instead.
    let key: String = public
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    let body = json!({ "title": title, "key": key });
    let (status, v) =
        call_answered(&url, &Auth::Bearer(&token), Some(&body)).map_err(KeyError::Failed)?;
    if status >= 400 {
        return Err(refusal(account.kind, status, &v));
    }
    Ok(account_key(&v))
}

/// The title a key gets on a forge by default.
pub fn default_title() -> String {
    format!("Git Raccoon on {}", git_raccoon_core::ssh::computer_name())
}

fn account_key(v: &Value) -> AccountKey {
    let key = v["key"].as_str().unwrap_or_default().to_string();
    AccountKey {
        id: v["id"].as_u64().unwrap_or_default(),
        title: v["title"].as_str().unwrap_or_default().to_string(),
        fingerprint: git_raccoon_core::ssh::fingerprint_of(&key),
        key,
    }
}

/// Reads a refusal: GitHub's `{"message", "errors": [{"message"}]}`,
/// GitLab's `{"message": {"key": ["has already been taken"]}}`.
fn refusal(kind: Kind, status: u16, v: &Value) -> KeyError {
    let mut said: Vec<String> = Vec::new();
    match &v["message"] {
        Value::String(m) => said.push(m.clone()),
        Value::Object(fields) => {
            for (field, problems) in fields {
                for p in problems.as_array().into_iter().flatten() {
                    if let Some(p) = p.as_str() {
                        said.push(format!("{field} {p}"));
                    }
                }
            }
        }
        _ => {}
    }
    for e in v["errors"].as_array().into_iter().flatten() {
        if let Some(m) = e["message"].as_str() {
            said.push(m.to_string());
        }
    }
    let text = said.join("; ");
    let lower = text.to_lowercase();
    if lower.contains("already in use") || lower.contains("already been taken") {
        return KeyError::AlreadyUsed;
    }
    // GitHub hides what a token may not touch: a 404 (or 403 naming the
    // missing scope) means this sign-in can't manage keys.
    if kind == Kind::GitHub
        && (status == 404
            || status == 403 && lower.contains("scope")
            || status == 401 && lower.contains("scope"))
    {
        return KeyError::NeedsPermission;
    }
    match status {
        401 => KeyError::Failed("the sign-in has expired: sign in again".into()),
        _ if text.is_empty() => KeyError::Refused(format!("it answered {status}")),
        _ => KeyError::Refused(text),
    }
}

#[cfg(test)]
mod tests {
    use super::super::accounts::{Secret, save, use_memory_store};
    use super::super::http::mock;
    use super::*;

    const KEY: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDK5y8Hw/u3qkFJ2Ki1m8HcE6V3hVYIp5HO8VpZrb3vT";

    fn account(kind: Kind, base: &str) -> Account {
        use_memory_store();
        let a = Account {
            kind,
            base: base.into(),
            login: "rocky".into(),
            client: None,
        };
        save(
            &a,
            &Secret {
                token: "t".into(),
                refresh: None,
                expires: None,
            },
        )
        .unwrap();
        a
    }

    #[test]
    fn lists_an_accounts_keys_with_fingerprints() {
        let (base, seen) = mock::serve(vec![(
            200,
            format!(r#"[{{"id": 7, "title": "Laptop", "key": "{KEY}"}}]"#),
        )]);
        let list = keys(&account(Kind::GitHub, &base)).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Laptop");
        assert_eq!(
            list[0].fingerprint,
            git_raccoon_core::ssh::fingerprint_of(KEY)
        );
        assert!(
            list[0]
                .fingerprint
                .as_deref()
                .is_some_and(|f| f.starts_with("SHA256:"))
        );
        assert_eq!(
            seen.lock().unwrap()[0].0,
            "GET /api/v3/user/keys?per_page=100"
        );
        let (base, seen) = mock::serve(vec![(200, "[]".into())]);
        assert!(keys(&account(Kind::GitLab, &base)).unwrap().is_empty());
        assert_eq!(
            seen.lock().unwrap()[0].0,
            "GET /api/v4/user/keys?per_page=100"
        );
    }

    #[test]
    fn adds_a_key_and_explains_refusals() {
        let (base, seen) = mock::serve(vec![
            (201, format!(r#"{{"id": 8, "title": "Git Raccoon on box", "key": "{KEY}"}}"#)),
            (
                422,
                r#"{"message": "Validation Failed", "errors": [{"resource": "PublicKey", "code": "custom", "field": "key", "message": "key is already in use"}]}"#.into(),
            ),
            (404, r#"{"message": "Not Found"}"#.into()),
            (403, r#"{"message": "SSH keys are disabled for your enterprise"}"#.into()),
        ]);
        let a = account(Kind::GitHub, &base);
        let added = add_key(&a, "Git Raccoon on box", &format!("{KEY} rocky@box")).unwrap();
        assert_eq!(added.id, 8);
        {
            let seen = seen.lock().unwrap();
            assert_eq!(seen[0].0, "POST /api/v3/user/keys");
            assert!(
                seen[0].2.contains(KEY) && !seen[0].2.contains("rocky@box"),
                "{}",
                seen[0].2
            );
        }
        assert_eq!(add_key(&a, "t", KEY), Err(KeyError::AlreadyUsed));
        assert_eq!(add_key(&a, "t", KEY), Err(KeyError::NeedsPermission));
        assert_eq!(
            add_key(&a, "t", KEY),
            Err(KeyError::Refused(
                "SSH keys are disabled for your enterprise".into()
            ))
        );
        // GitLab says it its own way.
        let (base, _) = mock::serve(vec![(
            400,
            r#"{"message": {"key": ["has already been taken"], "fingerprint_sha256": ["has already been taken"]}}"#.into(),
        )]);
        assert_eq!(
            add_key(&account(Kind::GitLab, &base), "t", KEY),
            Err(KeyError::AlreadyUsed)
        );
        assert!(default_title().starts_with("Git Raccoon on "));
    }
}
