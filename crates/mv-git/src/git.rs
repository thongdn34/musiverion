//! The [`Git`] handle: builds invocations of the located `git` and offers
//! thin typed helpers.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::error::GitError;
use crate::locate::{
    GitLocation, GitVersion, LfsVersion, Versions, parse_git_version, parse_lfs_version,
};
use crate::log::{Commit, LOG_FORMAT, parse_log};
use crate::runner::Invocation;
use crate::status::{StatusReport, parse_status};

/// A located git installation plus environment overrides applied to every
/// run (for example `GIT_CONFIG_GLOBAL` / `GIT_CONFIG_NOSYSTEM` in tests).
#[derive(Debug, Clone)]
pub struct Git {
    location: GitLocation,
    env: Vec<(OsString, OsString)>,
}

/// Which untracked files `status` lists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UntrackedFiles {
    /// Every untracked file (`--untracked-files=all`).
    #[default]
    All,
    /// Untracked directories collapsed (`--untracked-files=normal`).
    Normal,
    /// None (`--untracked-files=no`).
    No,
}

/// Options for [`Git::status_with`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusOptions {
    /// Untracked file listing.
    pub untracked: UntrackedFiles,
    /// Also list ignored files (`--ignored=matching`).
    pub ignored: bool,
}

/// What [`Git::log`] lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogQuery {
    /// A revision or range such as `main`, `origin/main..HEAD`. `None` means
    /// `HEAD`. Must not start with `-`.
    pub range: Option<OsString>,
    /// Maximum number of commits.
    pub limit: Option<usize>,
}

impl Git {
    /// Wraps a located installation.
    pub fn new(location: GitLocation) -> Self {
        Self {
            location,
            env: Vec::new(),
        }
    }

    /// Locates git (see [`GitLocation::discover`]) and wraps it.
    pub fn discover(bundled_dir: Option<&Path>) -> Result<Self, GitError> {
        GitLocation::discover(bundled_dir).map(Self::new)
    }

    /// Adds an environment override applied to every run of this handle.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// The located executables.
    pub fn location(&self) -> &GitLocation {
        &self.location
    }

    /// Prepares `git <args>` in `cwd`, with this handle's environment
    /// overrides and, when bundled executables are used, the bundled
    /// directory prepended to `PATH`.
    pub fn command<I, S>(&self, cwd: impl Into<PathBuf>, args: I) -> Invocation
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let mut invocation = Invocation::new(self.location.git(), cwd)
            .args(args)
            .envs(self.env.iter().cloned());
        if let Some(dir) = self.location.bundled_dir() {
            invocation = invocation.prepend_path(dir);
        }
        invocation
    }

    /// `git --version`.
    pub async fn git_version(&self) -> Result<GitVersion, GitError> {
        let out = self
            .command(std::env::temp_dir(), ["--version"])
            .output()
            .await?;
        Ok(parse_git_version(&out.stdout)?)
    }

    /// `git lfs version`; [`GitError::GitLfsNotFound`] if git-lfs was not
    /// located.
    pub async fn lfs_version(&self) -> Result<LfsVersion, GitError> {
        self.location.git_lfs()?;
        let out = self
            .command(std::env::temp_dir(), ["lfs", "version"])
            .output()
            .await?;
        Ok(parse_lfs_version(&out.stdout)?)
    }

    /// Versions of git and git-lfs. Fails with [`GitError::GitLfsNotFound`]
    /// when git-lfs is missing.
    pub async fn version(&self) -> Result<Versions, GitError> {
        let git = self.git_version().await?;
        let lfs = self.lfs_version().await?;
        Ok(Versions { git, lfs })
    }

    /// Status of the repository at `repo` with default options (all
    /// untracked files, no ignored files).
    pub async fn status(&self, repo: &Path) -> Result<StatusReport, GitError> {
        self.status_with(repo, StatusOptions::default()).await
    }

    /// Status of the repository at `repo`.
    ///
    /// Runs with `--no-optional-locks` so it never blocks or disturbs a
    /// concurrent mutating command.
    pub async fn status_with(
        &self,
        repo: &Path,
        options: StatusOptions,
    ) -> Result<StatusReport, GitError> {
        let untracked = match options.untracked {
            UntrackedFiles::All => "--untracked-files=all",
            UntrackedFiles::Normal => "--untracked-files=normal",
            UntrackedFiles::No => "--untracked-files=no",
        };
        let mut args = vec![
            "--no-optional-locks",
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            untracked,
        ];
        if options.ignored {
            args.push("--ignored=matching");
        }
        let out = self.command(repo, args).output().await?;
        Ok(parse_status(&out.stdout)?)
    }

    /// Commits of `query.range` (default `HEAD`), newest first.
    ///
    /// Fails with [`GitError::NonZeroExit`] on a repository with no commits.
    pub async fn log(&self, repo: &Path, query: &LogQuery) -> Result<Vec<Commit>, GitError> {
        let mut args: Vec<OsString> = [
            "-c",
            "log.showSignature=false",
            "log",
            "-z",
            "--no-color",
            "--encoding=UTF-8",
        ]
        .iter()
        .map(OsString::from)
        .collect();
        args.push(format!("--format={LOG_FORMAT}").into());
        if let Some(limit) = query.limit {
            args.push(format!("--max-count={limit}").into());
        }
        args.push("--end-of-options".into());
        if let Some(range) = &query.range {
            if range.is_empty() || starts_with_dash(range) {
                return Err(GitError::InvalidArgument(
                    "log range must be a non-empty revision not starting with '-'".to_owned(),
                ));
            }
            args.push(range.clone());
        }
        args.push("--".into());
        let out = self.command(repo, args).output().await?;
        Ok(parse_log(&out.stdout)?)
    }
}

fn starts_with_dash(arg: &OsStr) -> bool {
    arg.as_encoded_bytes().first() == Some(&b'-')
}
