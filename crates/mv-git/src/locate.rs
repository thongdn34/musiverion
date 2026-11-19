//! Locating the `git` and `git-lfs` executables.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::error::GitError;

/// Where the `git` and `git-lfs` executables live.
///
/// Built by [`GitLocation::discover`]: executables inside the bundled
/// directory win, otherwise `PATH` is searched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLocation {
    git: PathBuf,
    git_lfs: Option<PathBuf>,
    bundled_dir: Option<PathBuf>,
}

impl GitLocation {
    /// Finds `git` and `git-lfs`, preferring executables inside `bundled_dir`
    /// (with an `.exe` suffix on Windows) and falling back to `PATH`.
    ///
    /// Returns [`GitError::GitNotFound`] when no `git` exists. A missing
    /// `git-lfs` is not an error here; [`GitLocation::git_lfs`] reports it as
    /// [`GitError::GitLfsNotFound`] when LFS is actually needed.
    pub fn discover(bundled_dir: Option<&Path>) -> Result<Self, GitError> {
        Self::discover_with_path(bundled_dir, std::env::var_os("PATH"))
    }

    /// Like [`GitLocation::discover`], but searches the given `PATH` value
    /// instead of the process environment.
    pub fn discover_with_path(
        bundled_dir: Option<&Path>,
        path_var: Option<OsString>,
    ) -> Result<Self, GitError> {
        let in_bundle = |name: &str| {
            bundled_dir
                .map(|dir| dir.join(executable_name(name)))
                .filter(|candidate| is_executable(candidate))
        };
        let on_path = |name: &str| find_on_path(name, path_var.as_deref());

        let bundled_git = in_bundle("git");
        let bundled_lfs = in_bundle("git-lfs");
        let uses_bundle = bundled_git.is_some() || bundled_lfs.is_some();

        let git = bundled_git
            .or_else(|| on_path("git"))
            .ok_or(GitError::GitNotFound)?;
        let git_lfs = bundled_lfs.or_else(|| on_path("git-lfs"));

        let location = Self {
            git,
            git_lfs,
            bundled_dir: if uses_bundle {
                bundled_dir.map(Path::to_path_buf)
            } else {
                None
            },
        };
        tracing::debug!(
            git = %location.git.display(),
            git_lfs = ?location.git_lfs.as_ref().map(|p| p.display().to_string()),
            bundled = uses_bundle,
            "located git"
        );
        Ok(location)
    }

    /// Path of the `git` executable.
    pub fn git(&self) -> &Path {
        &self.git
    }

    /// Path of the `git-lfs` executable, or [`GitError::GitLfsNotFound`].
    pub fn git_lfs(&self) -> Result<&Path, GitError> {
        self.git_lfs.as_deref().ok_or(GitError::GitLfsNotFound)
    }

    /// Whether a `git-lfs` executable was found.
    pub fn has_git_lfs(&self) -> bool {
        self.git_lfs.is_some()
    }

    /// The bundled directory, if any executable was taken from it. Runs
    /// prepend it to `PATH` so `git` finds the bundled `git-lfs`.
    pub fn bundled_dir(&self) -> Option<&Path> {
        self.bundled_dir.as_deref()
    }
}

/// Platform file name of an executable.
fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn find_on_path(name: &str, path_var: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let file = executable_name(name);
    std::env::split_paths(path_var?)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(&file))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Parsed `git --version` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitVersion {
    /// Major version.
    pub major: u32,
    /// Minor version.
    pub minor: u32,
    /// Patch version (0 when absent).
    pub patch: u32,
    /// The full first line, for example `git version 2.45.1.windows.1`.
    pub raw: String,
}

/// Parsed `git lfs version` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsVersion {
    /// Major version.
    pub major: u32,
    /// Minor version.
    pub minor: u32,
    /// Patch version (0 when absent).
    pub patch: u32,
    /// The full first line, for example `git-lfs/3.7.1 (GitHub; darwin arm64; go 1.25.3)`.
    pub raw: String,
}

/// Versions of both executables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versions {
    /// `git --version`.
    pub git: GitVersion,
    /// `git lfs version`.
    pub lfs: LfsVersion,
}

/// Parses the output of `git --version`.
pub fn parse_git_version(output: &[u8]) -> Result<GitVersion, crate::ParseError> {
    let raw = first_line(output);
    let rest = raw
        .strip_prefix("git version ")
        .ok_or_else(|| crate::ParseError::new("git version", "missing `git version` prefix"))?;
    let (major, minor, patch) = parse_triple(rest)
        .ok_or_else(|| crate::ParseError::new("git version", "no numeric version"))?;
    Ok(GitVersion {
        major,
        minor,
        patch,
        raw,
    })
}

/// Parses the output of `git lfs version`.
pub fn parse_lfs_version(output: &[u8]) -> Result<LfsVersion, crate::ParseError> {
    let raw = first_line(output);
    let rest = raw
        .strip_prefix("git-lfs/")
        .ok_or_else(|| crate::ParseError::new("git-lfs version", "missing `git-lfs/` prefix"))?;
    let (major, minor, patch) = parse_triple(rest)
        .ok_or_else(|| crate::ParseError::new("git-lfs version", "no numeric version"))?;
    Ok(LfsVersion {
        major,
        minor,
        patch,
        raw,
    })
}

fn first_line(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    text.lines().next().unwrap_or("").trim().to_owned()
}

/// Parses `X.Y[.Z]` at the start of `text`, ignoring anything after.
fn parse_triple(text: &str) -> Option<(u32, u32, u32)> {
    let version = text.split(|c: char| c.is_whitespace()).next()?;
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts
        .next()
        .and_then(|p| {
            let digits: String = p.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
        .unwrap_or(0);
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_git_versions() {
        let v = parse_git_version(b"git version 2.50.1 (Apple Git-155)\n").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (2, 50, 1));
        assert_eq!(v.raw, "git version 2.50.1 (Apple Git-155)");
        let v = parse_git_version(b"git version 2.45.1.windows.1\r\n").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (2, 45, 1));
        let v = parse_git_version(b"git version 2.46\n").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (2, 46, 0));
    }

    #[test]
    fn parses_lfs_versions() {
        let v = parse_lfs_version(b"git-lfs/3.7.1 (GitHub; darwin arm64; go 1.25.3)\n").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (3, 7, 1));
    }

    #[test]
    fn rejects_garbage_versions() {
        assert!(parse_git_version(b"").is_err());
        assert!(parse_git_version(b"hello").is_err());
        assert!(parse_git_version(b"git version x.y").is_err());
        assert!(parse_lfs_version(b"git-lfs/").is_err());
        assert!(parse_lfs_version(b"\xff\xfe").is_err());
    }

    #[test]
    fn missing_git_is_typed() {
        let dir = tempfile::tempdir().unwrap();
        let err =
            GitLocation::discover_with_path(Some(dir.path()), Some(OsString::new())).unwrap_err();
        assert!(matches!(err, GitError::GitNotFound));
    }
}
