//! Git and git-lfs process runner.
//!
//! Runs the bundled (or system) `git` and `git-lfs` executables as child
//! processes and parses their output. Arguments are always passed as
//! arrays, never as shell strings; no libgit2/gitoxide.
//!
//! - [`GitLocation`] finds the executables; [`Git`] wraps one and builds
//!   [`Invocation`]s with a scrubbed environment.
//! - [`Invocation::output`] captures output; [`Invocation::stream`] delivers
//!   lines as they arrive. Both honour a [`CancellationToken`] and an
//!   optional timeout and clean up the child's process group (see
//!   [`runner`] for platform details).
//! - Pure parsers: [`parse_status`], [`parse_log`], [`parse_lfs_progress_line`].
//! - Typed helpers: [`Git::status`], [`Git::log`], [`Git::version`].
//!
//! The LFS lock HTTP client and credentials arrive with later tasks in
//! `docs/PLAN.md`.

mod error;
mod git;
pub mod lfs;
mod locate;
pub mod log;
mod paths;
pub mod runner;
pub mod status;

pub use error::{GitError, ParseError, STDERR_TAIL_LIMIT};
pub use git::{Git, LogQuery, StatusOptions, UntrackedFiles};
pub use lfs::{
    LfsDirection, LfsProgress, LfsProgressTail, parse_lfs_progress, parse_lfs_progress_line,
};
pub use locate::{
    GitLocation, GitVersion, LfsVersion, Versions, parse_git_version, parse_lfs_version,
};
pub use log::{Commit, LOG_FORMAT, parse_log};
pub use runner::{Invocation, Output, OutputLine, StreamKind, StreamOutput};
pub use status::{StatusEntry, StatusReport, parse_status};
pub use tokio_util::sync::CancellationToken;
