//! Reading Git repositories for Git Raccoon: history and graph layout,
//! commit details and diffs, refs, search, and watching for changes.
//!
//! Nothing here depends on the user interface.

pub mod details;
mod error;
pub mod graph;
pub mod refs;
pub mod repo;
pub mod search;
pub mod stage;
#[cfg(test)]
mod testutil;
pub mod watch;
pub mod words;
pub mod working;

pub use error::{Error, Result};
