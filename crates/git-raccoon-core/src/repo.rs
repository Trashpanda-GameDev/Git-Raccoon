//! A repository's history and ref labels, laid out for the commit graph.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use git2::{Oid, Repository, Sort};

use crate::Result;
use crate::graph::{self, Edge};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RefKind {
    /// A detached HEAD.
    Head,
    Local,
    Remote,
    Tag,
}

#[derive(Debug, Clone)]
pub struct RefLabel {
    pub name: String,
    pub kind: RefKind,
    /// The checked-out branch, or the detached HEAD.
    pub is_head: bool,
}

#[derive(Debug)]
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

/// One page of the graph, `commits[i]` being row `offset + i`.
#[derive(Debug)]
pub struct GraphPage {
    /// Identifies the history this page came from; changes on every reload.
    pub snapshot: u64,
    pub offset: usize,
    pub commits: Vec<GraphCommit>,
    /// Number of commits in the whole history.
    pub total: usize,
    /// Number of lanes the whole graph needs.
    pub width: usize,
    /// Checked-out branch name (even before its first commit), or `None`
    /// when HEAD is detached.
    pub branch: Option<String>,
    /// Absolute path of the working directory (or git dir if bare).
    pub path: String,
}

/// The full commit order and graph layout of a repository, computed once so
/// pages of it can be served cheaply as the user scrolls.
pub struct History {
    pub snapshot: u64,
    git_dir: std::path::PathBuf,
    path: String,
    branch: Option<String>,
    /// The commit HEAD points at, if any.
    head: Option<Oid>,
    ids: Vec<Oid>,
    index: HashMap<Oid, usize, BuildHasherDefault<OidHasher>>,
    layout: graph::Layout,
    labels: HashMap<Oid, Vec<RefLabel>>,
}

/// Hashes commit ids, which are already random: their first bytes will do.
#[derive(Default)]
pub struct OidHasher(u64);

impl Hasher for OidHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for (i, b) in bytes.iter().take(8).enumerate() {
            self.0 ^= u64::from(*b) << (i * 8);
        }
    }

    // The length an array is hashed with says nothing: every id is 20 bytes.
    fn write_usize(&mut self, _: usize) {}
}

/// Histories this big get Git's commit-graph cache, if they lack one.
const COMMIT_GRAPH_AT: usize = 50_000;

/// Every commit reachable from `tips`, newest first (parents after
/// children), with its parents, from `git rev-list`.
fn walk_with_git(path: &str, tips: &[Oid]) -> Result<Vec<(Oid, Vec<Oid>)>> {
    let mut pairs = Vec::new();
    if tips.is_empty() {
        return Ok(pairs);
    }
    let input: String = tips.iter().map(|t| format!("{t}\n")).collect();
    crate::cli::run_lines(
        path,
        &["rev-list", "--date-order", "--parents", "--stdin"],
        &input,
        &mut |line| {
            let mut ids = line.split(' ').map(Oid::from_str);
            if let Some(id) = ids.next() {
                let parents = ids.collect::<std::result::Result<Vec<_>, _>>()?;
                pairs.push((id?, parents));
            }
            Ok(())
        },
    )?;
    Ok(pairs)
}

/// Writes Git's commit-graph cache in the background if the repository
/// has none, so the next walk is faster. (Git writes the same file itself
/// during `git gc` and `git maintenance`.)
fn offer_commit_graph(git_dir: &std::path::Path, path: &str) {
    let info = git_dir.join("objects").join("info");
    if info.join("commit-graph").exists() || info.join("commit-graphs").exists() {
        return;
    }
    let path = path.to_string();
    std::thread::spawn(move || {
        let _ = crate::cli::run(&path, &["commit-graph", "write", "--reachable"], None);
    });
}

impl History {
    /// Walks every commit reachable from any branch, remote branch, tag or
    /// HEAD, newest first, and lays out the graph.
    pub fn load(path: &str, snapshot: u64) -> Result<History> {
        let repo = Repository::discover(path)?;
        let head = repo.head().ok();
        let head_branch = match &head {
            Some(h) => h.is_branch().then(|| h.name().map(String::from)).flatten(),
            // No commits yet: HEAD still names the branch they'll go on.
            None => repo
                .find_reference("HEAD")
                .ok()
                .and_then(|r| r.symbolic_target().map(String::from)),
        };

        let mut walk = repo.revwalk()?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

        let mut labels: HashMap<Oid, Vec<RefLabel>> = HashMap::new();
        let mut tips: Vec<Oid> = Vec::new();
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
            tips.push(commit.id());
            labels.entry(commit.id()).or_default().push(RefLabel {
                name: reference.shorthand().unwrap_or_default().to_string(),
                kind,
                is_head: kind == RefKind::Local
                    && reference.name().map(String::from) == head_branch,
            });
        }

        if let Some(head) = head.as_ref()
            && let Some(id) = head.target()
        {
            walk.push(id)?;
            tips.push(id);
            if !head.is_branch() {
                labels.entry(id).or_default().push(RefLabel {
                    name: "HEAD".into(),
                    kind: RefKind::Head,
                    is_head: true,
                });
            }
        }
        for refs in labels.values_mut() {
            refs.sort_by(|a, b| {
                b.is_head
                    .cmp(&a.is_head)
                    .then(a.kind.cmp(&b.kind))
                    .then(a.name.cmp(&b.name))
            });
        }

        // Git's own walk is several times faster on big histories (and uses
        // its commit-graph cache, if there is one). Without git, libgit2's.
        let workdir = repo.workdir().unwrap_or_else(|| repo.path());
        let pairs = match walk_with_git(&workdir.to_string_lossy(), &tips) {
            Ok(pairs) => pairs,
            Err(_) => {
                let mut pairs: Vec<(Oid, Vec<Oid>)> = Vec::new();
                for id in walk {
                    let id = id?;
                    let commit = repo.find_commit(id)?;
                    pairs.push((id, commit.parent_ids().collect()));
                }
                pairs
            }
        };
        if pairs.len() >= COMMIT_GRAPH_AT {
            offer_commit_graph(repo.path(), &workdir.to_string_lossy());
        }
        // The checked-out commit on the left.
        let checked_out = head.as_ref().and_then(git2::Reference::target);
        let layout = graph::layout(&pairs, checked_out.as_ref());
        let ids: Vec<Oid> = pairs.into_iter().map(|(id, _)| id).collect();
        let mut index = HashMap::with_capacity_and_hasher(ids.len(), BuildHasherDefault::default());
        index.extend(ids.iter().enumerate().map(|(i, id)| (*id, i)));

        Ok(History {
            snapshot,
            git_dir: repo.path().to_path_buf(),
            path: workdir.display().to_string(),
            branch: head_branch.map(|n| n.trim_start_matches("refs/heads/").to_string()),
            head: head.as_ref().and_then(|h| h.target()),
            ids,
            index,
            layout,
            labels,
        })
    }

    /// Absolute path of the working directory (or git dir if bare).
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Checked-out branch name (even before its first commit), or `None`
    /// when HEAD is detached.
    /// The row of the commit HEAD points at.
    pub fn head_row(&self) -> Option<usize> {
        self.head.and_then(|id| self.index.get(&id).copied())
    }

    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    /// Number of lanes the whole graph needs.
    pub fn width(&self) -> usize {
        self.layout.width
    }

    /// Commit ids in row order.
    pub fn ids(&self) -> &[Oid] {
        &self.ids
    }

    pub fn open_repo(&self) -> Result<Repository> {
        Ok(Repository::open(&self.git_dir)?)
    }

    pub fn total(&self) -> usize {
        self.ids.len()
    }

    /// The row of a commit, if it is in the history.
    pub fn locate(&self, id: &str) -> Option<usize> {
        Oid::from_str(id)
            .ok()
            .and_then(|id| self.index.get(&id).copied())
    }

    /// Rows `offset .. offset + limit` (clamped to the history's end).
    pub fn page(&self, offset: usize, limit: usize) -> Result<GraphPage> {
        let repo = self.open_repo()?;
        let start = offset.min(self.ids.len());
        let end = offset.saturating_add(limit).min(self.ids.len());
        let mut commits = Vec::with_capacity(end - start);
        for row in start..end {
            let commit = repo.find_commit(self.ids[row])?;
            let layout = &self.layout.rows[row];
            commits.push(GraphCommit {
                id: commit.id().to_string(),
                parents: commit.parent_ids().map(|p| p.to_string()).collect(),
                summary: commit.summary().unwrap_or_default().to_string(),
                author: commit.author().name().unwrap_or_default().to_string(),
                time: commit.time().seconds(),
                refs: self.labels.get(&commit.id()).cloned().unwrap_or_default(),
                column: layout.column,
                edges: layout.edges.to_vec(),
            });
        }
        Ok(GraphPage {
            snapshot: self.snapshot,
            offset: start,
            commits,
            total: self.ids.len(),
            width: self.layout.width,
            branch: self.branch.clone(),
            path: self.path.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    /// libgit2's walk: the fallback when git can't be run.
    fn libgit2_walk(path: &str) -> Vec<(Oid, Vec<Oid>)> {
        let repo = Repository::open(path).unwrap();
        let mut walk = repo.revwalk().unwrap();
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME).unwrap();
        walk.push_glob("refs/*").unwrap();
        walk.map(|id| {
            let id = id.unwrap();
            (id, repo.find_commit(id).unwrap().parent_ids().collect())
        })
        .collect()
    }

    fn tips(path: &str) -> Vec<Oid> {
        let repo = Repository::open(path).unwrap();
        repo.references()
            .unwrap()
            .filter_map(|r| r.ok()?.peel_to_commit().ok().map(|c| c.id()))
            .collect()
    }

    /// Branches and merges, each commit a minute after the last (or all at
    /// once, with `same_time`).
    fn branchy(same_time: bool) -> TempRepo {
        let t = TempRepo::new();
        let run = |n: u64, args: &[&str]| {
            let date = format!(
                "{} +0000",
                1_600_000_000 + if same_time { 0 } else { n * 60 }
            );
            crate::cli::run_env(
                t.path(),
                args,
                None,
                &[
                    ("GIT_AUTHOR_DATE", &date),
                    ("GIT_COMMITTER_DATE", &date),
                    ("GIT_AUTHOR_NAME", "R"),
                    ("GIT_AUTHOR_EMAIL", "r@example.com"),
                    ("GIT_COMMITTER_NAME", "R"),
                    ("GIT_COMMITTER_EMAIL", "r@example.com"),
                ],
            )
            .unwrap();
        };
        let mut n = 0;
        let commit = |n: &mut u64, msg: &str| {
            *n += 1;
            run(*n, &["commit", "-q", "--allow-empty", "-m", msg]);
        };
        commit(&mut n, "base");
        for round in 0..4 {
            let branch = format!("f{round}");
            crate::cli::run(t.path(), &["switch", "-q", "-c", &branch], None).unwrap();
            commit(&mut n, "feature a");
            commit(&mut n, "feature b");
            crate::cli::run(t.path(), &["switch", "-q", "-"], None).unwrap();
            commit(&mut n, "main");
            n += 1;
            run(n, &["merge", "-q", "--no-ff", "--no-edit", &branch]);
        }
        t
    }

    #[test]
    fn git_and_libgit2_walk_alike() {
        // Distinct times: exactly the same order.
        let t = branchy(false);
        let ours = walk_with_git(t.path(), &tips(t.path())).unwrap();
        assert_eq!(ours.len(), 1 + 4 * 4);
        assert_eq!(ours, libgit2_walk(t.path()));

        // All at once (as after a rebase): the same commits, children first.
        let t = branchy(true);
        let ours = walk_with_git(t.path(), &tips(t.path())).unwrap();
        let mut a: Vec<Oid> = ours.iter().map(|p| p.0).collect();
        let mut b: Vec<Oid> = libgit2_walk(t.path()).iter().map(|p| p.0).collect();
        let row: HashMap<Oid, usize> = a.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        for (id, parents) in &ours {
            for p in parents {
                assert!(row[p] > row[id], "a parent comes after its child");
            }
        }
        a.sort();
        b.sort();
        assert_eq!(a, b);
    }

    #[test]
    fn walks_all_branches_and_labels_refs() {
        let t = TempRepo::new();
        let base = t.commit(Some("HEAD"), "base", &[]);
        let main = t.commit(Some("HEAD"), "on main", &[base]);
        let feature = t.commit(Some("refs/heads/feature"), "on feature", &[base]);
        t.repo
            .tag_lightweight("v1", &t.repo.find_object(base, None).unwrap(), false)
            .unwrap();

        let g = History::load(t.path(), 1).unwrap().page(0, 100).unwrap();
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
    fn names_the_branch_of_an_empty_repository() {
        let t = TempRepo::new();
        t.repo.set_head("refs/heads/trunk").unwrap();
        let h = History::load(t.path(), 1).unwrap();
        assert_eq!(h.total(), 0);
        assert_eq!(h.branch(), Some("trunk"));
    }

    #[test]
    fn detached_head_gets_a_head_label() {
        let t = TempRepo::new();
        let first = t.commit(Some("HEAD"), "first", &[]);
        t.commit(Some("HEAD"), "second", &[first]);
        t.repo.set_head_detached(first).unwrap();

        let g = History::load(t.path(), 1).unwrap().page(0, 100).unwrap();
        assert_eq!(g.branch, None);
        let refs = &g.commits[1].refs;
        assert_eq!(refs[0].kind, RefKind::Head);
    }

    #[test]
    fn empty_repo_has_no_commits() {
        let t = TempRepo::new();
        let g = History::load(t.path(), 1).unwrap().page(0, 100).unwrap();
        assert!(g.commits.is_empty());
        assert_eq!(g.width, 0);
    }
}

#[cfg(test)]
mod paging_tests {
    use super::*;
    use crate::testutil::TempRepo;

    #[test]
    fn pages_cover_the_history_and_locate_finds_rows() {
        let t = TempRepo::new();
        let mut ids = Vec::new();
        for i in 0..25 {
            let parents: Vec<Oid> = ids.last().copied().into_iter().collect();
            ids.push(t.commit(Some("HEAD"), &format!("commit {i}"), &parents));
        }
        let h = History::load(t.path(), 7).unwrap();
        assert_eq!(h.total(), 25);

        let first = h.page(0, 10).unwrap();
        let last = h.page(20, 10).unwrap();
        assert_eq!(
            (first.snapshot, first.total, first.commits.len()),
            (7, 25, 10)
        );
        assert_eq!((last.offset, last.commits.len()), (20, 5));
        assert_eq!(first.commits[0].summary, "commit 24");
        assert_eq!(last.commits[4].summary, "commit 0");
        // The edge out of row 9 still points down to row 10.
        assert_eq!(first.commits[9].edges.len(), 1);

        assert_eq!(h.locate(&ids[0].to_string()), Some(24));
        assert_eq!(h.head_row(), Some(0));
        assert_eq!(h.locate("0000000000000000000000000000000000000000"), None);
        assert_eq!(h.locate("not a sha"), None);
        assert!(h.page(100, 10).unwrap().commits.is_empty());
    }
}
