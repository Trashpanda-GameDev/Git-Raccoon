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
pub mod graph;
pub mod merge;
pub mod refs;
pub mod remote;
pub mod repo;
pub mod rewrite;
pub mod search;
pub mod stage;
pub mod stash;
pub mod sync;
#[cfg(test)]
mod testutil;
pub mod undo;
pub mod watch;
pub mod words;
pub mod working;

pub use error::{Error, Result};
