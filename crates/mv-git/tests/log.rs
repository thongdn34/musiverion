//! `Git::log` against a real temp repository.

// Integration tests: unwrap is allowed here as in unit tests (clippy.toml).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use mv_git::{GitError, LogQuery};

#[tokio::test]
async fn log_handles_multiline_bodies_and_unicode_authors() {
    let env = common::TestEnv::new();
    let repo = env.init_repo("log").await;

    std::fs::write(repo.join("a.flp"), b"1").unwrap();
    env.commit_all(&repo, "First take").await;

    std::fs::write(repo.join("a.flp"), b"2").unwrap();
    env.run(&repo, ["add", "-A"]).await;
    let message = "Mix v2: \"louder\" | tabs\there\n\n\
                   Body line one\nBody line two with ✓ and 🎵\n\n- bullet\n";
    env.git
        .command(
            &repo,
            [
                "commit",
                "-q",
                "--author=Trần Thị Bích <bích@ví-dụ.vn>",
                "-F",
                "-",
            ],
        )
        .stdin(message)
        .output()
        .await
        .unwrap();

    let commits = env.git.log(&repo, &LogQuery::default()).await.unwrap();
    assert_eq!(commits.len(), 2);
    let newest = &commits[0];
    assert_eq!(newest.subject, "Mix v2: \"louder\" | tabs\there");
    assert_eq!(
        newest.body.trim_end(),
        "Body line one\nBody line two with ✓ and 🎵\n\n- bullet"
    );
    assert_eq!(newest.author_name, "Trần Thị Bích");
    assert_eq!(newest.author_email, "bích@ví-dụ.vn");
    assert_eq!(newest.parents, vec![commits[1].id.clone()]);
    assert!(newest.author_time > 1_600_000_000);
    assert!(newest.author_date.contains('T'));

    let oldest = &commits[1];
    assert_eq!(oldest.subject, "First take");
    assert_eq!(oldest.body, "");
    assert!(oldest.parents.is_empty());
    assert_eq!(oldest.author_name, "Test User");

    let limited = env
        .git
        .log(
            &repo,
            &LogQuery {
                range: None,
                limit: Some(1),
            },
        )
        .await
        .unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].id, newest.id);

    let ranged = env
        .git
        .log(
            &repo,
            &LogQuery {
                range: Some("HEAD~1..HEAD".into()),
                limit: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(ranged.len(), 1);
    assert_eq!(ranged[0].id, newest.id);
}

#[tokio::test]
async fn log_rejects_option_like_ranges() {
    let env = common::TestEnv::new();
    let repo = env.init_repo("log").await;
    let err = env
        .git
        .log(
            &repo,
            &LogQuery {
                range: Some("--output=/tmp/pwned".into()),
                limit: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::InvalidArgument(_)), "{err}");
}

#[tokio::test]
async fn log_of_empty_repo_is_a_typed_error() {
    let env = common::TestEnv::new();
    let repo = env.init_repo("empty").await;
    let err = env.git.log(&repo, &LogQuery::default()).await.unwrap_err();
    assert!(
        matches!(
            err,
            GitError::NonZeroExit {
                code: Some(128),
                ..
            }
        ),
        "{err}"
    );
}
