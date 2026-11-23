//! The SSH host keys forges publish, read from their APIs where they have
//! one (GitHub's `GET /meta`), to check a host's key against.

use super::http::{Auth, get};

/// Reads GitHub's SSH host key fingerprints from its API (at `api`, e.g.
/// "https://api.github.com") and uses them for the checks from now on.
pub fn refresh_github(api: &str) -> Result<usize, String> {
    let v = get("GitHub", &format!("{api}/meta"), &Auth::None)?;
    let fingerprints: Vec<String> = v["ssh_key_fingerprints"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(_, f)| f.as_str().map(str::to_string))
        .filter(|f| f.starts_with("SHA256:"))
        .collect();
    let n = fingerprints.len();
    git_raccoon_core::ssh::set_github_fingerprints(fingerprints);
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::super::http::mock;
    use super::*;
    use git_raccoon_core::ssh::{HostCheck, check_host_key};

    #[test]
    fn reads_githubs_fingerprints_from_its_api() {
        let (base, seen) = mock::serve(vec![(
            200,
            r#"{"ssh_key_fingerprints": {"SHA256_ECDSA": "SHA256:p2QAMXNIC1TJYWeIOttrVc98/R1BUFWu3/LiyKgUfQM", "SHA256_ED25519": "SHA256:new-one", "SHA256_RSA": "SHA256:uNiVztksCsDhcc0u9e8BujQXVUpKZIDTMczCvj3tD2s"}}"#.into(),
        )]);
        assert_eq!(refresh_github(&base), Ok(3));
        assert_eq!(seen.lock().unwrap()[0].0, "GET /meta");
        assert_eq!(
            check_host_key("github.com", "SHA256:new-one"),
            HostCheck::Published("GitHub")
        );
        // Back to the built-in list.
        git_raccoon_core::ssh::set_github_fingerprints(Vec::new());
        assert_eq!(
            check_host_key(
                "github.com",
                "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU"
            ),
            HostCheck::Published("GitHub")
        );
    }
}
