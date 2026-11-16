//! Signing in with the browser: OAuth's device flow, on GitHub and
//! GitLab.com. The app shows a short code and opens the forge's page;
//! once the person approves there, polling gets a token. No secret is
//! needed, only the OAuth app's public client id.

use super::Kind;
use super::accounts::Secret;
use super::http::{self, text};

/// The client ids of Git Raccoon's OAuth apps (public, not secrets).
/// Empty until they're registered; builds can also set them through the
/// GIT_RACCOON_GITHUB_CLIENT_ID and GIT_RACCOON_GITLAB_CLIENT_ID variables.
const GITHUB_CLIENT_ID: &str = match option_env!("GIT_RACCOON_GITHUB_CLIENT_ID") {
    Some(id) => id,
    None => "",
};
const GITLAB_CLIENT_ID: &str = match option_env!("GIT_RACCOON_GITLAB_CLIENT_ID") {
    Some(id) => id,
    None => "",
};

/// The client id for signing in to a forge with the browser, if there is
/// one (self-managed GitLab servers would each need their own app).
pub fn client_id(kind: Kind, base: &str) -> Option<&'static str> {
    let id = match (kind, base) {
        (Kind::GitHub, "https://github.com") => GITHUB_CLIENT_ID,
        (Kind::GitLab, "https://gitlab.com") => GITLAB_CLIENT_ID,
        _ => "",
    };
    (!id.is_empty()).then_some(id)
}

/// What the person is shown: the code, and the page to type it on.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceCode {
    pub user_code: String,
    pub page: String,
    device_code: String,
    /// How often to ask (seconds).
    pub interval: u64,
}

/// How signing in is going.
#[derive(Debug, Clone, PartialEq)]
pub enum Poll {
    Waiting,
    /// Asked too often: wait longer.
    SlowDown,
    Done(Secret),
    Failed(String),
}

fn urls(kind: Kind, base: &str) -> (String, String) {
    match kind {
        Kind::GitHub => (
            format!("{base}/login/device/code"),
            format!("{base}/login/oauth/access_token"),
        ),
        Kind::GitLab => (
            format!("{base}/oauth/authorize_device"),
            format!("{base}/oauth/token"),
        ),
    }
}

fn scope(kind: Kind) -> &'static str {
    match kind {
        Kind::GitHub => "repo read:org",
        Kind::GitLab => "api",
    }
}

/// Starts signing in: gets the code to show.
pub fn start(kind: Kind, base: &str, client: &str) -> Result<DeviceCode, String> {
    let (code_url, _) = urls(kind, base);
    let v = http::post_form(
        kind.name(),
        &code_url,
        &[("client_id", client), ("scope", scope(kind))],
    )?;
    if let Some(error) = v["error"].as_str() {
        return Err(format!(
            "{} wouldn't start signing in ({error}): {}",
            kind.name(),
            text(&v, "error_description")
        ));
    }
    let page = v["verification_uri_complete"]
        .as_str()
        .or(v["verification_uri"].as_str())
        .unwrap_or_default()
        .to_string();
    Ok(DeviceCode {
        user_code: text(&v, "user_code"),
        page,
        device_code: text(&v, "device_code"),
        interval: v["interval"].as_u64().unwrap_or(5).max(1),
    })
}

fn secret(v: &serde_json::Value) -> Secret {
    Secret {
        token: text(v, "access_token"),
        refresh: v["refresh_token"].as_str().map(str::to_string),
        expires: v["expires_in"].as_i64().map(|s| crate::settings::now() + s),
    }
}

/// Asks whether the person has approved yet.
pub fn poll(kind: Kind, base: &str, client: &str, code: &DeviceCode) -> Poll {
    let (_, token_url) = urls(kind, base);
    let answer = http::post_form(
        kind.name(),
        &token_url,
        &[
            ("client_id", client),
            ("device_code", &code.device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ],
    );
    match answer {
        Err(e) => Poll::Failed(e),
        Ok(v) => match v["error"].as_str() {
            None if v["access_token"].is_string() => Poll::Done(secret(&v)),
            Some("authorization_pending") => Poll::Waiting,
            Some("slow_down") => Poll::SlowDown,
            Some("expired_token") => Poll::Failed("the code ran out: start again".into()),
            Some("access_denied") => Poll::Failed("it was turned down".into()),
            other => Poll::Failed(format!(
                "{} said {}",
                kind.name(),
                other.unwrap_or("something unexpected")
            )),
        },
    }
}

/// Gets a fresh token with a refresh token (GitLab's expire).
pub fn refresh(kind: Kind, base: &str, refresh: &str) -> Result<Secret, String> {
    let client = client_id(kind, base).ok_or("signing in again is needed")?;
    let (_, token_url) = urls(kind, base);
    let v = http::post_form(
        kind.name(),
        &token_url,
        &[
            ("client_id", client),
            ("refresh_token", refresh),
            ("grant_type", "refresh_token"),
        ],
    )?;
    if v["access_token"].is_string() {
        Ok(secret(&v))
    } else {
        Err(format!("{} wants you to sign in again", kind.name()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::http::mock;
    use super::*;

    #[test]
    fn signs_in_through_the_device_flow() {
        let (base, seen) = mock::serve(vec![
            (
                200,
                r#"{"device_code": "dev", "user_code": "ABCD-1234",
                    "verification_uri": "https://github.com/login/device", "interval": 5}"#
                    .into(),
            ),
            (200, r#"{"error": "authorization_pending"}"#.into()),
            (200, r#"{"error": "slow_down"}"#.into()),
            (
                200,
                r#"{"access_token": "gho_x", "token_type": "bearer"}"#.into(),
            ),
        ]);
        let code = start(Kind::GitHub, &base, "client").unwrap();
        assert_eq!((code.user_code.as_str(), code.interval), ("ABCD-1234", 5));
        assert_eq!(poll(Kind::GitHub, &base, "client", &code), Poll::Waiting);
        assert_eq!(poll(Kind::GitHub, &base, "client", &code), Poll::SlowDown);
        match poll(Kind::GitHub, &base, "client", &code) {
            Poll::Done(s) => assert_eq!(
                (s.token.as_str(), s.refresh, s.expires),
                ("gho_x", None, None)
            ),
            other => panic!("{other:?}"),
        }
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].0, "POST /login/device/code");
        assert!(seen[0].2.contains("client_id=client") && seen[0].2.contains("scope=repo"));
        assert_eq!(seen[1].0, "POST /login/oauth/access_token");
        assert!(seen[1].2.contains("device_code=dev"));
    }

    #[test]
    fn gitlab_tokens_expire_and_ids_are_per_forge() {
        let (base, _) = mock::serve(vec![(
            200,
            r#"{"access_token": "glo", "refresh_token": "r", "expires_in": 7200}"#.into(),
        )]);
        let code = DeviceCode {
            user_code: String::new(),
            page: String::new(),
            device_code: "d".into(),
            interval: 5,
        };
        match poll(Kind::GitLab, &base, "c", &code) {
            Poll::Done(s) => {
                assert_eq!(s.refresh.as_deref(), Some("r"));
                assert!(s.expires.unwrap() > crate::settings::now() + 7000);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(client_id(Kind::GitLab, "https://git.example.com"), None);
    }
}
