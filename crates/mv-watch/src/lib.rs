//! File watcher.
//!
//! Watches project folders, debounces saves (including write-then-rename
//! saves) and detects files held open by a DAW.
//!
//! Status: scaffold only. The real implementation arrives with its task in
//! `docs/PLAN.md`.

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WatchError {
    /// The requested operation has not been implemented yet.
    #[error("not implemented yet: {0}")]
    Unimplemented(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_displays_operation() {
        let err = WatchError::Unimplemented("smoke");
        assert_eq!(err.to_string(), "not implemented yet: smoke");
    }
}
