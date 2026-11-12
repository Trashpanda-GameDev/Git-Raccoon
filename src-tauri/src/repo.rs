//! Reading a repository's history and refs for the commit graph.

use std::collections::HashMap;

use git2::{Oid, Repository, Sort};
use serde::Serialize;

use crate::graph::{self, Edge};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
    /// A detached HEAD.
    Head,
    Local,
    Remote,
    Tag,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefLabel {
    pub name: String,
    pub kind: RefKind,
    /// The checked-out branch, or the detached HEAD.
    pub is_head: bool,
}

#[derive(Debug, Serialize)]
pub struct GraphCommit {
    pub id: String,
    pub parents: Vec<String>,
    pub summary: String,
    pub author: String,
    pub time: i64,
    pub refs: Vec<RefLabel>,
    pub column: usize,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub commits: Vec<GraphCommit>,
    pub width: usize,
    /// Checked-out branch name, or `None` when HEAD is detached or unborn.
    pub branch: Option<String>,
    /// Absolute path of the working directory (or git dir if bare).
    pub path: String,
}

/// Loads up to `limit` commits reachable from any branch, remote branch, tag
/// or HEAD, newest first, with their graph layout.
pub fn load_graph(path: &str, limit: usize) -> Result<Graph, git2::Error> {
    let repo = Repository::discover(path)?;
    let head = repo.head().ok();
    let head_branch = head.as_ref().filter(|h| h.is_branch()).and_then(|h| h.name().map(String::from));

    let mut walk = repo.revwalk()?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

    let mut labels: HashMap<Oid, Vec<RefLabel>> = HashMap::new();
    for reference in repo.references()? {
        let reference = reference?;
        if reference.symbolic_target().is_some() {
            continue; // e.g. refs/remotes/origin/HEAD
        }
        let kind = if reference.is_branch() {
            RefKind::Local
        } else if reference.is_remote() {
            RefKind::Remote
        } else if reference.is_tag() {
            RefKind::Tag
        } else {
            continue; // notes, stash, etc.
        };
        let Ok(commit) = reference.peel_to_commit() else {
            continue; // tags on trees or blobs
        };
        walk.push(commit.id())?;
        labels.entry(commit.id()).or_default().push(RefLabel {
            name: reference.shorthand().unwrap_or_default().to_string(),
            kind,
            is_head: kind == RefKind::Local && reference.name().map(String::from) == head_branch,
        });
    }

    if let Some(head) = head.as_ref()
        && let Some(id) = head.target()
    {
        walk.push(id)?;
        if !head.is_branch() {
            labels.entry(id).or_default().push(RefLabel {
                name: "HEAD".into(),
                kind: RefKind::Head,
                is_head: true,
            });
        }
    }

    let mut commits = Vec::new();
    for id in walk.take(limit) {
        let commit = repo.find_commit(id?)?;
        let mut refs = labels.remove(&commit.id()).unwrap_or_default();
        refs.sort_by(|a, b| b.is_head.cmp(&a.is_head).then(a.kind.cmp(&b.kind)).then(a.name.cmp(&b.name)));
        commits.push(GraphCommit {
            id: commit.id().to_string(),
            parents: commit.parent_ids().map(|p| p.to_string()).collect(),
            summary: commit.summary().unwrap_or_default().to_string(),
            author: commit.author().name().unwrap_or_default().to_string(),
            time: commit.time().seconds(),
            refs,
            column: 0,
            edges: Vec::new(),
        });
    }

    let pairs: Vec<(&str, Vec<&str>)> = commits
        .iter()
        .map(|c| (c.id.as_str(), c.parents.iter().map(String::as_str).collect()))
        .collect();
    let layout = graph::layout(&pairs);
    for (commit, row) in commits.iter_mut().zip(layout.rows) {
        commit.column = row.column;
        commit.edges = row.edges;
    }

    let path = repo.workdir().unwrap_or_else(|| repo.path());
    Ok(Graph {
        commits,
        width: layout.width,
        branch: head_branch.map(|n| n.trim_start_matches("refs/heads/").to_string()),
        path: path.display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempRepo {
        dir: PathBuf,
        repo: Repository,
    }

    impl TempRepo {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("git-raccoon-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let repo = Repository::init(&dir).unwrap();
            TempRepo { dir, repo }
        }

        fn commit(&self, update_ref: Option<&str>, message: &str, parents: &[Oid]) -> Oid {
            let sig = git2::Signature::now("Raccoon", "raccoon@example.com").unwrap();
            let tree = self.repo.find_tree(self.repo.index().unwrap().write_tree().unwrap()).unwrap();
            let parents: Vec<_> = parents.iter().map(|p| self.repo.find_commit(*p).unwrap()).collect();
            let parents: Vec<_> = parents.iter().collect();
            self.repo.commit(update_ref, &sig, &sig, message, &tree, &parents).unwrap()
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn walks_all_branches_and_labels_refs() {
        let t = TempRepo::new("graph");
        let base = t.commit(Some("HEAD"), "base", &[]);
        let main = t.commit(Some("HEAD"), "on main", &[base]);
        let feature = t.commit(Some("refs/heads/feature"), "on feature", &[base]);
        t.repo.tag_lightweight("v1", &t.repo.find_object(base, None).unwrap(), false).unwrap();

        let g = load_graph(t.dir.to_str().unwrap(), 100).unwrap();
        let branch = g.branch.clone().unwrap();
        assert!(branch == "master" || branch == "main", "{branch}");

        let summaries: Vec<_> = g.commits.iter().map(|c| c.summary.as_str()).collect();
        assert_eq!(summaries.len(), 3);
        assert_eq!(summaries[2], "base");
        assert_eq!(g.width, 2);

        let by_id = |id: Oid| g.commits.iter().find(|c| c.id == id.to_string()).unwrap();
        let main_refs = &by_id(main).refs;
        assert_eq!(main_refs.len(), 1);
        assert!(main_refs[0].is_head && main_refs[0].kind == RefKind::Local);
        assert_eq!(by_id(feature).refs[0].name, "feature");
        assert!(!by_id(feature).refs[0].is_head);
        assert_eq!(by_id(base).refs[0].kind, RefKind::Tag);
    }

    #[test]
    fn detached_head_gets_a_head_label() {
        let t = TempRepo::new("detached");
        let first = t.commit(Some("HEAD"), "first", &[]);
        t.commit(Some("HEAD"), "second", &[first]);
        t.repo.set_head_detached(first).unwrap();

        let g = load_graph(t.dir.to_str().unwrap(), 100).unwrap();
        assert_eq!(g.branch, None);
        let refs = &g.commits[1].refs;
        assert_eq!(refs[0].kind, RefKind::Head);
    }

    #[test]
    fn empty_repo_has_no_commits() {
        let t = TempRepo::new("empty");
        let g = load_graph(t.dir.to_str().unwrap(), 100).unwrap();
        assert!(g.commits.is_empty());
        assert_eq!(g.width, 0);
    }
}
