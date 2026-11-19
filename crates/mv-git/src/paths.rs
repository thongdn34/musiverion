//! Converting raw path bytes from git output into `PathBuf`.

use std::path::PathBuf;

use crate::error::ParseError;

/// Converts raw bytes to a path. On Unix any byte sequence is accepted; on
/// other platforms git writes UTF-8 and anything else is a parse error.
pub(crate) fn path_from_bytes(bytes: &[u8], what: &'static str) -> Result<PathBuf, ParseError> {
    #[cfg(unix)]
    {
        let _ = what;
        use std::os::unix::ffi::OsStrExt;
        Ok(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
    }
    #[cfg(not(unix))]
    {
        std::str::from_utf8(bytes)
            .map(PathBuf::from)
            .map_err(|_| ParseError::new(what, "path is not valid UTF-8"))
    }
}
