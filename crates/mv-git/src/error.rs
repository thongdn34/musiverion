//! Error types for the crate.
//!
//! Errors never carry environment values: those will later hold credentials.
//! Arguments and the tail of stderr are included because they are what makes
//! a failure diagnosable; callers must not put secrets in arguments.

use std::path::PathBuf;
use std::time::Duration;

/// Maximum number of stderr bytes kept in a [`GitError::NonZeroExit`].
pub const STDERR_TAIL_LIMIT: usize = 8 * 1024;

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GitError {
    /// No `git` executable was found in the bundled directory or on `PATH`.
    #[error("git executable not found (bundled directory or PATH)")]
    GitNotFound,

    /// No `git-lfs` executable was found in the bundled directory or on `PATH`.
    #[error("git-lfs executable not found (bundled directory or PATH)")]
    GitLfsNotFound,

    /// The child process could not be started.
    #[error("failed to start {program}: {source}")]
    Spawn {
        /// Program that failed to start (as given, lossy).
        program: String,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },

    /// An I/O error while talking to a running child (pipes, wait, progress file).
    #[error("i/o error while running {program}: {source}")]
    Io {
        /// Program being run (lossy).
        program: String,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },

    /// The child exited with a status the caller did not accept.
    #[error("{program} {args} failed ({}): {stderr}", describe_code(*.code))]
    NonZeroExit {
        /// Program that ran (file name, lossy).
        program: String,
        /// Arguments, lossy and joined by spaces, for diagnostics only.
        args: String,
        /// Exit code, or `None` if the child was terminated by a signal.
        code: Option<i32>,
        /// Tail of stderr (at most [`STDERR_TAIL_LIMIT`] bytes, lossy UTF-8).
        stderr: String,
    },

    /// The run was cancelled through its cancellation token.
    #[error("{program} was cancelled")]
    Cancelled {
        /// Program that was cancelled (lossy).
        program: String,
    },

    /// The run exceeded its timeout and was terminated.
    #[error("{program} timed out after {after:?}")]
    TimedOut {
        /// Program that timed out (lossy).
        program: String,
        /// The timeout that elapsed.
        after: Duration,
    },

    /// Output from git could not be parsed.
    #[error(transparent)]
    Parse(#[from] ParseError),

    /// The caller passed an argument this crate refuses to forward
    /// (for example a revision that would be read as an option).
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    /// The working directory does not exist or is not a directory.
    #[error("working directory does not exist: {0}")]
    MissingWorkingDirectory(PathBuf),
}

/// Output that did not match the expected format.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cannot parse {what}: {message}")]
pub struct ParseError {
    /// Which format was being parsed (for example `"status"`).
    pub what: &'static str,
    /// What was wrong.
    pub message: String,
}

impl ParseError {
    /// Creates a parse error for the given format.
    pub fn new(what: &'static str, message: impl Into<String>) -> Self {
        Self {
            what,
            message: message.into(),
        }
    }
}

fn describe_code(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("exit code {code}"),
        None => "terminated by signal".to_owned(),
    }
}

/// Returns the last [`STDERR_TAIL_LIMIT`] bytes of `stderr` as lossy UTF-8,
/// trimmed, with a marker when bytes were dropped.
pub(crate) fn stderr_tail(stderr: &[u8]) -> String {
    let start = stderr.len().saturating_sub(STDERR_TAIL_LIMIT);
    let text = String::from_utf8_lossy(&stderr[start..]);
    let text = text.trim();
    if start > 0 {
        format!("[...] {text}")
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_zero_exit_display() {
        let err = GitError::NonZeroExit {
            program: "git".into(),
            args: "status".into(),
            code: Some(128),
            stderr: "fatal: not a git repository".into(),
        };
        assert_eq!(
            err.to_string(),
            "git status failed (exit code 128): fatal: not a git repository"
        );
    }

    #[test]
    fn stderr_tail_truncates_front() {
        let mut long = vec![b'a'; STDERR_TAIL_LIMIT + 10];
        long.extend_from_slice(b"fatal: end");
        let tail = stderr_tail(&long);
        assert!(tail.starts_with("[...] "));
        assert!(tail.ends_with("fatal: end"));
        assert!(tail.len() <= STDERR_TAIL_LIMIT + 6);
        assert_eq!(stderr_tail(b"  short\n"), "short");
    }
}
