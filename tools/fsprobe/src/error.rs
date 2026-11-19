//! Errors, worded for the musician who runs the tool.

use std::path::PathBuf;

/// Everything that can stop `fsprobe`. Messages are plain language because
/// they are printed as-is to someone who is not a programmer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProbeError {
    /// The protocol file could not be read.
    #[error("could not open the protocol file {}: {source}", path.display())]
    ProtocolRead {
        /// Protocol file.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// The protocol file is not valid.
    #[error("the protocol file {} has a mistake in it: {message}", path.display())]
    ProtocolInvalid {
        /// Protocol file.
        path: PathBuf,
        /// What is wrong.
        message: String,
    },
    /// A folder to watch is unusable.
    #[error("cannot watch the folder {}: {message}", path.display())]
    WatchFolder {
        /// Folder.
        path: PathBuf,
        /// What is wrong.
        message: String,
    },
    /// The filesystem watcher failed.
    #[error("the file watcher stopped working: {0}")]
    Watcher(String),
    /// Reading or writing a file failed.
    #[error("could not {action} {}: {source}", path.display())]
    Io {
        /// What we were doing, e.g. "write to".
        action: &'static str,
        /// File or folder.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// A results file exists but cannot be understood.
    #[error("the results file {} is damaged: {message}", path.display())]
    ResultsDamaged {
        /// File.
        path: PathBuf,
        /// What is wrong.
        message: String,
    },
    /// The way the tool was started does not fit the results folder.
    #[error("{0}")]
    Usage(String),
    /// The input (keyboard or script) ended.
    #[error("input ended")]
    InputEnded,
    /// The user typed "pause".
    #[error("paused")]
    Paused,
}

impl ProbeError {
    /// Shorthand for [`ProbeError::Io`].
    pub fn io(action: &'static str, path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            action,
            path: path.into(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_reads_like_a_sentence() {
        let err = ProbeError::io(
            "write to",
            "/x/trace.jsonl",
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        );
        assert_eq!(err.to_string(), "could not write to /x/trace.jsonl: denied");
    }
}
