//! Comments and approvals.
//!
//! Comments, approvals and team notes stored in the repository's meta branch.
//!
//! Status: scaffold only. The real implementation arrives with its task in
//! `docs/PLAN.md`.

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MetaError {
    /// The requested operation has not been implemented yet.
    #[error("not implemented yet: {0}")]
    Unimplemented(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_displays_operation() {
        let err = MetaError::Unimplemented("smoke");
        assert_eq!(err.to_string(), "not implemented yet: smoke");
    }
}
