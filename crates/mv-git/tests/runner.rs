//! Runner behaviour: streaming, stdin, exit codes, cancel, timeout, drop.

// Integration tests: unwrap is allowed here as in unit tests (clippy.toml).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

#[cfg(unix)]
use mv_git::CancellationToken;
use mv_git::{GitError, Invocation, OutputLine, StreamKind};
use tokio::sync::mpsc;

#[tokio::test]
async fn streaming_delivers_lines_as_they_arrive() {
    let env = common::TestEnv::new();
    let alias = "alias.slow=!printf 'one\\n'; sleep 1; printf 'two\\n' >&2; printf 'three\\n'";
    let (tx, mut rx) = mpsc::channel::<OutputLine>(16);
    let run = tokio::spawn(
        env.git
            .command(env.path(), ["-c", alias, "slow"])
            .stream(tx),
    );

    let first = rx.recv().await.unwrap();
    let first_at = Instant::now();
    assert_eq!(first.line, b"one");
    assert_eq!(first.stream, StreamKind::Stdout);
    assert!(!run.is_finished(), "first line arrived only after exit");

    let mut rest = Vec::new();
    while let Some(line) = rx.recv().await {
        rest.push(line);
    }
    let summary = run.await.unwrap().unwrap();
    let elapsed = first_at.elapsed();
    assert!(
        elapsed >= Duration::from_millis(500),
        "first line was not delivered incrementally ({elapsed:?} before exit)"
    );
    assert_eq!(summary.exit_code, Some(0));
    assert!(rest.contains(&OutputLine {
        stream: StreamKind::Stderr,
        line: b"two".to_vec()
    }));
    assert!(rest.contains(&OutputLine {
        stream: StreamKind::Stdout,
        line: b"three".to_vec()
    }));
}

#[cfg(unix)]
#[tokio::test]
async fn generic_program_streams_both_kinds_and_reports_failure() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(16);
    let err = Invocation::new("/bin/sh", dir.path())
        .args(["-c", "echo out; echo 'fatal: boom' >&2; exit 3"])
        .stream(tx)
        .await
        .unwrap_err();
    let mut lines = Vec::new();
    while let Some(line) = rx.recv().await {
        lines.push(line);
    }
    assert!(lines.contains(&OutputLine {
        stream: StreamKind::Stdout,
        line: b"out".to_vec()
    }));
    match err {
        GitError::NonZeroExit { code, stderr, .. } => {
            assert_eq!(code, Some(3));
            assert_eq!(stderr, "fatal: boom");
        }
        other => panic!("unexpected {other}"),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn stdin_is_closed_unless_input_is_given() {
    let dir = tempfile::tempdir().unwrap();
    let out = Invocation::new("/bin/sh", dir.path())
        .args(["-c", "cat"])
        .timeout(Duration::from_secs(10))
        .output()
        .await
        .unwrap();
    assert!(out.stdout.is_empty());

    let out = Invocation::new("/bin/sh", dir.path())
        .args(["-c", "cat"])
        .stdin(b"hello\0world".to_vec())
        .output()
        .await
        .unwrap();
    assert_eq!(out.stdout, b"hello\0world");
}

#[cfg(unix)]
#[tokio::test]
async fn arguments_are_not_shell_interpreted() {
    let dir = tempfile::tempdir().unwrap();
    let tricky = "$(touch pwned); `touch pwned2` | ; * \"'\n";
    let out = Invocation::new("/bin/echo", dir.path())
        .arg(tricky)
        .output()
        .await
        .unwrap();
    assert_eq!(out.stdout, format!("{tricky}\n").as_bytes());
    assert!(!dir.path().join("pwned").exists());
    assert!(!dir.path().join("pwned2").exists());
}

#[tokio::test]
async fn non_zero_exit_maps_to_typed_error_with_stderr() {
    let env = common::TestEnv::new();
    let err = env.git.status(env.path()).await.unwrap_err();
    match err {
        GitError::NonZeroExit {
            program,
            args,
            code,
            stderr,
        } => {
            assert!(program.starts_with("git"));
            assert!(args.contains("status"));
            assert_eq!(code, Some(128));
            assert!(stderr.contains("not a git repository"), "{stderr}");
        }
        other => panic!("unexpected {other}"),
    }
}

#[tokio::test]
async fn accepted_exit_codes_are_not_errors() {
    let env = common::TestEnv::new();
    std::fs::write(env.path().join("a"), "1\n").unwrap();
    std::fs::write(env.path().join("b"), "2\n").unwrap();
    let out = env
        .git
        .command(env.path(), ["diff", "--no-index", "--exit-code", "a", "b"])
        .accept_exit_codes(&[0, 1])
        .output()
        .await
        .unwrap();
    assert_eq!(out.exit_code, Some(1));
    assert!(!out.stdout.is_empty());
}

#[tokio::test]
async fn spawn_and_working_directory_errors_are_typed() {
    let dir = tempfile::tempdir().unwrap();
    let err = Invocation::new(dir.path().join("no-such-program"), dir.path())
        .output()
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::Spawn { .. }), "{err}");

    let env = common::TestEnv::new();
    let err = env
        .git
        .command(dir.path().join("missing"), ["status"])
        .output()
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::MissingWorkingDirectory(_)), "{err}");
}

/// A git alias that runs a shell which starts a background `sleep` and
/// waits for it: git -> sh -> sleep. Both pids are written to files.
#[cfg(unix)]
fn hanging_alias(dir: &std::path::Path) -> (String, std::path::PathBuf, std::path::PathBuf) {
    let sh_pid = dir.join("sh.pid");
    let sleep_pid = dir.join("sleep.pid");
    let alias = format!(
        "alias.hang=!echo $$ > '{}'; sleep 300 & echo $! > '{}'; wait",
        sh_pid.display(),
        sleep_pid.display()
    );
    (alias, sh_pid, sleep_pid)
}

#[cfg(unix)]
async fn assert_all_gone(pids: &[i32]) {
    for &pid in pids {
        assert!(
            common::wait_for(Duration::from_secs(3), || !common::process_alive(pid)).await,
            "process {pid} is still running"
        );
    }
}

#[cfg(unix)]
async fn wait_for_pids(sh: &std::path::Path, sleep: &std::path::Path) -> (i32, i32) {
    assert!(
        common::wait_for(Duration::from_secs(10), || {
            common::read_pid(sh).is_some() && common::read_pid(sleep).is_some()
        })
        .await,
        "child did not start"
    );
    (
        common::read_pid(sh).unwrap(),
        common::read_pid(sleep).unwrap(),
    )
}

#[cfg(unix)]
#[tokio::test]
async fn cancel_kills_child_and_grandchildren() {
    let env = common::TestEnv::new();
    let (alias, sh, sleep) = hanging_alias(env.path());
    let token = CancellationToken::new();
    let run = tokio::spawn(
        env.git
            .command(env.path(), ["-c", alias.as_str(), "hang"])
            .cancel_token(token.clone())
            .output(),
    );
    let (sh_pid, sleep_pid) = wait_for_pids(&sh, &sleep).await;
    assert!(common::process_alive(sleep_pid));

    let start = Instant::now();
    token.cancel();
    let err = run.await.unwrap().unwrap_err();
    assert!(matches!(err, GitError::Cancelled { .. }), "{err}");
    assert!(start.elapsed() < Duration::from_secs(3), "cancel was slow");
    assert_all_gone(&[sh_pid, sleep_pid]).await;
}

#[cfg(unix)]
#[tokio::test]
async fn timeout_kills_child_and_grandchildren() {
    let env = common::TestEnv::new();
    let (alias, sh, sleep) = hanging_alias(env.path());
    let start = Instant::now();
    let err = env
        .git
        .command(env.path(), ["-c", alias.as_str(), "hang"])
        .timeout(Duration::from_millis(800))
        .output()
        .await
        .unwrap_err();
    assert!(
        matches!(err, GitError::TimedOut { after, .. } if after == Duration::from_millis(800)),
        "{err}"
    );
    assert!(start.elapsed() < Duration::from_secs(4), "timeout was slow");
    let (sh_pid, sleep_pid) = wait_for_pids(&sh, &sleep).await;
    assert_all_gone(&[sh_pid, sleep_pid]).await;
}

#[cfg(unix)]
#[tokio::test]
async fn dropping_the_future_kills_the_process_group() {
    let env = common::TestEnv::new();
    let (alias, sh, sleep) = hanging_alias(env.path());
    let run = tokio::spawn(
        env.git
            .command(env.path(), ["-c", alias.as_str(), "hang"])
            .output(),
    );
    let (sh_pid, sleep_pid) = wait_for_pids(&sh, &sleep).await;
    run.abort();
    assert!(run.await.unwrap_err().is_cancelled());
    assert_all_gone(&[sh_pid, sleep_pid]).await;
}

#[cfg(unix)]
#[tokio::test]
async fn sigterm_ignoring_children_are_killed_after_grace() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("sleep.pid");
    let script = format!(
        "trap '' TERM; sleep 300 & echo $! > '{}'; wait",
        pid_file.display()
    );
    let token = CancellationToken::new();
    let run = tokio::spawn(
        Invocation::new("/bin/sh", dir.path())
            .args(["-c", script.as_str()])
            .cancel_token(token.clone())
            .kill_grace(Duration::from_millis(300))
            .output(),
    );
    assert!(
        common::wait_for(Duration::from_secs(10), || common::read_pid(&pid_file)
            .is_some())
        .await
    );
    let pid = common::read_pid(&pid_file).unwrap();
    let start = Instant::now();
    token.cancel();
    let err = run.await.unwrap().unwrap_err();
    assert!(matches!(err, GitError::Cancelled { .. }), "{err}");
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_all_gone(&[pid]).await;
}
