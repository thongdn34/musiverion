//! `Git::status` against real temp repositories.

// Integration tests: unwrap is allowed here as in unit tests (clippy.toml).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};

use mv_git::status::{ChangeKind, RenameKind};
use mv_git::{StatusEntry, StatusOptions, StatusReport, UntrackedFiles};

fn find<'a>(report: &'a StatusReport, path: &str) -> &'a StatusEntry {
    report
        .entries
        .iter()
        .find(|e| e.path() == Path::new(path))
        .unwrap_or_else(|| panic!("no entry for {path:?} in {:#?}", report.entries))
}

fn write(repo: &Path, name: &str, content: &[u8]) {
    let path = repo.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

#[tokio::test]
async fn status_reports_every_kind_of_change() {
    let env = common::TestEnv::new();
    let repo = env.init_repo("studio").await;

    let rename_content = b"FLhd project data that is long enough for rename detection\n".repeat(20);
    write(&repo, ".gitignore", b"*.tmp\n");
    write(&repo, "modified.wav", b"take 1");
    write(&repo, "staged mix.wav", b"mix 1");
    write(&repo, "to rename.flp", &rename_content);
    write(&repo, "deleted.wav", b"gone soon");
    write(&repo, "-tracked dash.wav", b"dash 1");
    env.commit_all(&repo, "initial").await;

    write(&repo, "modified.wav", b"take 2");
    write(&repo, "-tracked dash.wav", b"dash 2");
    write(&repo, "staged mix.wav", b"mix 2");
    env.run(&repo, ["add", "--", "staged mix.wav"]).await;
    env.run(&repo, ["mv", "--", "to rename.flp", "renamed song.flp"])
        .await;
    std::fs::remove_file(repo.join("deleted.wav")).unwrap();
    write(&repo, "Bản thu âm.wav", b"vocal");
    env.run(&repo, ["add", "--", "Bản thu âm.wav"]).await;
    write(&repo, "untracked file.wav", b"new");
    write(&repo, "-leading dash.wav", b"dash");
    write(&repo, "stems/Bản thu âm 2.wav", b"stem");
    write(&repo, "render.tmp", b"ignored");
    #[cfg(unix)]
    write(&repo, "line\nbreak.wav", b"newline in name");

    let report = env
        .git
        .status_with(
            &repo,
            StatusOptions {
                untracked: UntrackedFiles::All,
                ignored: true,
            },
        )
        .await
        .unwrap();

    assert_eq!(report.branch.head.as_deref(), Some("main"));
    assert!(report.branch.oid.is_some());
    assert_eq!(report.branch.upstream, None);
    assert_eq!(report.branch.ahead, None);

    let StatusEntry::Changed(modified) = find(&report, "modified.wav") else {
        panic!("modified.wav should be a changed entry")
    };
    assert_eq!(modified.code.index, ChangeKind::Unmodified);
    assert_eq!(modified.code.worktree, ChangeKind::Modified);

    let StatusEntry::Changed(dash) = find(&report, "-tracked dash.wav") else {
        panic!("dash file should be a changed entry")
    };
    assert_eq!(dash.code.worktree, ChangeKind::Modified);

    let StatusEntry::Changed(staged) = find(&report, "staged mix.wav") else {
        panic!("staged file should be a changed entry")
    };
    assert_eq!(staged.code.index, ChangeKind::Modified);
    assert_eq!(staged.code.worktree, ChangeKind::Unmodified);
    assert_ne!(staged.oid_head, staged.oid_index);

    let StatusEntry::Changed(added) = find(&report, "Bản thu âm.wav") else {
        panic!("added non-ASCII file should be a changed entry")
    };
    assert_eq!(added.code.index, ChangeKind::Added);

    let StatusEntry::Renamed(renamed) = find(&report, "renamed song.flp") else {
        panic!("rename should be a renamed entry")
    };
    assert_eq!(renamed.kind, RenameKind::Renamed);
    assert_eq!(renamed.score, 100);
    assert_eq!(renamed.orig_path, PathBuf::from("to rename.flp"));

    let StatusEntry::Changed(deleted) = find(&report, "deleted.wav") else {
        panic!("deleted file should be a changed entry")
    };
    assert_eq!(deleted.code.worktree, ChangeKind::Deleted);

    for untracked in [
        "untracked file.wav",
        "-leading dash.wav",
        "stems/Bản thu âm 2.wav",
    ] {
        assert!(
            matches!(find(&report, untracked), StatusEntry::Untracked(_)),
            "{untracked} should be untracked"
        );
    }
    #[cfg(unix)]
    assert!(matches!(
        find(&report, "line\nbreak.wav"),
        StatusEntry::Untracked(_)
    ));
    assert!(matches!(
        find(&report, "render.tmp"),
        StatusEntry::Ignored(_)
    ));

    // Default options: untracked listed, ignored not.
    let default = env.git.status(&repo).await.unwrap();
    assert!(
        default
            .entries
            .iter()
            .all(|e| !matches!(e, StatusEntry::Ignored(_)))
    );
    assert!(matches!(
        find(&default, "stems/Bản thu âm 2.wav"),
        StatusEntry::Untracked(_)
    ));
}

#[tokio::test]
async fn status_on_empty_repo_reports_initial() {
    let env = common::TestEnv::new();
    let repo = env.init_repo("empty").await;
    let report = env.git.status(&repo).await.unwrap();
    assert_eq!(report.branch.oid, None);
    assert_eq!(report.branch.head.as_deref(), Some("main"));
    assert!(report.entries.is_empty());
}

#[tokio::test]
async fn status_reports_ahead_and_behind_a_bare_remote() {
    let env = common::TestEnv::new();
    let remote = env.path().join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    env.run(&remote, ["init", "-q", "--bare", "-b", "main"])
        .await;

    let alice = env.init_repo("alice").await;
    write(&alice, "song.flp", b"v1");
    env.commit_all(&alice, "v1").await;
    env.run(
        &alice,
        ["remote", "add", "origin", remote.to_str().unwrap()],
    )
    .await;
    env.run(&alice, ["push", "-q", "-u", "origin", "main"])
        .await;

    let bob = env.path().join("bob");
    env.run(
        env.path(),
        [
            "clone",
            "-q",
            remote.to_str().unwrap(),
            bob.to_str().unwrap(),
        ],
    )
    .await;
    write(&bob, "song.flp", b"bob's take");
    env.commit_all(&bob, "bob").await;
    env.run(&bob, ["push", "-q"]).await;

    write(&alice, "song.flp", b"v2");
    env.commit_all(&alice, "v2").await;
    write(&alice, "song.flp", b"v3");
    env.commit_all(&alice, "v3").await;
    env.run(&alice, ["fetch", "-q"]).await;

    let report = env.git.status(&alice).await.unwrap();
    assert_eq!(report.branch.upstream.as_deref(), Some("origin/main"));
    assert_eq!(report.branch.ahead, Some(2));
    assert_eq!(report.branch.behind, Some(1));
}
