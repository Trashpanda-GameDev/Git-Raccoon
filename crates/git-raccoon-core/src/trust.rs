//! Whether a repository in a folder someone else owns may be opened.
//!
//! Git refuses those unless they're listed in `safe.directory`. libgit2 makes
//! the same check, but more strictly: on Windows it refuses a folder owned by
//! the Administrators group even to an administrator, which Git (and so the
//! user's own command line) accepts. So libgit2's check is turned off and Git
//! is asked instead, and the app agrees with Git. Every `git` the app runs
//! still makes its own check.

use crate::cli;
use crate::{Error, Result};

/// Turns off libgit2's own ownership check, leaving it to [`check`].
/// Call once at startup, before any other thread uses libgit2.
pub fn defer_to_git() {
    #[expect(
        unsafe_code,
        reason = "a libgit2 option set once at startup, before other threads"
    )]
    // SAFETY: it only sets a libgit2 option; no repository is open yet.
    // It can't fail (libgit2 returns an error code only for other options).
    unsafe {
        let _ = git2::opts::set_verify_owner_validation(false);
    }
}

/// Refuses `path` when Git would, with Git's message (which says how to
/// trust the folder). Anything else Git says (not a repository, Git missing)
/// is left for the caller to find out.
pub fn check(path: &str) -> Result<()> {
    match cli::run(path, &["rev-parse", "--git-dir"], None) {
        // Git's message names the setting, whatever language it's in.
        Err(Error::Command(text)) if text.contains("safe.directory") => Err(Error::Command(text)),
        _ => Ok(()),
    }
}
