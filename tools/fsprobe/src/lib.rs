//! `fsprobe`: records how a DAW saves its files.
//!
//! A guided session (`fsprobe run`) walks a musician through a protocol of
//! steps (see `protocols/`), records every filesystem event under the watched
//! folders into `trace.jsonl`, and classifies what happened to each file per
//! step (in-place write, temp file then rename, delete then create, ...).
//!
//! The classifier ([`classify`]) is pure: it works on trace records only, so a
//! returned results folder can be re-analysed with `fsprobe report`.

pub mod actions;
pub mod classify;
pub mod console;
pub mod environment;
pub mod error;
pub mod paths;
pub mod protocol;
pub mod recorder;
pub mod report;
pub mod session;
pub mod trace;

pub use error::ProbeError;

/// Version of this tool, recorded in every results folder.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
