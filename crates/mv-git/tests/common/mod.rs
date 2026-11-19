//! Shared helpers for integration tests: an isolated git environment in a
//! temp dir (no global/system config, private HOME).

#![allow(dead_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mv_git::{Git, Output};

pub struct TestEnv {
    pub dir: tempfile::TempDir,
    pub git: Git,
}

impl TestEnv {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".config")).unwrap();
        let config = dir.path().join("gitconfig");
        std::fs::write(
            &config,
            "[user]\n\tname = Test User\n\temail = test@example.com\n\
             [init]\n\tdefaultBranch = main\n\
             [commit]\n\tgpgsign = false\n\
             [core]\n\tautocrlf = false\n",
        )
        .unwrap();
        let git = Git::discover(None)
            .unwrap()
            .with_env("GIT_CONFIG_GLOBAL", &config)
            .with_env("GIT_CONFIG_NOSYSTEM", "1")
            .with_env("HOME", &home)
            .with_env("XDG_CONFIG_HOME", home.join(".config"))
            .with_env("GIT_CEILING_DIRECTORIES", dir.path());
        Self { dir, git }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Runs git and panics with stderr on failure.
    pub async fn run<I, S>(&self, cwd: &Path, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        match self.git.command(cwd, args).output().await {
            Ok(out) => out,
            Err(err) => panic!("git failed: {err}"),
        }
    }

    /// Creates `name` and runs `git init` in it.
    pub async fn init_repo(&self, name: &str) -> PathBuf {
        let repo = self.path().join(name);
        std::fs::create_dir_all(&repo).unwrap();
        self.run(&repo, ["init", "-q", "-b", "main"]).await;
        repo
    }

    pub async fn commit_all(&self, repo: &Path, message: &str) {
        self.run(repo, ["add", "-A"]).await;
        self.run(repo, ["commit", "-q", "-m", message]).await;
    }
}

/// Polls `cond` until it is true or `timeout` passes.
pub async fn wait_for(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    cond()
}

/// Whether a process with `pid` exists (Unix).
#[cfg(unix)]
pub fn process_alive(pid: i32) -> bool {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
}

/// Reads a pid written by a test script.
pub fn read_pid(path: &Path) -> Option<i32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}
