//! Running child processes: scrubbed environment, captured or streamed
//! output, cancellation, timeouts, and process-group cleanup.
//!
//! Programs are started directly with an argument vector. Nothing here goes
//! through a shell.
//!
//! # Termination
//!
//! On Unix every child is placed in its own process group
//! (`process_group(0)`), so cancellation, timeout and drop can signal git
//! together with the helpers it spawns (`git-lfs`, `git-remote-https`, ...).
//! Cancel and timeout send `SIGTERM` to the group (git removes its lock files
//! on `SIGTERM`), wait up to [`Invocation::kill_grace`] for the child, then
//! send `SIGKILL` to the group and reap the child. Dropping a running future
//! sends `SIGKILL` to the group and leaves reaping to tokio (`kill_on_drop`).
//!
//! On Windows the direct child is killed (`kill_on_drop` plus an explicit
//! kill). Grandchildren are **not** killed yet; a Job Object is needed for
//! that and is a known gap.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::error::{GitError, STDERR_TAIL_LIMIT, stderr_tail};
use crate::lfs::{LfsProgress, LfsProgressTail};

/// Inherited variables that would redirect git to another repository.
/// They are always removed from the child environment.
pub const SCRUBBED_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_COMMON_DIR",
];

/// Default time between `SIGTERM` and `SIGKILL` on cancel or timeout.
pub const DEFAULT_KILL_GRACE: Duration = Duration::from_millis(1500);

/// Streamed lines longer than this are delivered in pieces.
pub const MAX_LINE_LEN: usize = 64 * 1024;

const READ_CHUNK: usize = 8 * 1024;
const LFS_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Which output stream a line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// One line of streamed output, without its terminator.
///
/// Stdout is split on `\n`. Stderr is split on `\n` and `\r` (git progress
/// meters rewrite a line with `\r`); `\r\n` counts as one terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputLine {
    /// Source stream.
    pub stream: StreamKind,
    /// Raw bytes of the line.
    pub line: Vec<u8>,
}

/// Result of a captured run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Exit code, or `None` if the child was terminated by a signal.
    pub exit_code: Option<i32>,
    /// Everything written to stdout.
    pub stdout: Vec<u8>,
    /// Everything written to stderr.
    pub stderr: Vec<u8>,
}

/// Result of a streamed run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamOutput {
    /// Exit code, or `None` if the child was terminated by a signal.
    pub exit_code: Option<i32>,
    /// The last bytes of stderr (bounded), kept for diagnostics.
    pub stderr_tail: Vec<u8>,
}

/// A single program run: program, working directory, arguments, environment
/// and run controls. Build one with [`Invocation::new`] or
/// [`crate::Git::command`], then call [`Invocation::output`] or
/// [`Invocation::stream`].
#[derive(Debug)]
pub struct Invocation {
    program: PathBuf,
    args: Vec<OsString>,
    cwd: PathBuf,
    env: Vec<(OsString, OsString)>,
    path_prepend: Option<PathBuf>,
    stdin: Option<Vec<u8>>,
    timeout: Option<Duration>,
    cancel: CancellationToken,
    accepted_codes: Vec<i32>,
    kill_grace: Duration,
    lfs_progress: Option<(PathBuf, mpsc::Sender<LfsProgress>)>,
}

impl Invocation {
    /// Prepares a run of `program` in `cwd` with no arguments.
    ///
    /// The environment is inherited minus [`SCRUBBED_ENV`], plus
    /// `GIT_TERMINAL_PROMPT=0` and `LC_ALL=C`. Stdin is closed unless
    /// [`Invocation::stdin`] supplies input. Only exit code 0 is accepted.
    pub fn new(program: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            env: Vec::new(),
            path_prepend: None,
            stdin: None,
            timeout: None,
            cancel: CancellationToken::new(),
            accepted_codes: vec![0],
            kill_grace: DEFAULT_KILL_GRACE,
            lfs_progress: None,
        }
    }

    /// Appends one argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Appends arguments.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Sets an environment variable for the child. Applied after the
    /// defaults, so it can override them.
    #[must_use]
    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Sets several environment variables (see [`Invocation::env`]).
    #[must_use]
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        self.env
            .extend(vars.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    /// Writes `input` to the child's stdin, then closes it.
    #[must_use]
    pub fn stdin(mut self, input: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(input.into());
        self
    }

    /// Terminates the child and returns [`GitError::TimedOut`] after `timeout`.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Terminates the child and returns [`GitError::Cancelled`] when `token`
    /// is cancelled.
    #[must_use]
    pub fn cancel_token(mut self, token: CancellationToken) -> Self {
        self.cancel = token;
        self
    }

    /// Exit codes treated as success (default `[0]`).
    #[must_use]
    pub fn accept_exit_codes(mut self, codes: &[i32]) -> Self {
        self.accepted_codes = codes.to_vec();
        self
    }

    /// How long to wait after `SIGTERM` before `SIGKILL` on cancel/timeout.
    #[must_use]
    pub fn kill_grace(mut self, grace: Duration) -> Self {
        self.kill_grace = grace;
        self
    }

    /// Sets `GIT_LFS_PROGRESS` to `path` (truncated before the run) and
    /// sends each parsed progress line to `events` while the run is going.
    /// The run returns after the last line has been sent, so receive
    /// concurrently when the channel may fill up.
    #[must_use]
    pub fn lfs_progress(
        mut self,
        path: impl Into<PathBuf>,
        events: mpsc::Sender<LfsProgress>,
    ) -> Self {
        self.lfs_progress = Some((path.into(), events));
        self
    }

    /// Prepends `dir` to the child's `PATH`.
    #[must_use]
    pub(crate) fn prepend_path(mut self, dir: impl Into<PathBuf>) -> Self {
        self.path_prepend = Some(dir.into());
        self
    }

    /// The program that will run.
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// The arguments that will be passed.
    pub fn arguments(&self) -> &[OsString] {
        &self.args
    }

    /// Runs to completion, capturing stdout and stderr.
    ///
    /// Returns [`GitError::NonZeroExit`] when the exit code is not accepted.
    pub async fn output(self) -> Result<Output, GitError> {
        let (finished, ctx) = self.execute(None).await?;
        ctx.check(finished.exit_code, &finished.stderr)?;
        Ok(Output {
            exit_code: finished.exit_code,
            stdout: finished.stdout,
            stderr: finished.stderr,
        })
    }

    /// Runs to completion, sending stdout and stderr lines to `lines` as they
    /// arrive. Output is not otherwise kept, except a bounded stderr tail.
    ///
    /// The channel applies backpressure: receive concurrently, or a full
    /// channel stalls the child once its pipe buffer fills. If the receiver
    /// is dropped, output is drained and discarded.
    pub async fn stream(self, lines: mpsc::Sender<OutputLine>) -> Result<StreamOutput, GitError> {
        let (finished, ctx) = self.execute(Some(lines)).await?;
        ctx.check(finished.exit_code, &finished.stderr)?;
        Ok(StreamOutput {
            exit_code: finished.exit_code,
            stderr_tail: finished.stderr,
        })
    }

    async fn execute(
        self,
        sink: Option<mpsc::Sender<OutputLine>>,
    ) -> Result<(Finished, RunContext), GitError> {
        let ctx = RunContext {
            program: self
                .program
                .file_name()
                .unwrap_or(self.program.as_os_str())
                .to_string_lossy()
                .into_owned(),
            args: self
                .args
                .iter()
                .map(|a| a.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" "),
            accepted_codes: self.accepted_codes.clone(),
        };

        if !self.cwd.is_dir() {
            return Err(GitError::MissingWorkingDirectory(self.cwd));
        }
        if self.cancel.is_cancelled() {
            return Err(GitError::Cancelled {
                program: ctx.program,
            });
        }

        let mut progress_tail = None;
        if let Some((path, events)) = &self.lfs_progress {
            tokio::fs::File::create(path)
                .await
                .map_err(|source| ctx.io(source))?;
            progress_tail = Some((path.clone(), events.clone()));
        }

        let mut cmd = self.build_command()?;

        tracing::debug!(
            program = %ctx.program,
            args = %ctx.args,
            cwd = %self.cwd.display(),
            "spawning"
        );

        let mut child = cmd.spawn().map_err(|source| GitError::Spawn {
            program: ctx.program.clone(),
            source,
        })?;
        let mut guard = KillGuard::new(&child);
        let progress_tail = progress_tail
            .map(|(path, events)| LfsProgressTail::spawn(path, events, LFS_POLL_INTERVAL));

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let input = self.stdin;
        let streaming = sink.is_some();

        let stop = {
            let io = async {
                tokio::join!(
                    write_stdin(stdin, input),
                    pump(stdout, StreamKind::Stdout, sink.as_ref()),
                    pump(stderr, StreamKind::Stderr, sink.as_ref()),
                )
            };
            let run = async {
                let (stdin_res, stdout_res, stderr_res) = io.await;
                let status = child.wait().await;
                (stdin_res, stdout_res, stderr_res, status)
            };
            let deadline = async {
                match self.timeout {
                    Some(after) => tokio::time::sleep(after).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                biased;
                () = self.cancel.cancelled() => Stop::Cancelled,
                () = deadline => Stop::TimedOut,
                result = run => Stop::Finished(Box::new(result)),
            }
        };

        match stop {
            Stop::Cancelled => {
                tracing::debug!(program = %ctx.program, "cancelled, terminating");
                terminate(&mut child, &mut guard, self.kill_grace).await;
                drop(progress_tail);
                Err(GitError::Cancelled {
                    program: ctx.program,
                })
            }
            Stop::TimedOut => {
                tracing::debug!(program = %ctx.program, "timed out, terminating");
                terminate(&mut child, &mut guard, self.kill_grace).await;
                drop(progress_tail);
                Err(GitError::TimedOut {
                    program: ctx.program,
                    after: self.timeout.unwrap_or_default(),
                })
            }
            Stop::Finished(result) => {
                let (stdin_res, stdout_res, stderr_res, status) = *result;
                let status = status.map_err(|source| ctx.io(source))?;
                // The child has been reaped; its group must not be signalled.
                guard.disarm();
                if let Some(tail) = progress_tail {
                    tail.finish().await;
                }
                stdin_res.map_err(|source| ctx.io(source))?;
                let stdout = stdout_res.map_err(|source| ctx.io(source))?;
                let stderr = stderr_res.map_err(|source| ctx.io(source))?;
                tracing::debug!(program = %ctx.program, code = ?status.code(), streaming, "exited");
                Ok((
                    Finished {
                        exit_code: status.code(),
                        stdout,
                        stderr,
                    },
                    ctx,
                ))
            }
        }
    }

    /// Builds the child command: arguments, working directory, pipes,
    /// process group and environment. Nothing is spawned.
    fn build_command(&self) -> Result<tokio::process::Command, GitError> {
        let mut cmd = tokio::process::Command::new(&self.program);
        cmd.args(&self.args)
            .current_dir(&self.cwd)
            .stdin(if self.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        cmd.process_group(0);

        for key in SCRUBBED_ENV {
            cmd.env_remove(key);
        }
        cmd.env("GIT_TERMINAL_PROMPT", "0").env("LC_ALL", "C");
        if let Some(dir) = &self.path_prepend {
            let inherited = std::env::var_os("PATH").unwrap_or_default();
            let joined = std::env::join_paths(
                std::iter::once(dir.clone()).chain(std::env::split_paths(&inherited)),
            )
            .map_err(|_| {
                GitError::InvalidArgument("bundled directory contains a PATH separator".to_owned())
            })?;
            cmd.env("PATH", joined);
        }

        if let Some((path, _)) = &self.lfs_progress {
            cmd.env("GIT_LFS_PROGRESS", path);
        }

        // Caller overrides last. Values are never logged.
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        Ok(cmd)
    }
}

type RunResult = (
    std::io::Result<()>,
    std::io::Result<Vec<u8>>,
    std::io::Result<Vec<u8>>,
    std::io::Result<std::process::ExitStatus>,
);

enum Stop {
    Cancelled,
    TimedOut,
    Finished(Box<RunResult>),
}

struct Finished {
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

struct RunContext {
    program: String,
    args: String,
    accepted_codes: Vec<i32>,
}

impl RunContext {
    fn io(&self, source: std::io::Error) -> GitError {
        GitError::Io {
            program: self.program.clone(),
            source,
        }
    }

    fn check(&self, code: Option<i32>, stderr: &[u8]) -> Result<(), GitError> {
        match code {
            Some(code) if self.accepted_codes.contains(&code) => Ok(()),
            _ => Err(GitError::NonZeroExit {
                program: self.program.clone(),
                args: self.args.clone(),
                code,
                stderr: stderr_tail(stderr),
            }),
        }
    }
}

async fn write_stdin(stdin: Option<ChildStdin>, input: Option<Vec<u8>>) -> std::io::Result<()> {
    let (Some(mut stdin), Some(input)) = (stdin, input) else {
        return Ok(());
    };
    match stdin.write_all(&input).await {
        // The child may exit without reading its input; that is not our error.
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
    // `stdin` drops here, closing the pipe.
}

/// Reads one output stream to EOF. Without a sink the bytes are returned;
/// with a sink they are split into lines and sent, and only a bounded stderr
/// tail is returned.
async fn pump<R: AsyncRead + Unpin>(
    reader: Option<R>,
    kind: StreamKind,
    sink: Option<&mpsc::Sender<OutputLine>>,
) -> std::io::Result<Vec<u8>> {
    let Some(mut reader) = reader else {
        return Ok(Vec::new());
    };
    let Some(sink) = sink else {
        let mut all = Vec::new();
        reader.read_to_end(&mut all).await?;
        return Ok(all);
    };

    let mut splitter = LineSplitter::new(kind == StreamKind::Stderr);
    let mut tail = Vec::new();
    let mut chunk = vec![0u8; READ_CHUNK];
    let mut lines = Vec::new();
    let mut receiver_open = true;
    loop {
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        let bytes = &chunk[..n];
        if kind == StreamKind::Stderr {
            tail.extend_from_slice(bytes);
            if tail.len() > 2 * STDERR_TAIL_LIMIT {
                let excess = tail.len() - STDERR_TAIL_LIMIT;
                tail.drain(..excess);
            }
        }
        splitter.push(bytes, &mut lines);
        for line in lines.drain(..) {
            if receiver_open {
                receiver_open = sink.send(OutputLine { stream: kind, line }).await.is_ok();
            }
        }
    }
    if let Some(line) = splitter.finish()
        && receiver_open
    {
        let _ = sink.send(OutputLine { stream: kind, line }).await;
    }
    Ok(tail)
}

/// Splits a byte stream into lines across chunk boundaries.
struct LineSplitter {
    buf: Vec<u8>,
    split_cr: bool,
    after_cr: bool,
}

impl LineSplitter {
    fn new(split_cr: bool) -> Self {
        Self {
            buf: Vec::new(),
            split_cr,
            after_cr: false,
        }
    }

    fn push(&mut self, bytes: &[u8], out: &mut Vec<Vec<u8>>) {
        for &byte in bytes {
            match byte {
                b'\n' if self.after_cr => self.after_cr = false,
                b'\n' => out.push(std::mem::take(&mut self.buf)),
                b'\r' if self.split_cr => {
                    out.push(std::mem::take(&mut self.buf));
                    self.after_cr = true;
                }
                _ => {
                    self.after_cr = false;
                    self.buf.push(byte);
                    if self.buf.len() >= MAX_LINE_LEN {
                        out.push(std::mem::take(&mut self.buf));
                    }
                }
            }
        }
    }

    fn finish(&mut self) -> Option<Vec<u8>> {
        (!self.buf.is_empty()).then(|| std::mem::take(&mut self.buf))
    }
}

/// Kills the child's process group if the run future is dropped while the
/// child has not been reaped. `kill_on_drop` covers the direct child (and
/// tokio reaps it in the background, so no zombie is left).
struct KillGuard {
    #[cfg(unix)]
    pgid: Option<nix::unistd::Pid>,
    armed: bool,
}

impl KillGuard {
    fn new(child: &Child) -> Self {
        #[cfg(not(unix))]
        let _ = child;
        Self {
            #[cfg(unix)]
            pgid: child
                .id()
                .and_then(|id| i32::try_from(id).ok())
                .map(nix::unistd::Pid::from_raw),
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }

    #[cfg(unix)]
    fn signal(&self, signal: nix::sys::signal::Signal) {
        if let Some(pgid) = self.pgid {
            // ESRCH (group already gone) is expected and ignored.
            let _ = nix::sys::signal::killpg(pgid, signal);
        }
    }
}

impl Drop for KillGuard {
    fn drop(&mut self) {
        if self.armed {
            #[cfg(unix)]
            self.signal(nix::sys::signal::Signal::SIGKILL);
        }
    }
}

/// Terminates the child (and on Unix its process group) and reaps it.
async fn terminate(child: &mut Child, guard: &mut KillGuard, grace: Duration) {
    #[cfg(unix)]
    {
        use nix::sys::signal::Signal;
        guard.signal(Signal::SIGTERM);
        let _ = tokio::time::timeout(grace, child.wait()).await;
        // Stragglers that ignored SIGTERM. If the leader was just reaped and
        // its group is already empty, the pgid could in theory be reused in
        // the microseconds since; this is accepted.
        guard.signal(Signal::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = grace;
        let _ = child.start_kill();
    }
    let _ = child.wait().await;
    guard.disarm();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(split_cr: bool, chunks: &[&[u8]]) -> Vec<Vec<u8>> {
        let mut s = LineSplitter::new(split_cr);
        let mut out = Vec::new();
        for chunk in chunks {
            s.push(chunk, &mut out);
        }
        out.extend(s.finish());
        out
    }

    #[test]
    fn splits_lines_across_chunks() {
        let lines = split(false, &[b"one\ntw", b"o\n", b"three"]);
        assert_eq!(
            lines,
            vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]
        );
    }

    #[test]
    fn stdout_keeps_carriage_returns() {
        let lines = split(false, &[b"a\rb\n"]);
        assert_eq!(lines, vec![b"a\rb".to_vec()]);
    }

    #[test]
    fn stderr_splits_on_carriage_return() {
        let lines = split(true, &[b"10%\r20%\r", b"\n", b"done\r\nend\n\n"]);
        assert_eq!(
            lines,
            vec![
                b"10%".to_vec(),
                b"20%".to_vec(),
                b"done".to_vec(),
                b"end".to_vec(),
                b"".to_vec(),
            ]
        );
    }

    fn env_of(inv: &Invocation) -> Vec<(OsString, Option<OsString>)> {
        let cmd = inv.build_command().unwrap();
        cmd.as_std()
            .get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(ToOwned::to_owned)))
            .collect()
    }

    fn lookup(env: &[(OsString, Option<OsString>)], key: &str) -> Option<Option<OsString>> {
        env.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn environment_is_scrubbed_and_fixed() {
        let (tx, _rx) = mpsc::channel(1);
        let dir = std::env::temp_dir();
        let inv = Invocation::new("git", &dir)
            .prepend_path("/opt/bundle")
            .lfs_progress("/tmp/progress", tx)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("LC_ALL", "C.UTF-8");
        let env = env_of(&inv);
        for key in SCRUBBED_ENV {
            assert_eq!(lookup(&env, key), Some(None), "{key} not removed");
        }
        assert_eq!(lookup(&env, "GIT_TERMINAL_PROMPT"), Some(Some("0".into())));
        // Caller overrides win over the defaults.
        assert_eq!(lookup(&env, "LC_ALL"), Some(Some("C.UTF-8".into())));
        assert_eq!(lookup(&env, "GIT_CONFIG_NOSYSTEM"), Some(Some("1".into())));
        assert_eq!(
            lookup(&env, "GIT_LFS_PROGRESS"),
            Some(Some("/tmp/progress".into()))
        );
        let path = lookup(&env, "PATH").flatten().unwrap();
        let first = std::env::split_paths(&path).next().unwrap();
        assert_eq!(first, PathBuf::from("/opt/bundle"));

        let plain = env_of(&Invocation::new("git", &dir));
        assert_eq!(lookup(&plain, "LC_ALL"), Some(Some("C".into())));
        assert_eq!(lookup(&plain, "PATH"), None);
        assert_eq!(lookup(&plain, "GIT_LFS_PROGRESS"), None);
    }

    #[test]
    fn long_lines_are_chunked() {
        let long = vec![b'x'; MAX_LINE_LEN + 5];
        let lines = split(false, &[&long]);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), MAX_LINE_LEN);
        assert_eq!(lines[1].len(), 5);
    }
}
