//! Locating git and git-lfs.

// Integration tests: unwrap is allowed here as in unit tests (clippy.toml).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use mv_git::GitLocation;
#[cfg(unix)]
use mv_git::{Git, GitError};

#[tokio::test]
async fn finds_system_git_and_reports_version() {
    let env = common::TestEnv::new();
    let version = env.git.git_version().await.unwrap();
    assert!(version.major >= 2, "unexpected version {version:?}");
    assert!(version.raw.starts_with("git version "));
    assert!(env.git.location().bundled_dir().is_none());

    if env.git.location().has_git_lfs() {
        let versions = env.git.version().await.unwrap();
        assert!(versions.lfs.raw.starts_with("git-lfs/"));
        assert!(versions.lfs.major >= 2);
    } else {
        eprintln!("skipping lfs version check: git-lfs not found on PATH");
    }
}

#[test]
fn empty_bundled_dir_falls_back_to_path() {
    let dir = tempfile::tempdir().unwrap();
    let location = GitLocation::discover(Some(dir.path())).unwrap();
    assert!(!location.git().starts_with(dir.path()));
    assert!(location.bundled_dir().is_none());
}

#[cfg(unix)]
fn write_script(path: &std::path::Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn bundled_dir_is_preferred_and_prepended_to_path() {
    let bundle = tempfile::tempdir().unwrap();
    write_script(
        &bundle.path().join("git"),
        r#"if [ "$1" = "--version" ]; then echo "git version 9.8.7 (fake)"; else printf '%s' "$PATH"; fi"#,
    );
    let git = Git::discover(Some(bundle.path())).unwrap();
    assert_eq!(git.location().git(), bundle.path().join("git"));
    assert_eq!(git.location().bundled_dir(), Some(bundle.path()));

    let version = git.git_version().await.unwrap();
    assert_eq!((version.major, version.minor, version.patch), (9, 8, 7));

    let out = git
        .command(bundle.path(), ["print-path"])
        .output()
        .await
        .unwrap();
    let path = std::ffi::OsString::from(String::from_utf8(out.stdout).unwrap());
    let first = std::env::split_paths(&path).next().unwrap();
    assert_eq!(first, bundle.path());
}

#[cfg(unix)]
#[tokio::test]
async fn missing_git_lfs_is_a_distinct_error() {
    let bundle = tempfile::tempdir().unwrap();
    write_script(&bundle.path().join("git"), r#"echo "git version 2.99.0""#);
    // Search only the bundle directory, so no git-lfs can be found.
    let location =
        GitLocation::discover_with_path(None, Some(bundle.path().as_os_str().to_owned())).unwrap();
    assert_eq!(location.git(), bundle.path().join("git"));
    assert!(matches!(location.git_lfs(), Err(GitError::GitLfsNotFound)));

    let git = Git::new(location);
    assert!(matches!(git.version().await, Err(GitError::GitLfsNotFound)));
    assert!(matches!(
        git.lfs_version().await,
        Err(GitError::GitLfsNotFound)
    ));

    let none = GitLocation::discover_with_path(None, Some(std::ffi::OsString::new()));
    assert!(matches!(none, Err(GitError::GitNotFound)));
}
