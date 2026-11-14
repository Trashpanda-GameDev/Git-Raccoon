use thiserror::Error;

/// Everything that can go wrong reading a repository.
#[derive(Debug, Error)]
pub enum Error {
    #[error("{}", .0.message())]
    Git(#[from] git2::Error),
    #[error("{0} is not changed in this commit")]
    NotChanged(String),
    #[error("could not watch the repository: {0}")]
    Watch(#[from] notify::Error),
    #[error("{0} changed since it was shown; look at it again and retry")]
    Stale(String),
    #[error("not possible: {0}")]
    Unsupported(String),
    /// A `git` command failed; the text is what it printed.
    #[error("{0}")]
    Command(String),
}

impl Error {
    /// Whether the path given was not inside a Git repository.
    pub fn is_not_a_repo(&self) -> bool {
        matches!(self, Error::Git(e) if e.code() == git2::ErrorCode::NotFound && e.class() == git2::ErrorClass::Repository)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
