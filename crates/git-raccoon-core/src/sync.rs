//! Fetch, pull and push, through the user's own `git`, so their SSH agent,
//! keys and credential helpers apply. Progress is reported as Git prints
//! it, and each can be cancelled.

use std::sync::atomic::AtomicBool;

use git2::Repository;

use crate::cli;
use crate::{Error, Result};

/// Where a fetch, pull or push has got to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Progress {
    /// What Git is doing ("Receiving objects").
    pub phase: String,
    /// How far through that phase, if Git says.
    pub percent: Option<u8>,
}

impl Progress {
    /// Reads one of Git's progress lines, such as
    /// "remote: Counting objects:  45% (9/20)" or "Receiving objects: 100% (20/20), done.".
    pub fn parse(line: &str) -> Progress {
        let line = line.trim();
        let line = line.strip_prefix("remote:").map_or(line, str::trim);
        let (phase, rest) = line.split_once(':').unwrap_or((line, ""));
        let percent = rest
            .split_whitespace()
            .find_map(|w| w.strip_suffix('%'))
            .and_then(|n| n.parse::<u8>().ok());
        Progress {
            phase: phase.trim().to_string(),
            percent,
        }
    }
}

/// How a pull combines the upstream's new commits with local ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullMode {
    /// Only if there are no local commits; otherwise [`Error::Diverged`].
    FastForward,
    /// A fast-forward if it can be, otherwise a merge commit.
    Merge,
    /// Local commits replayed on top of the upstream's.
    Rebase,
}

fn adds_hint(text: String) -> Error {
    let lower = text.to_lowercase();
    let hint = if lower.contains("password authentication is not supported") {
        // GitHub: an account password was given, typed or saved long ago.
        Some(
            "GitHub doesn't accept account passwords for Git. Sign in to GitHub in Git Raccoon (Settings, Accounts) and it answers for Git, or give a personal access token as the password. If an old password is saved, remove it from your credential helper (on Windows: Credential Manager, git:https://github.com).",
        )
    } else if lower.contains("terminal prompts disabled")
        || lower.contains("could not read username")
        || lower.contains("authentication failed")
    {
        Some(
            "Git couldn't sign in. Set up a credential helper (Git Credential Manager comes with Git for Windows), or sign in once from a terminal so it remembers you.",
        )
    } else if lower.contains("permission denied (publickey") {
        Some(
            "Git couldn't sign in over SSH. Add your key to your SSH agent (ssh-add), or check the key is registered with the host.",
        )
    } else if lower.contains("host key verification failed") {
        Some(
            "This host isn't known yet. Connect once from a terminal (for example: ssh -T git@github.com) to confirm its key.",
        )
    } else {
        None
    };
    Error::Command(match hint {
        Some(h) => format!("{text}\n\n{h}"),
        None => text,
    })
}

fn run(
    path: &str,
    args: &[&str],
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<String> {
    cli::run_with_progress(path, args, &mut |l| on(Progress::parse(l)), cancel).map_err(|e| match e
    {
        Error::Command(text) => adds_hint(text),
        e => e,
    })
}

/// How a fetch goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchOptions {
    /// Remove remote-tracking branches whose branch is gone from the remote.
    pub prune: bool,
    /// Let fetched refs overwrite local ones even where that loses commits
    /// (`--force`).
    pub force: bool,
}

impl Default for FetchOptions {
    fn default() -> Self {
        FetchOptions {
            prune: true,
            force: false,
        }
    }
}

/// `git fetch` with `options`, then `rest`.
fn fetch_with(
    path: &str,
    options: &FetchOptions,
    rest: &[&str],
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<()> {
    let mut args = vec!["fetch", "--progress"];
    if options.prune {
        args.push("--prune");
    }
    if options.force {
        args.push("--force");
    }
    args.extend(rest);
    run(path, &args, on, cancel).map(drop)
}

/// Fetches from one remote.
pub fn fetch_remote(
    path: &str,
    remote: &str,
    options: &FetchOptions,
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<()> {
    fetch_with(path, options, &[remote], on, cancel)
}

/// Fetches from every remote.
pub fn fetch(
    path: &str,
    options: &FetchOptions,
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<()> {
    fetch_with(path, options, &["--all"], on, cancel)
}

/// Deletes `branch` on `remote` ("origin", "feature"), for everyone (like
/// `git push origin --delete feature`); its remote-tracking branch here
/// goes too.
pub fn delete_remote_branch(
    path: &str,
    remote: &str,
    branch: &str,
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<()> {
    run(
        path,
        &["push", "--progress", remote, "--delete", branch],
        on,
        cancel,
    )
    .map(drop)
}

/// Fetches `source` on `remote` (like a pull request's
/// "pull/7/head") into the local branch `branch`, made or moved to it.
pub fn fetch_into(
    path: &str,
    remote: &str,
    source: &str,
    branch: &str,
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<()> {
    let refspec = format!("+{source}:refs/heads/{branch}");
    run(path, &["fetch", "--progress", remote, &refspec], on, cancel).map(drop)
}

/// Puts `branch` back on `remote` at commit `id` (undoing a delete).
pub fn restore_remote_branch(
    path: &str,
    remote: &str,
    branch: &str,
    id: &str,
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<()> {
    let refspec = format!("{id}:refs/heads/{branch}");
    run(path, &["push", "--progress", remote, &refspec], on, cancel).map(drop)
}

/// The checked-out branch and its upstream remote, if it has one.
struct Current {
    branch: String,
    remote: Option<String>,
    /// The upstream's branch on its remote ("refs/heads/main").
    merge: Option<String>,
}

fn current(path: &str) -> Result<Current> {
    let repo = Repository::discover(path)?;
    let head = repo.head()?;
    if !head.is_branch() {
        return Err(Error::Unsupported(
            "No branch is checked out (detached HEAD). Check out a branch first.".into(),
        ));
    }
    let branch = head.shorthand().unwrap_or_default().to_string();
    let config = repo.config()?;
    let get = |key: &str| config.get_string(&format!("branch.{branch}.{key}")).ok();
    let (remote, merge) = (get("remote"), get("merge"));
    // An upstream on a remote that no longer exists counts as none.
    let remote = remote.filter(|r| r == "." || repo.find_remote(r).is_ok());
    Ok(Current {
        merge: remote.as_ref().and(merge),
        remote,
        branch,
    })
}

/// Pulls into the checked-out branch: from its upstream, or (`from`) from
/// the branch of the same name on that remote. With `stash`, uncommitted
/// changes are put aside and back again; without, Git refuses if they're in
/// the way ([`Error::Dirty`]).
pub fn pull(
    path: &str,
    from: Option<&str>,
    mode: PullMode,
    stash: bool,
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<()> {
    let cur = current(path)?;
    let mut args = vec!["pull"];
    args.extend(match mode {
        PullMode::FastForward => ["--ff-only"].as_slice(),
        PullMode::Merge => &["--no-rebase", "--ff"],
        PullMode::Rebase => &["--rebase"],
    });
    // Even with stashing, changes to the files the pull changes make Git
    // refuse, harmlessly.
    args.push(if stash {
        "--autostash"
    } else {
        "--no-autostash"
    });
    args.push("--progress");
    // The upstream's branch name, else the local one.
    let branch = cur
        .merge
        .as_deref()
        .map_or(cur.branch.as_str(), |m| {
            m.strip_prefix("refs/heads/").unwrap_or(m)
        })
        .to_string();
    match from {
        Some(remote) => args.extend([remote, &branch]),
        None if cur.merge.is_none() => {
            return Err(Error::Unsupported(format!(
                "{} has no upstream branch to pull from. Push it first.",
                cur.branch
            )));
        }
        None => {}
    }
    match run(path, &args, on, cancel) {
        Err(Error::Command(text))
            if mode == PullMode::FastForward
                && (text.contains("Not possible to fast-forward") || text.contains("diverg")) =>
        {
            Err(Error::Diverged(text))
        }
        Err(Error::Command(text))
            if !stash
                && (text.contains("would be overwritten")
                    || text.contains("cannot pull with rebase")) =>
        {
            Err(Error::Dirty(text))
        }
        other => other.map(drop),
    }
}

/// The remote a branch without an upstream is pushed to: "origin", or the
/// only remote there is.
fn default_remote(path: &str) -> Result<String> {
    let repo = Repository::discover(path)?;
    let remotes = repo.remotes()?;
    let names: Vec<&str> = remotes.iter().flatten().collect();
    match names.as_slice() {
        [] => Err(Error::Unsupported(
            "This repository has no remote to push to.".into(),
        )),
        [one] => Ok((*one).to_string()),
        many if many.contains(&"origin") => Ok("origin".into()),
        _ => Err(Error::Unsupported(
            "There are several remotes and none is called origin, so it isn't clear where to push. Set the branch's upstream first.".into(),
        )),
    }
}

/// Pushes the checked-out branch to its upstream, setting one up (on
/// "origin") if it has none. `force` overwrites the remote branch, but only
/// if it is still where we last fetched it (`--force-with-lease`).
pub fn push(
    path: &str,
    force: bool,
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<()> {
    let cur = current(path)?;
    let mut args = vec!["push", "--progress"];
    if force {
        args.push("--force-with-lease");
    }
    let refspec;
    let remote;
    match (&cur.remote, &cur.merge) {
        (Some(r), Some(merge)) if r != "." => {
            remote = r.clone();
            refspec = format!("HEAD:{merge}");
        }
        _ => {
            remote = default_remote(path)?;
            refspec = cur.branch.clone();
            args.push("--set-upstream");
        }
    }
    args.push(&remote);
    args.push(&refspec);
    run_push(path, &args, on, cancel)
}

/// Pushes the checked-out branch to `branch` on `remote`, which it then
/// pushes to and pulls from (its upstream). `force` as for [`push`].
pub fn push_to(
    path: &str,
    remote: &str,
    branch: &str,
    force: bool,
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<()> {
    let cur = current(path)?;
    let mut args = vec!["push", "--progress", "--set-upstream"];
    if force {
        args.push("--force-with-lease");
    }
    let refspec = format!("{}:refs/heads/{branch}", cur.branch);
    args.push(remote);
    args.push(&refspec);
    run_push(path, &args, on, cancel)
}

/// Runs a `git push`, telling a refusal to overwrite the remote's commits
/// ([`Error::Rejected`]) from other failures.
fn run_push(
    path: &str,
    args: &[&str],
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<()> {
    match run(path, args, on, cancel) {
        Err(Error::Command(text))
            if text.contains("[rejected]")
                || text.contains("non-fast-forward")
                || text.contains("stale info") =>
        {
            Err(Error::Rejected(text))
        }
        other => other.map(drop),
    }
}

/// Whether the checked-out branch has an upstream to push to.
pub fn has_upstream(path: &str) -> bool {
    current(path).is_ok_and(|c| c.merge.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;
    use crate::{commit, refs};

    #[test]
    fn parses_progress_lines() {
        assert_eq!(
            Progress::parse("remote: Counting objects:  45% (9/20)"),
            Progress {
                phase: "Counting objects".into(),
                percent: Some(45)
            }
        );
        assert_eq!(
            Progress::parse("Receiving objects: 100% (20/20), 1.2 MiB | 3 MiB/s, done."),
            Progress {
                phase: "Receiving objects".into(),
                percent: Some(100)
            }
        );
        assert_eq!(Progress::parse("From /tmp/remote").percent, None);
    }

    /// Two clones of a shared bare remote, each with an identity.
    struct Setup {
        _remote: TempRepo,
        a: TempRepo,
        b: TempRepo,
        branch: String,
    }

    fn identity(t: &TempRepo) {
        let mut config = t.repo.config().unwrap();
        config.set_str("user.name", "Rocky Raccoon").unwrap();
        config.set_str("user.email", "rocky@example.com").unwrap();
        config.set_bool("commit.gpgsign", false).unwrap();
    }

    fn commit_file(t: &TempRepo, file: &str, text: &str, message: &str) {
        t.write(file, text);
        cli::run(t.path(), &["add", file], None).unwrap();
        commit::commit(t.path(), message, false).unwrap();
    }

    fn setup() -> Setup {
        let remote = TempRepo::new();
        cli::run(remote.path(), &["config", "core.bare", "true"], None).unwrap();
        let a = TempRepo::new();
        identity(&a);
        commit_file(&a, "f.txt", "one\n", "first");
        let branch = a.repo.head().unwrap().shorthand().unwrap().to_string();
        cli::run(
            a.path(),
            &["remote", "add", "origin", &remote.path().replace('\\', "/")],
            None,
        )
        .unwrap();
        let none = AtomicBool::new(false);
        push(a.path(), false, &mut |_| {}, &none).unwrap();
        assert!(has_upstream(a.path()), "the first push sets the upstream");

        let b = TempRepo::new();
        identity(&b);
        cli::run(
            b.path(),
            &["remote", "add", "origin", &remote.path().replace('\\', "/")],
            None,
        )
        .unwrap();
        fetch(b.path(), &FetchOptions::default(), &mut |_| {}, &none).unwrap();
        cli::run(
            b.path(),
            &[
                "checkout",
                "-q",
                "-B",
                &branch,
                "--track",
                &format!("origin/{branch}"),
            ],
            None,
        )
        .unwrap();
        Setup {
            _remote: remote,
            a,
            b,
            branch,
        }
    }

    fn head(t: &TempRepo) -> String {
        crate::undo::head_id(t.path()).unwrap().unwrap()
    }

    #[test]
    fn fetches_a_remote_ref_into_a_branch() {
        let s = setup();
        let none = AtomicBool::new(false);
        // Something on the remote that isn't a branch, like a pull request's head.
        let id = crate::undo::head_id(s.a.path()).unwrap().unwrap();
        cli::run(
            s.a.path(),
            &["push", "-q", "origin", &format!("{id}:refs/pull/7/head")],
            None,
        )
        .unwrap();
        fetch_into(
            s.b.path(),
            "origin",
            "pull/7/head",
            "pr/7",
            &mut |_| {},
            &none,
        )
        .unwrap();
        assert_eq!(crate::branch::tip(s.b.path(), "pr/7").unwrap(), id);
    }

    #[test]
    fn deletes_a_branch_on_the_remote_and_puts_it_back() {
        let s = setup();
        let none = AtomicBool::new(false);
        let remote_has = |name: &str| {
            cli::run(s.a.path(), &["ls-remote", "--heads", "origin", name], None)
                .unwrap()
                .contains(name)
        };
        cli::run(
            s.a.path(),
            &["push", "-q", "origin", "HEAD:refs/heads/topic"],
            None,
        )
        .unwrap();
        fetch(s.a.path(), &FetchOptions::default(), &mut |_| {}, &none).unwrap();
        assert!(remote_has("topic"));
        let id = crate::undo::ref_target(s.a.path(), "refs/remotes/origin/topic").unwrap();

        delete_remote_branch(s.a.path(), "origin", "topic", &mut |_| {}, &none).unwrap();
        assert!(!remote_has("topic"), "gone from the remote");
        assert!(
            crate::undo::ref_target(s.a.path(), "refs/remotes/origin/topic").is_err(),
            "and its remote-tracking branch here"
        );

        restore_remote_branch(s.a.path(), "origin", "topic", &id, &mut |_| {}, &none).unwrap();
        assert!(remote_has("topic"), "back");
    }

    #[test]
    fn prunes_deleted_branches_only_when_asked() {
        let s = setup();
        let none = AtomicBool::new(false);
        cli::run(
            s.a.path(),
            &["push", "-q", "origin", "HEAD:refs/heads/gone"],
            None,
        )
        .unwrap();
        let options = |prune| FetchOptions {
            prune,
            force: false,
        };
        fetch(s.b.path(), &options(true), &mut |_| {}, &none).unwrap();
        let has_gone = || {
            refs::load_refs(s.b.path())
                .unwrap()
                .remotes
                .iter()
                .flat_map(|r| &r.branches)
                .any(|b| b.name == "gone")
        };
        assert!(has_gone());
        cli::run(
            s.a.path(),
            &["push", "-q", "origin", "--delete", "gone"],
            None,
        )
        .unwrap();
        fetch(s.b.path(), &options(false), &mut |_| {}, &none).unwrap();
        assert!(has_gone(), "kept without pruning");
        fetch_remote(s.b.path(), "origin", &options(true), &mut |_| {}, &none).unwrap();
        assert!(!has_gone(), "pruned");
    }

    #[test]
    fn pushes_fetches_and_pulls() {
        let s = setup();
        let none = AtomicBool::new(false);
        commit_file(&s.a, "f.txt", "two\n", "second");
        let mut phases = Vec::new();
        push(s.a.path(), false, &mut |p| phases.push(p), &none).unwrap();

        fetch(s.b.path(), &FetchOptions::default(), &mut |_| {}, &none).unwrap();
        let r = refs::load_refs(s.b.path()).unwrap();
        let local = r.local.iter().find(|b| b.name == s.branch).unwrap();
        assert_eq!(local.upstream.as_ref().unwrap().behind, 1);

        pull(
            s.b.path(),
            None,
            PullMode::FastForward,
            true,
            &mut |_| {},
            &none,
        )
        .unwrap();
        assert_eq!(head(&s.b), head(&s.a));
        assert_eq!(
            std::fs::read_to_string(s.b.dir.join("f.txt")).unwrap(),
            "two\n"
        );
    }

    #[test]
    fn pushes_a_new_branch_where_asked_and_tracks_it() {
        let s = setup();
        let none = AtomicBool::new(false);
        cli::run(s.a.path(), &["checkout", "-q", "-b", "fix/thing"], None).unwrap();
        commit_file(&s.a, "fix.txt", "fixed\n", "fix");
        assert!(!has_upstream(s.a.path()));

        push_to(
            s.a.path(),
            "origin",
            "fix/other-name",
            false,
            &mut |_| {},
            &none,
        )
        .unwrap();
        let r = refs::load_refs(s.a.path()).unwrap();
        let local = r.local.iter().find(|b| b.name == "fix/thing").unwrap();
        let upstream = local.upstream.as_ref().unwrap();
        assert_eq!(upstream.name, "origin/fix/other-name");
        assert_eq!(upstream.ahead, 0);
    }

    #[test]
    fn reports_diverged_pulls_and_rejected_pushes() {
        let s = setup();
        let none = AtomicBool::new(false);
        commit_file(&s.a, "a.txt", "from a\n", "from a");
        push(s.a.path(), false, &mut |_| {}, &none).unwrap();
        commit_file(&s.b, "b.txt", "from b\n", "from b");

        let err = push(s.b.path(), false, &mut |_| {}, &none).unwrap_err();
        assert!(matches!(err, Error::Rejected(_)), "{err}");
        let err = pull(
            s.b.path(),
            None,
            PullMode::FastForward,
            true,
            &mut |_| {},
            &none,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Diverged(_)), "{err}");

        // Rebase (with an uncommitted change, kept), then the push goes through.
        s.b.write("f.txt", "uncommitted\n");
        pull(s.b.path(), None, PullMode::Rebase, true, &mut |_| {}, &none).unwrap();
        assert!(s.b.dir.join("a.txt").exists() && s.b.dir.join("b.txt").exists());
        assert_eq!(
            std::fs::read_to_string(s.b.dir.join("f.txt")).unwrap(),
            "uncommitted\n"
        );
        push(s.b.path(), false, &mut |_| {}, &none).unwrap();
    }

    #[test]
    fn merges_diverged_pulls_and_leaves_changes_when_asked() {
        let s = setup();
        let none = AtomicBool::new(false);
        commit_file(&s.a, "f.txt", "from a\n", "from a");
        push(s.a.path(), false, &mut |_| {}, &none).unwrap();
        commit_file(&s.b, "b.txt", "from b\n", "from b");

        // Changes in the pull's way, not put aside: refused, and kept.
        s.b.write("f.txt", "uncommitted\n");
        let err = pull(s.b.path(), None, PullMode::Merge, false, &mut |_| {}, &none).unwrap_err();
        assert!(matches!(err, Error::Dirty(_)), "{err}");
        assert_eq!(
            std::fs::read_to_string(s.b.dir.join("f.txt")).unwrap(),
            "uncommitted\n"
        );

        // Without them, from the remote named: a merge commit joins both.
        cli::run(s.b.path(), &["checkout", "--", "f.txt"], None).unwrap();
        pull(
            s.b.path(),
            Some("origin"),
            PullMode::Merge,
            false,
            &mut |_| {},
            &none,
        )
        .unwrap();
        let merged = s.b.repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(merged.parent_count(), 2);
        assert!(s.b.dir.join("b.txt").exists());
        assert_eq!(
            std::fs::read_to_string(s.b.dir.join("f.txt")).unwrap(),
            "from a\n"
        );
    }

    #[test]
    fn force_pushes_only_with_lease() {
        let s = setup();
        let none = AtomicBool::new(false);
        commit_file(&s.a, "a.txt", "from a\n", "from a");
        push(s.a.path(), false, &mut |_| {}, &none).unwrap();
        commit_file(&s.b, "b.txt", "from b\n", "from b");
        // b never saw a's commit: the lease protects it, even with force.
        let err = push(s.b.path(), true, &mut |_| {}, &none).unwrap_err();
        assert!(matches!(err, Error::Rejected(_)), "{err}");
        // Once b has fetched (and so has seen it), force replaces it.
        fetch(s.b.path(), &FetchOptions::default(), &mut |_| {}, &none).unwrap();
        push(s.b.path(), true, &mut |_| {}, &none).unwrap();
        fetch(s.a.path(), &FetchOptions::default(), &mut |_| {}, &none).unwrap();
        let r = refs::load_refs(s.a.path()).unwrap();
        let remote = r.remotes[0]
            .branches
            .iter()
            .find(|b| b.name == s.branch)
            .unwrap();
        assert_eq!(remote.target, head(&s.b));
    }

    #[test]
    fn refuses_without_a_branch_or_remote() {
        let t = TempRepo::new();
        identity(&t);
        commit_file(&t, "f.txt", "one\n", "first");
        let none = AtomicBool::new(false);
        let err = push(t.path(), false, &mut |_| {}, &none).unwrap_err();
        assert!(err.to_string().contains("no remote"), "{err}");
        let err = pull(
            t.path(),
            None,
            PullMode::FastForward,
            true,
            &mut |_| {},
            &none,
        )
        .unwrap_err();
        assert!(err.to_string().contains("no upstream"), "{err}");
    }

    #[test]
    fn cancels_a_hanging_fetch() {
        let s = setup();
        // A remote that takes a minute to answer.
        cli::run(
            s.b.path(),
            &[
                "config",
                "remote.origin.uploadpack",
                "sleep 60; git-upload-pack",
            ],
            None,
        )
        .unwrap();
        cli::run(
            s.b.path(),
            &["config", "protocol.file.allow", "always"],
            None,
        )
        .unwrap();
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        let start = std::time::Instant::now();
        let err = fetch(s.b.path(), &FetchOptions::default(), &mut |_| {}, &cancel).unwrap_err();
        assert!(matches!(err, Error::Cancelled), "{err}");
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn explains_sign_in_failures() {
        let e = adds_hint(
            "fatal: could not read Username for 'https://github.com': terminal prompts disabled"
                .into(),
        );
        assert!(e.to_string().contains("credential helper"), "{e}");
        let e = adds_hint(
            "remote: Invalid username or token. Password authentication is not supported for Git operations.\nfatal: Authentication failed for 'https://github.com/owner/repo.git/'"
                .into(),
        );
        assert!(
            e.to_string().contains("doesn't accept account passwords"),
            "{e}"
        );
    }
}
