//! DAW project parsers.
//!
//! Parses DAW project files and produces semantic diffs (FL Studio first),
//! plus the plugin manifest. Project files are never content-merged.
//!
//! Status: scaffold only. The real implementation arrives with its task in
//! `docs/PLAN.md`.

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DawError {
    /// The requested operation has not been implemented yet.
    #[error("not implemented yet: {0}")]
    Unimplemented(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_displays_operation() {
        let err = DawError::Unimplemented("smoke");
        assert_eq!(err.to_string(), "not implemented yet: smoke");
    }
}
