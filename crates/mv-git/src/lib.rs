//! Git and git-lfs process runner.
//!
//! Runs the bundled `git` and `git-lfs` executables as child processes,
//! parses their output, talks to the LFS lock API over HTTP and manages
//! host credentials. Arguments are always passed as arrays, never as shell
//! strings; no libgit2/gitoxide for network or LFS work.
//!
//! Status: scaffold only. The real implementation arrives with its task in
//! `docs/PLAN.md`.

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GitError {
    /// The requested operation has not been implemented yet.
    #[error("not implemented yet: {0}")]
    Unimplemented(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_displays_operation() {
        let err = GitError::Unimplemented("smoke");
        assert_eq!(err.to_string(), "not implemented yet: smoke");
    }
}
