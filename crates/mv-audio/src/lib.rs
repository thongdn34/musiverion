//! Audio analysis.
//!
//! Decoding, metadata, PCM hashes, waveform peaks, loudness, null tests and
//! spectrograms. Results are cached by LFS OID in the OS cache directory,
//! never written into the repository.
//!
//! Status: scaffold only. The real implementation arrives with its task in
//! `docs/PLAN.md`.

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AudioError {
    /// The requested operation has not been implemented yet.
    #[error("not implemented yet: {0}")]
    Unimplemented(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_displays_operation() {
        let err = AudioError::Unimplemented("smoke");
        assert_eq!(err.to_string(), "not implemented yet: smoke");
    }
}
