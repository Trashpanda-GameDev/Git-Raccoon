//! Images in pull/merge requests' Markdown that need the account to see
//! (a private repository's uploads): fetched with its token. Every other
//! address is left to egui's own loader.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui::{
    self,
    load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, LoadError},
};

use super::Kind;
use super::http::{self, Auth};

/// A forge signed in to: its web address, its API's, and the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    pub kind: Kind,
    /// "https://gitlab.example.com/team/app": the project's page.
    pub project_web: String,
    /// "https://gitlab.example.com/api/v4/projects/team%2Fapp".
    pub project_api: String,
    /// The forge's own hosts (its pages, and where it keeps uploads).
    pub hosts: Vec<String>,
    pub token: String,
}

enum Fetched {
    Pending,
    Ready(Arc<[u8]>, Option<String>),
    Failed(String),
}

/// Fetches images from the forges signed in to, with their tokens.
#[derive(Default)]
pub struct ForgeImages {
    signed: Mutex<Vec<Signed>>,
    cache: Arc<Mutex<HashMap<String, Fetched>>>,
}

impl ForgeImages {
    /// The forges signed in to, now.
    pub fn set(&self, signed: Vec<Signed>) {
        if let Ok(mut s) = self.signed.lock() {
            *s = signed;
        }
    }

    /// Where to fetch `uri` from, and with which token, if it's one of a
    /// signed-in forge's.
    fn route(&self, uri: &str) -> Option<(String, String)> {
        let signed = self.signed.lock().ok()?;
        let host = uri.split("://").nth(1)?.split('/').next()?;
        let forge = signed.iter().find(|s| s.hosts.iter().any(|h| h == host))?;
        Some((fetch_url(forge, uri), forge.token.clone()))
    }
}

/// Where a forge serves `uri` to its token: GitLab's project uploads come
/// from its API (the page's address wants a browser session).
pub fn fetch_url(forge: &Signed, uri: &str) -> String {
    match uri.strip_prefix(&format!("{}/uploads/", forge.project_web)) {
        Some(rest) if forge.kind == Kind::GitLab => format!("{}/uploads/{rest}", forge.project_api),
        _ => uri.to_string(),
    }
}

impl BytesLoader for ForgeImages {
    fn id(&self) -> &'static str {
        egui::generate_loader_id!(ForgeImages)
    }

    fn load(&self, ctx: &egui::Context, uri: &str) -> BytesLoadResult {
        let Some((url, token)) = self.route(uri) else {
            return Err(LoadError::NotSupported);
        };
        let Ok(mut cache) = self.cache.lock() else {
            return Err(LoadError::NotSupported);
        };
        match cache.get(uri) {
            Some(Fetched::Pending) => return Ok(BytesPoll::Pending { size: None }),
            Some(Fetched::Ready(bytes, mime)) => {
                return Ok(BytesPoll::Ready {
                    size: None,
                    bytes: Bytes::Shared(Arc::clone(bytes)),
                    mime: mime.clone(),
                });
            }
            Some(Fetched::Failed(e)) => return Err(LoadError::Loading(e.clone())),
            None => {}
        }
        cache.insert(uri.to_string(), Fetched::Pending);
        let (cache, uri, ctx) = (Arc::clone(&self.cache), uri.to_string(), ctx.clone());
        std::thread::spawn(move || {
            let fetched = match http::get_bytes(&url, &Auth::Bearer(&token)) {
                Ok((bytes, mime)) => Fetched::Ready(bytes.into(), mime),
                Err(e) => Fetched::Failed(e),
            };
            if let Ok(mut cache) = cache.lock() {
                cache.insert(uri, fetched);
            }
            ctx.request_repaint();
        });
        Ok(BytesPoll::Pending { size: None })
    }

    fn forget(&self, uri: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(uri);
        }
    }

    fn forget_all(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    fn byte_size(&self) -> usize {
        self.cache.lock().map_or(0, |cache| {
            cache
                .values()
                .map(|f| match f {
                    Fetched::Ready(bytes, _) => bytes.len(),
                    _ => 0,
                })
                .sum()
        })
    }

    fn has_pending(&self) -> bool {
        self.cache
            .lock()
            .is_ok_and(|cache| cache.values().any(|f| matches!(f, Fetched::Pending)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetches_gitlab_uploads_through_its_api() {
        let forge = Signed {
            kind: Kind::GitLab,
            project_web: "https://gitlab.example.com/team/app".into(),
            project_api: "https://gitlab.example.com/api/v4/projects/team%2Fapp".into(),
            hosts: vec!["gitlab.example.com".into()],
            token: "t".into(),
        };
        assert_eq!(
            fetch_url(
                &forge,
                "https://gitlab.example.com/team/app/uploads/ab12/shot.png"
            ),
            "https://gitlab.example.com/api/v4/projects/team%2Fapp/uploads/ab12/shot.png"
        );
        let images = ForgeImages::default();
        images.set(vec![forge]);
        assert!(
            images
                .route("https://elsewhere.example.com/x.png")
                .is_none()
        );
        assert_eq!(
            images.route("https://gitlab.example.com/team/app/uploads/ab12/shot.png"),
            Some((
                "https://gitlab.example.com/api/v4/projects/team%2Fapp/uploads/ab12/shot.png"
                    .into(),
                "t".into()
            ))
        );
    }
}
