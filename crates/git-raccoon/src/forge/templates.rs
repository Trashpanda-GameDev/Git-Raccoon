//! The .gitignore and licence templates a forge offers, for a new
//! repository's first commit: GitHub's (which anyone may read, signed in or
//! not) and GitLab's.

use super::Kind;
use super::accounts::api_root;
use super::http::{self, Auth, text};

/// Where templates come from: a forge, and a token for it if signed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub kind: Kind,
    pub base: String,
    pub token: Option<String>,
}

impl Source {
    /// GitHub's, without signing in.
    pub fn github() -> Self {
        Source {
            kind: Kind::GitHub,
            base: "https://github.com".into(),
            token: None,
        }
    }

    fn auth(&self) -> Auth<'_> {
        self.token.as_deref().map_or(Auth::None, Auth::Bearer)
    }

    fn get(&self, path: &str) -> Result<serde_json::Value, String> {
        let url = format!("{}{path}", api_root(self.kind, &self.base));
        http::get(self.kind.name(), &url, &self.auth())
    }
}

/// A template's key (to fetch it by) and its name (to show).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub key: String,
    pub name: String,
}

/// What a forge offers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Templates {
    pub gitignores: Vec<Template>,
    pub licenses: Vec<Template>,
}

/// The templates `source` offers. Blocks.
pub fn list(source: &Source) -> Result<Templates, String> {
    let entries = |v: serde_json::Value, key: &str| -> Vec<Template> {
        v.as_array()
            .into_iter()
            .flatten()
            .map(|t| match t.as_str() {
                // GitHub lists .gitignores by name alone.
                Some(name) => Template {
                    key: name.into(),
                    name: name.into(),
                },
                None => Template {
                    key: text(t, key),
                    name: text(t, "name"),
                },
            })
            .filter(|t| !t.key.is_empty())
            .collect()
    };
    let (gitignores, licenses) = match source.kind {
        Kind::GitHub => (
            source.get("/gitignore/templates")?,
            source.get("/licenses")?,
        ),
        Kind::GitLab => (
            source.get("/templates/gitignores")?,
            source.get("/templates/licenses")?,
        ),
    };
    Ok(Templates {
        gitignores: entries(gitignores, "key"),
        licenses: entries(licenses, "key"),
    })
}

/// A .gitignore template's text. Blocks.
pub fn gitignore(source: &Source, key: &str) -> Result<String, String> {
    let key = http::encode(key);
    Ok(match source.kind {
        Kind::GitHub => text(
            &source.get(&format!("/gitignore/templates/{key}"))?,
            "source",
        ),
        Kind::GitLab => text(
            &source.get(&format!("/templates/gitignores/{key}"))?,
            "content",
        ),
    })
}

/// A licence's text, made out to `holder` this `year`. Blocks.
pub fn license(source: &Source, key: &str, holder: &str, year: i32) -> Result<String, String> {
    let key = http::encode(key);
    Ok(match source.kind {
        Kind::GitHub => text(&source.get(&format!("/licenses/{key}"))?, "body")
            .replace("[year]", &year.to_string())
            .replace("[fullname]", holder),
        Kind::GitLab => text(
            &source.get(&format!(
                "/templates/licenses/{key}?fullname={}",
                http::encode(holder)
            ))?,
            "content",
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::super::http::mock;
    use super::*;

    #[test]
    fn lists_and_fills_in_githubs_templates() {
        let (base, seen) = mock::serve(vec![
            (200, r#"["Rust", "Unity"]"#.into()),
            (
                200,
                r#"[{"key": "mit", "name": "MIT License", "spdx_id": "MIT"}]"#.into(),
            ),
            (200, r#"{"name": "Rust", "source": "target/\n"}"#.into()),
            (
                200,
                r#"{"key": "mit", "body": "Copyright (c) [year] [fullname]\n"}"#.into(),
            ),
        ]);
        let source = Source {
            kind: Kind::GitHub,
            base,
            token: None,
        };
        let t = list(&source).unwrap();
        assert_eq!(t.gitignores[1].name, "Unity");
        assert_eq!(t.licenses[0].key, "mit");
        assert_eq!(t.licenses[0].name, "MIT License");
        assert_eq!(gitignore(&source, "Rust").unwrap(), "target/\n");
        assert_eq!(
            license(&source, "mit", "Rocky Raccoon", 2026).unwrap(),
            "Copyright (c) 2026 Rocky Raccoon\n"
        );
        let asked: Vec<String> = seen.lock().unwrap().iter().map(|s| s.0.clone()).collect();
        assert_eq!(
            asked,
            [
                "GET /api/v3/gitignore/templates",
                "GET /api/v3/licenses",
                "GET /api/v3/gitignore/templates/Rust",
                "GET /api/v3/licenses/mit"
            ]
        );
    }

    #[test]
    fn fetches_gitlabs_with_the_holder_filled_in_there() {
        let (base, seen) = mock::serve(vec![(
            200,
            r#"{"key": "mit", "content": "Copyright (c) 2026 Rocky\n"}"#.into(),
        )]);
        let source = Source {
            kind: Kind::GitLab,
            base,
            token: Some("glpat".into()),
        };
        assert_eq!(
            license(&source, "mit", "Rocky", 2026).unwrap(),
            "Copyright (c) 2026 Rocky\n"
        );
        let (asked, headers, _) = seen.lock().unwrap()[0].clone();
        assert_eq!(asked, "GET /api/v4/templates/licenses/mit?fullname=Rocky");
        assert!(headers.contains("authorization: bearer glpat"));
    }
}
