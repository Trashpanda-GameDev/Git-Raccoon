//! Branches, remotes, tags and stashes for the sidebar.

use git2::{BranchType, Oid, Repository};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Upstream {
    /// e.g. "origin/main"
    pub name: String,
    /// Commits on the local branch that the upstream lacks.
    pub ahead: usize,
    /// Commits on the upstream that the local branch lacks.
    pub behind: usize,
    /// The upstream branch was deleted on the remote.
    pub gone: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalBranch {
    pub name: String,
    pub target: String,
    pub is_head: bool,
    pub upstream: Option<Upstream>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBranch {
    /// Name without the remote prefix, e.g. "feature/x".
    pub name: String,
    /// e.g. "origin/feature/x"
    pub full_name: String,
    pub target: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    pub url: Option<String>,
    pub branches: Vec<RemoteBranch>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub name: String,
    /// The commit the tag points at (peeled through annotated tags).
    pub target: String,
    pub annotated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stash {
    pub index: usize,
    pub message: String,
    pub target: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refs {
    pub local: Vec<LocalBranch>,
    pub remotes: Vec<Remote>,
    pub tags: Vec<Tag>,
    pub stashes: Vec<Stash>,
}

fn local_branches(repo: &Repository) -> Result<Vec<LocalBranch>, git2::Error> {
    let mut out = Vec::new();
    for item in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = item?;
        let Some(target) = branch.get().target() else {
            continue;
        };
        let upstream = match branch.upstream() {
            Ok(up) => {
                let name = up.name()?.unwrap_or_default().to_string();
                match up.get().target() {
                    Some(up_target) => {
                        let (ahead, behind) = repo.graph_ahead_behind(target, up_target)?;
                        Some(Upstream {
                            name,
                            ahead,
                            behind,
                            gone: false,
                        })
                    }
                    None => Some(Upstream {
                        name,
                        ahead: 0,
                        behind: 0,
                        gone: true,
                    }),
                }
            }
            // Configured upstream whose remote-tracking branch no longer exists.
            Err(e) if e.code() == git2::ErrorCode::NotFound => {
                let name = branch.name()?.unwrap_or_default();
                configured_upstream(repo, name).map(|name| Upstream {
                    name,
                    ahead: 0,
                    behind: 0,
                    gone: true,
                })
            }
            Err(e) => return Err(e),
        };
        out.push(LocalBranch {
            name: branch.name()?.unwrap_or_default().to_string(),
            target: target.to_string(),
            is_head: branch.is_head(),
            upstream,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// "origin/main" from branch.<name>.remote and branch.<name>.merge, if set.
fn configured_upstream(repo: &Repository, branch: &str) -> Option<String> {
    let config = repo.config().ok()?;
    let remote = config.get_string(&format!("branch.{branch}.remote")).ok()?;
    let merge = config.get_string(&format!("branch.{branch}.merge")).ok()?;
    Some(format!(
        "{remote}/{}",
        merge.trim_start_matches("refs/heads/")
    ))
}

fn remotes(repo: &Repository) -> Result<Vec<Remote>, git2::Error> {
    let mut remotes: Vec<Remote> = repo
        .remotes()?
        .iter()
        .flatten()
        .map(|name| Remote {
            name: name.to_string(),
            url: repo
                .find_remote(name)
                .ok()
                .and_then(|r| r.url().map(String::from)),
            branches: Vec::new(),
        })
        .collect();
    // Longest names first, so "origin/x" isn't claimed by a remote named "or".
    let mut by_length: Vec<usize> = (0..remotes.len()).collect();
    by_length.sort_by_key(|&i| std::cmp::Reverse(remotes[i].name.len()));

    for item in repo.branches(Some(BranchType::Remote))? {
        let (branch, _) = item?;
        if branch.get().symbolic_target().is_some() {
            continue; // origin/HEAD
        }
        let Some(target) = branch.get().target() else {
            continue;
        };
        let full_name = branch.name()?.unwrap_or_default().to_string();
        let owner = by_length
            .iter()
            .copied()
            .find(|&i| full_name.starts_with(&format!("{}/", remotes[i].name)));
        if let Some(i) = owner {
            let name = full_name[remotes[i].name.len() + 1..].to_string();
            remotes[i].branches.push(RemoteBranch {
                name,
                full_name,
                target: target.to_string(),
            });
        }
    }
    for remote in &mut remotes {
        remote.branches.sort_by(|a, b| a.name.cmp(&b.name));
    }
    remotes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(remotes)
}

fn tags(repo: &Repository) -> Result<Vec<Tag>, git2::Error> {
    let mut out: Vec<(i64, Tag)> = Vec::new();
    for name in repo.tag_names(None)?.iter().flatten() {
        let reference = repo.find_reference(&format!("refs/tags/{name}"))?;
        let Ok(commit) = reference.peel_to_commit() else {
            continue;
        };
        let annotated = reference
            .target()
            .and_then(|id| repo.find_tag(id).ok())
            .is_some();
        out.push((
            commit.time().seconds(),
            Tag {
                name: name.to_string(),
                target: commit.id().to_string(),
                annotated,
            },
        ));
    }
    // Newest first; names break ties.
    out.sort_by(|(ta, a), (tb, b)| tb.cmp(ta).then_with(|| b.name.cmp(&a.name)));
    Ok(out.into_iter().map(|(_, t)| t).collect())
}

fn stashes(repo: &mut Repository) -> Result<Vec<Stash>, git2::Error> {
    let mut out = Vec::new();
    repo.stash_foreach(|index, message, id: &Oid| {
        out.push(Stash {
            index,
            message: message.to_string(),
            target: id.to_string(),
        });
        true
    })?;
    Ok(out)
}

pub fn load_refs(path: &str) -> Result<Refs, git2::Error> {
    let mut repo = Repository::discover(path)?;
    Ok(Refs {
        local: local_branches(&repo)?,
        remotes: remotes(&repo)?,
        tags: tags(&repo)?,
        stashes: stashes(&mut repo)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    #[test]
    fn lists_branches_with_upstream_ahead_behind() {
        let t = TempRepo::new();
        t.write("a", "1");
        let base = t.commit_all("base");
        t.write("a", "2");
        let local_only = t.commit_all("local work");
        let head_branch = t.repo.head().unwrap().shorthand().unwrap().to_string();

        // origin/<head> is at base plus one commit the local branch lacks.
        let remote_tip = t.commit(
            Some(&format!("refs/remotes/origin/{head_branch}")),
            "remote work",
            &[base],
        );
        t.repo
            .remote("origin", "https://example.com/repo.git")
            .unwrap();
        let mut config = t.repo.config().unwrap();
        config
            .set_str(&format!("branch.{head_branch}.remote"), "origin")
            .unwrap();
        config
            .set_str(
                &format!("branch.{head_branch}.merge"),
                &format!("refs/heads/{head_branch}"),
            )
            .unwrap();
        t.repo
            .branch("feature/x", &t.repo.find_commit(base).unwrap(), false)
            .unwrap();

        let refs = load_refs(t.path()).unwrap();
        let names: Vec<_> = refs.local.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"feature/x"));
        let current = refs.local.iter().find(|b| b.is_head).unwrap();
        assert_eq!(current.target, local_only.to_string());
        let up = current.upstream.as_ref().unwrap();
        assert_eq!(up.name, format!("origin/{head_branch}"));
        assert_eq!((up.ahead, up.behind, up.gone), (1, 1, false));
        assert!(
            refs.local
                .iter()
                .find(|b| b.name == "feature/x")
                .unwrap()
                .upstream
                .is_none()
        );

        assert_eq!(refs.remotes.len(), 1);
        assert_eq!(
            refs.remotes[0].url.as_deref(),
            Some("https://example.com/repo.git")
        );
        assert_eq!(refs.remotes[0].branches[0].name, head_branch);
        assert_eq!(refs.remotes[0].branches[0].target, remote_tip.to_string());
    }

    #[test]
    fn reports_a_gone_upstream() {
        let t = TempRepo::new();
        t.write("a", "1");
        t.commit_all("base");
        let head_branch = t.repo.head().unwrap().shorthand().unwrap().to_string();
        let mut config = t.repo.config().unwrap();
        config
            .set_str(&format!("branch.{head_branch}.remote"), "origin")
            .unwrap();
        config
            .set_str(&format!("branch.{head_branch}.merge"), "refs/heads/deleted")
            .unwrap();

        let refs = load_refs(t.path()).unwrap();
        let up = refs.local[0].upstream.as_ref().unwrap();
        assert!(up.gone);
        assert_eq!(up.name, "origin/deleted");
    }

    #[test]
    fn lists_tags_newest_first_and_peels_annotated_tags() {
        let t = TempRepo::new();
        t.write("a", "1");
        let first = t.commit_all("first");
        t.write("a", "2");
        let second = t.commit_all("second");
        let sig = git2::Signature::now("Raccoon", "raccoon@example.com").unwrap();
        // Same-second commits: the name tiebreak keeps the order stable.
        t.repo
            .tag_lightweight("v1", &t.repo.find_object(first, None).unwrap(), false)
            .unwrap();
        t.repo
            .tag(
                "v2",
                &t.repo.find_object(second, None).unwrap(),
                &sig,
                "release",
                false,
            )
            .unwrap();

        let refs = load_refs(t.path()).unwrap();
        let tags: Vec<_> = refs
            .tags
            .iter()
            .map(|t| (t.name.as_str(), t.annotated))
            .collect();
        assert_eq!(tags, [("v2", true), ("v1", false)]);
        assert_eq!(refs.tags[0].target, second.to_string());
    }

    #[test]
    fn lists_stashes() {
        let mut t = TempRepo::new();
        t.write("a", "1");
        t.commit_all("base");
        t.write("a", "dirty");
        let sig = git2::Signature::now("Raccoon", "raccoon@example.com").unwrap();
        let id = t.repo.stash_save(&sig, "half-done work", None).unwrap();

        let refs = load_refs(t.path()).unwrap();
        assert_eq!(refs.stashes.len(), 1);
        assert_eq!(refs.stashes[0].index, 0);
        assert!(refs.stashes[0].message.contains("half-done work"));
        assert_eq!(refs.stashes[0].target, id.to_string());
    }
}
