//! Trace records (`trace.jsonl`): one JSON object per line.
//!
//! Every record carries the run number (a resumed session is a new run), a
//! monotonic timestamp in microseconds since that run started, and the wall
//! clock time. Paths are stored relative to a watch root (`root` is the index
//! into the session's watch list) with `/` separators on every OS.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::ProbeError;

/// One line of `trace.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Sequence number within the run.
    pub seq: u64,
    /// Run number (1 for the first session, +1 per resume).
    pub run: u32,
    /// Monotonic microseconds since the run started.
    pub t_us: u64,
    /// Wall-clock time, RFC 3339 with milliseconds and local offset.
    pub wall: String,
    /// The record itself.
    #[serde(flatten)]
    pub body: Body,
}

/// What a record says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Body {
    /// The recorder started (once per run).
    RunStart {
        /// Tool version.
        tool_version: String,
        /// OS family (`macos`, `windows`, `linux`): tells the classifier how
        /// far to trust event flags.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        os: Option<String>,
        /// Watched folders, in root-index order.
        roots: Vec<String>,
        /// Number of files and folders found by the initial scan.
        indexed: u64,
    },
    /// A raw filesystem event from the OS.
    Event {
        /// Event kind, e.g. `create.file`, `modify.data.content`, `modify.name.from`.
        kind: String,
        /// Paths the event names (two for a paired rename).
        paths: Vec<PathRef>,
        /// Fresh stat of each path, taken when the event arrived.
        stat: Vec<Stat>,
        /// Last known stat of each path before this event.
        prev: Vec<Stat>,
        /// Rename cookie (Linux), pairing `from` and `to` halves.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tracker: Option<usize>,
        /// Extra information from the OS backend.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        info: Option<String>,
        /// The OS dropped events and asked for a rescan.
        #[serde(default, skip_serializing_if = "is_false")]
        rescan: bool,
        /// Set while one of the tool's own helper actions was running.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<String>,
    },
    /// A change found by polling a recently touched file (no event needed).
    Poll {
        /// Path.
        path: PathRef,
        /// New stat.
        stat: Stat,
        /// Previous stat.
        prev: Stat,
        /// Set while one of the tool's own helper actions was running.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<String>,
    },
    /// Final stat of a path touched during a step, taken at the step's end.
    Final {
        /// Path.
        path: PathRef,
        /// Stat at the end of the step.
        stat: Stat,
        /// Last known stat before this one.
        prev: Stat,
    },
    /// Step boundary.
    Marker {
        /// Step id from the protocol.
        step: String,
        /// Which boundary.
        phase: Phase,
    },
    /// Result of a helper action.
    Action {
        /// Step id.
        step: String,
        /// Action name, e.g. `set_readonly`.
        action: String,
        /// Whether it worked.
        ok: bool,
        /// Plain-language result shown to the user.
        message: String,
        /// Structured details (open handles, snapshot file, ...).
        #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
        detail: serde_json::Value,
    },
    /// The watcher reported an error.
    WatchError {
        /// Message.
        message: String,
    },
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Step boundary kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// The user pressed Enter to start the step.
    Start,
    /// The user pressed Enter to say they had finished.
    Done,
    /// The filesystem went quiet and the step's window closed.
    End,
}

/// A path relative to one of the watch roots.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PathRef {
    /// Index into the session's watch roots; `None` if outside all of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<usize>,
    /// Path relative to the root with `/` separators (absolute if `root` is `None`).
    pub rel: String,
}

impl PathRef {
    /// Last path component.
    pub fn file_name(&self) -> &str {
        self.rel.rsplit('/').next().unwrap_or(&self.rel)
    }

    /// Parent directory part (`""` at the root).
    pub fn parent(&self) -> &str {
        match self.rel.rfind('/') {
            Some(i) => &self.rel[..i],
            None => "",
        }
    }
}

/// What a `stat` saw.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stat {
    /// The path existed.
    pub exists: bool,
    /// It is a folder.
    #[serde(default, skip_serializing_if = "is_false")]
    pub dir: bool,
    /// Size in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Modification time, nanoseconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_ns: Option<u64>,
    /// Read-only flag (Unix: no write permission bits at all).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
    /// Unix permission bits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
    /// Inode number (Unix only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inode: Option<u64>,
    /// Stat failed for a reason other than "not found".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Stat {
    /// The "does not exist" stat.
    pub fn missing() -> Self {
        Self::default()
    }

    /// Stat `path` without following a final symlink.
    pub fn of(path: &Path) -> Self {
        match std::fs::symlink_metadata(path) {
            Ok(meta) => Self::from_metadata(&meta),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Self::missing(),
            Err(err) => Self {
                exists: path.exists(),
                error: Some(err.to_string()),
                ..Self::default()
            },
        }
    }

    /// Build from metadata.
    pub fn from_metadata(meta: &std::fs::Metadata) -> Self {
        let mtime_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| u64::try_from(d.as_nanos()).ok());
        let (mode, inode) = unix_ids(meta);
        Self {
            exists: true,
            dir: meta.is_dir(),
            size: if meta.is_dir() {
                None
            } else {
                Some(meta.len())
            },
            mtime_ns,
            readonly: Some(meta.permissions().readonly()),
            mode,
            inode,
            error: None,
        }
    }

    /// Same content-relevant state (existence, identity, size, mtime, flags)?
    pub fn same_state(&self, other: &Stat) -> bool {
        self.exists == other.exists
            && self.dir == other.dir
            && self.size == other.size
            && self.mtime_ns == other.mtime_ns
            && self.readonly == other.readonly
            && self.mode == other.mode
            && self.inode == other.inode
    }
}

#[cfg(unix)]
fn unix_ids(meta: &std::fs::Metadata) -> (Option<u32>, Option<u64>) {
    use std::os::unix::fs::MetadataExt;
    (Some(meta.mode() & 0o7777), Some(meta.ino()))
}

#[cfg(not(unix))]
fn unix_ids(_meta: &std::fs::Metadata) -> (Option<u32>, Option<u64>) {
    (None, None)
}

/// Current wall-clock time as RFC 3339 with milliseconds and local offset.
pub fn wall_now() -> String {
    wall_string(SystemTime::now())
}

/// Format a wall-clock time as RFC 3339 with milliseconds and local offset.
pub fn wall_string(t: SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Local> = t.into();
    dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, false)
}

/// Appends records to `trace.jsonl`.
#[derive(Debug)]
pub struct TraceWriter {
    path: PathBuf,
    out: BufWriter<File>,
}

impl TraceWriter {
    /// Open (or create) the trace for appending.
    pub fn open(path: &Path) -> Result<Self, ProbeError> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| ProbeError::io("open", path, e))?;
        Ok(Self {
            path: path.to_path_buf(),
            out: BufWriter::new(file),
        })
    }

    /// Append one record (buffered).
    pub fn write(&mut self, record: &Record) -> Result<(), ProbeError> {
        let line = serde_json::to_string(record).map_err(|e| ProbeError::ResultsDamaged {
            path: self.path.clone(),
            message: e.to_string(),
        })?;
        self.out
            .write_all(line.as_bytes())
            .and_then(|()| self.out.write_all(b"\n"))
            .map_err(|e| ProbeError::io("write to", &self.path, e))
    }

    /// Push buffered records to disk.
    pub fn flush(&mut self) -> Result<(), ProbeError> {
        self.out
            .flush()
            .map_err(|e| ProbeError::io("write to", &self.path, e))
    }
}

/// Read every valid record. A damaged line (for example the last line of a
/// session that was killed mid-write) is skipped and counted.
pub fn read_trace(path: &Path) -> Result<(Vec<Record>, usize), ProbeError> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(e) => return Err(ProbeError::io("read", path, e)),
    };
    let mut records = Vec::new();
    let mut damaged = 0;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| ProbeError::io("read", path, e))?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Record>(&line) {
            Ok(r) => records.push(r),
            Err(_) => damaged += 1,
        }
    }
    Ok((records, damaged))
}

/// The records strictly between the last `start` marker of `step` and the
/// `end` marker that follows it. `None` if the step never finished.
pub fn step_window<'a>(records: &'a [Record], step: &str) -> Option<&'a [Record]> {
    let start = records.iter().rposition(
        |r| matches!(&r.body, Body::Marker { step: s, phase: Phase::Start } if s == step),
    )?;
    let rest = records.get(start + 1..)?;
    let end = rest.iter().position(
        |r| matches!(&r.body, Body::Marker { step: s, phase: Phase::End } if s == step),
    )?;
    rest.get(..end)
}

/// The OS recorded at the start of the run a window belongs to.
pub fn run_os<'a>(records: &'a [Record], window: &[Record]) -> Option<&'a str> {
    let run = window.first()?.run;
    records.iter().find_map(|r| match &r.body {
        Body::RunStart { os, .. } if r.run == run => os.as_deref(),
        _ => None,
    })
}

/// Records outside every step window (while the user reads instructions or
/// answers questions), split at step boundaries and run starts. Each gap is
/// labelled with the step that ended just before it. Gaps without any OS
/// event are left out.
pub fn gaps(records: &[Record]) -> Vec<(Option<String>, &[Record])> {
    let mut out = Vec::new();
    let mut start: Option<usize> = Some(0);
    let mut label: Option<String> = None;
    let mut close = |from: Option<usize>, to: usize, label: &Option<String>| {
        if let Some(slice) = from.and_then(|f| records.get(f..to))
            && slice.iter().any(|r| matches!(r.body, Body::Event { .. }))
        {
            out.push((label.clone(), slice));
        }
    };
    for (i, r) in records.iter().enumerate() {
        match &r.body {
            Body::RunStart { .. } => {
                close(start, i, &label);
                start = Some(i + 1);
                label = None;
            }
            Body::Marker {
                phase: Phase::Start,
                ..
            } => {
                close(start, i, &label);
                start = None;
            }
            Body::Marker {
                step,
                phase: Phase::End,
            } => {
                start = Some(i + 1);
                label = Some(step.clone());
            }
            _ => {}
        }
    }
    close(start, records.len(), &label);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_cover_time_between_steps() {
        let ev = |seq| Record {
            seq,
            run: 1,
            t_us: seq,
            wall: String::new(),
            body: Body::Event {
                kind: "create.file".into(),
                paths: vec![],
                stat: vec![],
                prev: vec![],
                tracker: None,
                info: None,
                rescan: false,
                tool: None,
            },
        };
        let records = vec![
            ev(1),
            marker(2, "a", Phase::Start),
            ev(3),
            marker(4, "a", Phase::End),
            ev(5),
            marker(6, "b", Phase::Start),
            marker(7, "b", Phase::End),
            marker(8, "c", Phase::Start),
            marker(9, "c", Phase::End),
            ev(10),
        ];
        let g = gaps(&records);
        let labels: Vec<Option<&str>> = g.iter().map(|(l, _)| l.as_deref()).collect();
        assert_eq!(labels, vec![None, Some("a"), Some("c")]);
        assert_eq!(g[1].1.len(), 1);
    }

    fn marker(seq: u64, step: &str, phase: Phase) -> Record {
        Record {
            seq,
            run: 1,
            t_us: seq,
            wall: String::new(),
            body: Body::Marker {
                step: step.into(),
                phase,
            },
        }
    }

    #[test]
    fn record_round_trips_through_json() {
        let r = Record {
            seq: 3,
            run: 1,
            t_us: 42,
            wall: "2026-11-19T10:00:00.000+01:00".into(),
            body: Body::Event {
                kind: "modify.name.any".into(),
                paths: vec![PathRef {
                    root: Some(0),
                    rel: "Project/Song A.flp".into(),
                }],
                stat: vec![Stat {
                    exists: true,
                    size: Some(10),
                    ..Stat::default()
                }],
                prev: vec![Stat::missing()],
                tracker: None,
                info: None,
                rescan: false,
                tool: None,
            },
        };
        let line = serde_json::to_string(&r).unwrap();
        assert!(line.contains("\"type\":\"event\""));
        let back: Record = serde_json::from_str(&line).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn window_uses_last_completed_attempt() {
        let records = vec![
            marker(1, "s1", Phase::Start),
            marker(2, "s1", Phase::Start),
            marker(3, "x", Phase::Done),
            marker(4, "s1", Phase::End),
        ];
        let w = step_window(&records, "s1").unwrap();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].seq, 3);
        assert!(step_window(&records, "s2").is_none());
    }

    #[test]
    fn unfinished_step_has_no_window() {
        let records = vec![marker(1, "s1", Phase::Start)];
        assert!(step_window(&records, "s1").is_none());
    }

    #[test]
    fn path_ref_parts() {
        let p = PathRef {
            root: Some(0),
            rel: "Project/Song A.flp".into(),
        };
        assert_eq!(p.file_name(), "Song A.flp");
        assert_eq!(p.parent(), "Project");
        let top = PathRef {
            root: Some(0),
            rel: "a.txt".into(),
        };
        assert_eq!(top.parent(), "");
    }
}
