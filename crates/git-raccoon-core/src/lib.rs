//! Reading Git repositories for Git Raccoon: history and graph layout,
//! commit details and diffs, refs, search, and watching for changes.
//!
//! Nothing here depends on the user interface.

pub mod askpass;
pub mod branch;
mod cli;
pub mod commit;
pub mod conflict;
pub mod create;
pub mod details;
pub mod discard;
mod error;
pub mod file_history;
pub mod graph;
pub mod ignore;
pub mod interactive;
pub mod merge;
pub mod modules;
pub mod refs;
pub mod remote;
pub mod repo;
pub mod request;
pub mod rewrite;
pub mod search;
pub mod stage;
pub mod stash;
pub mod sync;
#[cfg(test)]
mod testutil;
pub mod trust;
pub mod undo;
pub mod watch;
pub mod words;
pub mod working;

pub use error::{Error, Result};

/// Runs `git` with `args` in the repository at `path` and returns what it
/// printed (for queries that read, such as `log` or `diff`).
pub fn git_output(path: &str, args: &[&str]) -> Result<String> {
    cli::run(path, args, None)
}
