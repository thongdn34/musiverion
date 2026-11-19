//! Git LFS through the runner: pointer commits and real progress events.
//! Skipped (with a message) when git-lfs is not installed.

// Integration tests: unwrap is allowed here as in unit tests (clippy.toml).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use mv_git::{LfsDirection, LfsProgress};
use tokio::sync::mpsc;

fn lfs_available(env: &common::TestEnv) -> bool {
    if env.git.location().has_git_lfs() {
        true
    } else {
        eprintln!("skipping LFS test: git-lfs not found (bundled dir or PATH)");
        false
    }
}

fn pseudo_random(len: usize, seed: u32) -> Vec<u8> {
    let mut state = seed;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (state >> 16) as u8
        })
        .collect()
}

/// Collects progress events concurrently so the channel never fills.
fn collector() -> (
    mpsc::Sender<LfsProgress>,
    tokio::task::JoinHandle<Vec<LfsProgress>>,
) {
    let (tx, mut rx) = mpsc::channel(4);
    let handle = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    });
    (tx, handle)
}

async fn setup_lfs_repo(env: &common::TestEnv, files: &[(&str, &[u8])]) -> std::path::PathBuf {
    let repo = env.init_repo("lfs").await;
    env.run(&repo, ["lfs", "install", "--local"]).await;
    env.run(&repo, ["lfs", "track", "*.wav"]).await;
    for (name, content) in files {
        std::fs::write(repo.join(name), content).unwrap();
    }
    env.commit_all(&repo, "add audio").await;
    repo
}

#[tokio::test]
async fn tracked_audio_is_committed_as_a_pointer() {
    let env = common::TestEnv::new();
    if !lfs_available(&env) {
        return;
    }
    let audio = pseudo_random(100_000, 1);
    let repo = setup_lfs_repo(&env, &[("Bản thu âm.wav", &audio)]).await;

    let attributes = std::fs::read_to_string(repo.join(".gitattributes")).unwrap();
    assert!(attributes.contains("*.wav filter=lfs"), "{attributes}");

    let pointer = env
        .run(&repo, ["cat-file", "-p", "HEAD:Bản thu âm.wav"])
        .await;
    let pointer = String::from_utf8(pointer.stdout).unwrap();
    assert!(
        pointer.starts_with("version https://git-lfs.github.com/spec/v1\n"),
        "{pointer}"
    );
    assert!(pointer.contains("oid sha256:"));
    assert!(pointer.contains("size 100000"));

    let listed = env.run(&repo, ["lfs", "ls-files", "--name-only"]).await;
    assert_eq!(
        String::from_utf8(listed.stdout).unwrap().trim(),
        "Bản thu âm.wav"
    );

    // Working tree keeps the real audio and status is clean.
    assert_eq!(std::fs::read(repo.join("Bản thu âm.wav")).unwrap(), audio);
    assert!(env.git.status(&repo).await.unwrap().entries.is_empty());
}

#[tokio::test]
async fn clone_from_bare_remote_emits_download_progress() {
    let env = common::TestEnv::new();
    if !lfs_available(&env) {
        return;
    }
    let big = pseudo_random(300_000, 2);
    let small = pseudo_random(40_000, 3);
    let repo = setup_lfs_repo(&env, &[("kick drum.wav", &big), ("snare.wav", &small)]).await;

    let remote = env.path().join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    env.run(&remote, ["init", "-q", "--bare", "-b", "main"])
        .await;
    env.run(&repo, ["remote", "add", "origin", remote.to_str().unwrap()])
        .await;

    // git-lfs's local-path transfer adapter does not write upload progress,
    // so the push is only checked for success; events are collected anyway.
    let (tx, push_events) = collector();
    env.git
        .command(&repo, ["push", "-q", "-u", "origin", "main"])
        .lfs_progress(env.path().join("push-progress"), tx)
        .output()
        .await
        .unwrap();
    let push_events = push_events.await.unwrap();
    assert!(
        push_events
            .iter()
            .all(|e| e.direction == LfsDirection::Upload)
    );
    assert!(remote.join("lfs").join("objects").is_dir());

    // A smudging clone downloads both objects and reports progress.
    let clone = env.path().join("clone");
    let (tx, clone_events) = collector();
    env.git
        .command(
            env.path(),
            [
                "-c",
                "filter.lfs.process=git-lfs filter-process",
                "-c",
                "filter.lfs.smudge=git-lfs smudge -- %f",
                "-c",
                "filter.lfs.clean=git-lfs clean -- %f",
                "-c",
                "filter.lfs.required=true",
                "clone",
                "-q",
                remote.to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        )
        .lfs_progress(env.path().join("clone-progress"), tx)
        .output()
        .await
        .unwrap();
    let events = clone_events.await.unwrap();

    assert_eq!(std::fs::read(clone.join("kick drum.wav")).unwrap(), big);
    assert_eq!(std::fs::read(clone.join("snare.wav")).unwrap(), small);

    let finished = |name: &str, size: u64| {
        events.iter().any(|e| {
            e.direction == LfsDirection::Download
                && e.name == Path::new(name)
                && e.bytes_done == size
                && e.bytes_total == size
        })
    };
    assert!(
        finished("kick drum.wav", 300_000),
        "no completed download event for kick drum.wav in {events:#?}"
    );
    assert!(finished("snare.wav", 40_000), "{events:#?}");
}
