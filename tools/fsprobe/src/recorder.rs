//! Records raw filesystem events, polls recently touched files until they
//! settle, and writes everything in order to `trace.jsonl`.
//!
//! One thread owns the trace writer. The OS watcher callback stats each path
//! as soon as its event arrives and hands both to that thread over a channel;
//! markers and action results from the guided session go through the same
//! channel, so the file order is the order things happened.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use notify::event::{
    AccessKind, AccessMode, CreateKind, DataChange, MetadataKind, ModifyKind, RemoveKind,
    RenameMode,
};
use notify::{EventKind, RecursiveMode, Watcher};

use crate::error::ProbeError;
use crate::paths::{WatchRoot, to_ref};
use crate::trace::{Body, Phase, Record, Stat, TraceWriter, wall_string};

/// Recorder timing.
#[derive(Debug, Clone)]
pub struct RecorderOptions {
    /// How often recently touched files are stat'ed.
    pub poll_every: Duration,
    /// A file stops being polled once it has not changed for this long.
    pub hot_for: Duration,
    /// Safety cap on the initial scan.
    pub max_indexed: u64,
}

impl Default for RecorderOptions {
    fn default() -> Self {
        Self {
            poll_every: Duration::from_millis(200),
            hot_for: Duration::from_secs(3),
            max_indexed: 500_000,
        }
    }
}

enum Msg {
    Fs {
        res: notify::Result<notify::Event>,
        at: Instant,
        wall: SystemTime,
        stats: Vec<Stat>,
    },
    Write {
        body: Body,
        at: Instant,
        wall: SystemTime,
    },
    Tool(Option<String>),
    Flush(Sender<Result<(), String>>),
    Stop,
}

#[derive(Debug)]
struct Shared {
    last_activity: Instant,
    error: Option<String>,
}

/// A running recorder. Stop it with [`Recorder::stop`] (dropping also stops it).
pub struct Recorder {
    tx: Sender<Msg>,
    shared: Arc<Mutex<Shared>>,
    thread: Option<JoinHandle<()>>,
    watcher: Option<notify::RecommendedWatcher>,
    roots: Vec<WatchRoot>,
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder")
            .field("roots", &self.roots)
            .finish_non_exhaustive()
    }
}

impl Recorder {
    /// Index the roots, start watching them and start writing `trace`.
    pub fn start(
        roots: Vec<WatchRoot>,
        trace: &Path,
        run: u32,
        opts: RecorderOptions,
    ) -> Result<Self, ProbeError> {
        let writer = TraceWriter::open(trace)?;
        let mut index = HashMap::new();
        let mut indexed = 0u64;
        for root in &roots {
            indexed += scan(
                &root.path,
                &mut index,
                opts.max_indexed.saturating_sub(indexed),
            );
        }
        let run_start = Instant::now();
        let shared = Arc::new(Mutex::new(Shared {
            last_activity: run_start,
            error: None,
        }));
        let (tx, rx) = mpsc::channel();
        let mut state = Worker {
            roots: roots.clone(),
            writer,
            run,
            run_start,
            seq: 0,
            index,
            hot: HashMap::new(),
            touched: BTreeSet::new(),
            tool: None,
            shared: Arc::clone(&shared),
            opts: opts.clone(),
            dirty: false,
        };
        let thread = std::thread::Builder::new()
            .name("fsprobe-recorder".into())
            .spawn(move || state.run(rx))
            .map_err(|e| ProbeError::Watcher(format!("could not start the recorder: {e}")))?;

        let cb_tx = tx.clone();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let at = Instant::now();
            let wall = SystemTime::now();
            let stats = match &res {
                Ok(ev) => ev.paths.iter().map(|p| Stat::of(p)).collect(),
                Err(_) => Vec::new(),
            };
            let _ = cb_tx.send(Msg::Fs {
                res,
                at,
                wall,
                stats,
            });
        })
        .map_err(|e| ProbeError::Watcher(e.to_string()))?;
        for root in &roots {
            watcher
                .watch(&root.path, RecursiveMode::Recursive)
                .map_err(|e| ProbeError::WatchFolder {
                    path: root.path.clone(),
                    message: e.to_string(),
                })?;
        }
        let rec = Self {
            tx,
            shared,
            thread: Some(thread),
            watcher: Some(watcher),
            roots,
        };
        rec.write(Body::RunStart {
            tool_version: crate::TOOL_VERSION.to_string(),
            os: Some(std::env::consts::OS.to_string()),
            roots: rec
                .roots
                .iter()
                .map(|r| r.path.to_string_lossy().to_string())
                .collect(),
            indexed,
        });
        Ok(rec)
    }

    /// The watched folders.
    pub fn roots(&self) -> &[WatchRoot] {
        &self.roots
    }

    /// Append a record (in order with the events).
    pub fn write(&self, body: Body) {
        let _ = self.tx.send(Msg::Write {
            body,
            at: Instant::now(),
            wall: SystemTime::now(),
        });
    }

    /// Append a step marker.
    pub fn marker(&self, step: &str, phase: Phase) {
        self.write(Body::Marker {
            step: step.to_string(),
            phase,
        });
    }

    /// Tag the following records as caused by a helper action (or stop tagging).
    pub fn set_tool(&self, action: Option<&str>) {
        let _ = self.tx.send(Msg::Tool(action.map(str::to_string)));
    }

    /// Wait until everything sent so far is on disk.
    pub fn flush(&self) -> Result<(), ProbeError> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Msg::Flush(tx))
            .map_err(|_| ProbeError::Watcher("the recorder stopped unexpectedly".into()))?;
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(Ok(())) => self.check(),
            Ok(Err(e)) => Err(ProbeError::Watcher(e)),
            Err(_) => Err(ProbeError::Watcher(
                "the recorder did not respond within 30 seconds".into(),
            )),
        }
    }

    /// Report a background failure (for example, the disk is full).
    pub fn check(&self) -> Result<(), ProbeError> {
        match self.shared.lock() {
            Ok(s) => match &s.error {
                Some(e) => Err(ProbeError::Watcher(e.clone())),
                None => Ok(()),
            },
            Err(_) => Err(ProbeError::Watcher("the recorder crashed".into())),
        }
    }

    /// Time since the last filesystem change was seen.
    pub fn idle_for(&self) -> Duration {
        self.shared
            .lock()
            .map(|s| s.last_activity.elapsed())
            .unwrap_or_default()
    }

    /// Block until nothing has changed for `quiet`, or `max` has passed.
    /// Returns `true` if it went quiet.
    ///
    /// The quiet period counts from the call at the earliest: the OS delivers
    /// events a little late, so a change made just before the call must still
    /// have time to show up.
    pub fn wait_quiet(&self, quiet: Duration, max: Duration) -> bool {
        let started = Instant::now();
        loop {
            if self.idle_for().min(started.elapsed()) >= quiet {
                return true;
            }
            if started.elapsed() >= max {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Stop watching and close the trace.
    pub fn stop(mut self) -> Result<(), ProbeError> {
        self.shutdown();
        self.check()
    }

    fn shutdown(&mut self) {
        // Stop OS events first so nothing arrives after the last record.
        self.watcher.take();
        let _ = self.tx.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Worker {
    roots: Vec<WatchRoot>,
    writer: TraceWriter,
    run: u32,
    run_start: Instant,
    seq: u64,
    index: HashMap<PathBuf, Stat>,
    /// Recently touched files: last stat and when it last changed.
    hot: HashMap<PathBuf, (Stat, Instant)>,
    /// Paths touched since the last step start.
    touched: BTreeSet<PathBuf>,
    tool: Option<String>,
    shared: Arc<Mutex<Shared>>,
    opts: RecorderOptions,
    dirty: bool,
}

impl Worker {
    fn run(&mut self, rx: Receiver<Msg>) {
        let mut next_poll = Instant::now() + self.opts.poll_every;
        loop {
            let wait = next_poll.saturating_duration_since(Instant::now());
            match rx.recv_timeout(wait) {
                Ok(Msg::Stop) => break,
                Ok(msg) => self.handle(msg),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if Instant::now() >= next_poll {
                self.poll();
                self.flush_if_dirty();
                next_poll = Instant::now() + self.opts.poll_every;
            }
        }
        // Drain whatever is still queued (events that raced the stop).
        while let Ok(msg) = rx.try_recv() {
            if !matches!(msg, Msg::Stop) {
                self.handle(msg);
            }
        }
        self.flush_if_dirty();
        if let Err(e) = self.writer.flush() {
            self.fail(e.to_string());
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Fs {
                res,
                at,
                wall,
                stats,
            } => self.on_fs(res, at, wall, stats),
            Msg::Write { body, at, wall } => {
                match &body {
                    Body::Marker {
                        phase: Phase::Start,
                        ..
                    } => self.touched.clear(),
                    Body::Marker {
                        phase: Phase::End, ..
                    } => self.write_finals(at, wall),
                    _ => {}
                }
                self.emit(at, wall, body);
                self.flush_if_dirty();
            }
            Msg::Tool(t) => self.tool = t,
            Msg::Flush(reply) => {
                self.flush_if_dirty();
                let res = self.writer.flush().map_err(|e| e.to_string());
                let _ = reply.send(res);
            }
            Msg::Stop => {}
        }
    }

    fn on_fs(
        &mut self,
        res: notify::Result<notify::Event>,
        at: Instant,
        wall: SystemTime,
        stats: Vec<Stat>,
    ) {
        let event = match res {
            Ok(ev) => ev,
            Err(e) => {
                self.emit(
                    at,
                    wall,
                    Body::WatchError {
                        message: e.to_string(),
                    },
                );
                return;
            }
        };
        self.activity(at);
        let mut refs = Vec::with_capacity(event.paths.len());
        let mut prevs = Vec::with_capacity(event.paths.len());
        for (i, path) in event.paths.iter().enumerate() {
            let stat = stats.get(i).cloned().unwrap_or_default();
            let prev = self.index.get(path).cloned().unwrap_or_default();
            refs.push(to_ref(path, &self.roots));
            prevs.push(prev);
            self.remember(path, &stat, at);
            self.touched.insert(path.clone());
        }
        let body = Body::Event {
            kind: kind_name(&event.kind),
            paths: refs,
            stat: stats,
            prev: prevs,
            tracker: event.tracker(),
            info: event.info().map(str::to_string),
            rescan: event.need_rescan(),
            tool: self.tool.clone(),
        };
        self.emit(at, wall, body);
    }

    fn remember(&mut self, path: &Path, stat: &Stat, at: Instant) {
        if stat.exists {
            self.index.insert(path.to_path_buf(), stat.clone());
            if !stat.dir {
                self.hot.insert(path.to_path_buf(), (stat.clone(), at));
            }
        } else {
            self.index.remove(path);
            self.hot.remove(path);
        }
    }

    fn poll(&mut self) {
        let now = Instant::now();
        let paths: Vec<PathBuf> = self.hot.keys().cloned().collect();
        for path in paths {
            let stat = Stat::of(&path);
            let Some((last, since)) = self.hot.get(&path).cloned() else {
                continue;
            };
            if !stat.same_state(&last) {
                let prev = self.index.get(&path).cloned().unwrap_or_default();
                let body = Body::Poll {
                    path: to_ref(&path, &self.roots),
                    stat: stat.clone(),
                    prev,
                    tool: self.tool.clone(),
                };
                self.activity(now);
                self.touched.insert(path.clone());
                self.remember(&path, &stat, now);
                self.emit(now, SystemTime::now(), body);
            } else if now.duration_since(since) >= self.opts.hot_for {
                self.hot.remove(&path);
            }
        }
    }

    fn write_finals(&mut self, at: Instant, wall: SystemTime) {
        let touched: Vec<PathBuf> = std::mem::take(&mut self.touched).into_iter().collect();
        for path in touched {
            let stat = Stat::of(&path);
            let prev = self.index.get(&path).cloned().unwrap_or_default();
            let body = Body::Final {
                path: to_ref(&path, &self.roots),
                stat: stat.clone(),
                prev,
            };
            if stat.exists {
                self.index.insert(path.clone(), stat);
            } else {
                self.index.remove(&path);
            }
            self.emit(at, wall, body);
        }
    }

    fn activity(&self, at: Instant) {
        if let Ok(mut s) = self.shared.lock()
            && at > s.last_activity
        {
            s.last_activity = at;
        }
    }

    fn emit(&mut self, at: Instant, wall: SystemTime, body: Body) {
        self.seq += 1;
        let t_us = u64::try_from(at.saturating_duration_since(self.run_start).as_micros())
            .unwrap_or(u64::MAX);
        let record = Record {
            seq: self.seq,
            run: self.run,
            t_us,
            wall: wall_string(wall),
            body,
        };
        if let Err(e) = self.writer.write(&record) {
            self.fail(e.to_string());
        }
        self.dirty = true;
    }

    fn flush_if_dirty(&mut self) {
        if self.dirty {
            if let Err(e) = self.writer.flush() {
                self.fail(e.to_string());
            }
            self.dirty = false;
        }
    }

    fn fail(&self, message: String) {
        if let Ok(mut s) = self.shared.lock()
            && s.error.is_none()
        {
            s.error = Some(message);
        }
    }
}

/// Walk `root` and stat everything (no symlink following). Returns the count.
pub fn scan(root: &Path, index: &mut HashMap<PathBuf, Stat>, limit: u64) -> u64 {
    let mut count = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if count >= limit {
                return count;
            }
            let path = entry.path();
            let stat = Stat::of(&path);
            if stat.dir {
                stack.push(path.clone());
            }
            index.insert(path, stat);
            count += 1;
        }
    }
    count
}

/// Stable dotted name for an event kind, e.g. `modify.name.from`.
pub fn kind_name(kind: &EventKind) -> String {
    fn mode(m: &AccessMode) -> &'static str {
        match m {
            AccessMode::Any => "any",
            AccessMode::Execute => "execute",
            AccessMode::Read => "read",
            AccessMode::Write => "write",
            AccessMode::Other => "other",
        }
    }
    match kind {
        EventKind::Any => "any".into(),
        EventKind::Access(a) => match a {
            AccessKind::Any => "access.any".into(),
            AccessKind::Read => "access.read".into(),
            AccessKind::Open(m) => format!("access.open.{}", mode(m)),
            AccessKind::Close(m) => format!("access.close.{}", mode(m)),
            AccessKind::Other => "access.other".into(),
        },
        EventKind::Create(c) => match c {
            CreateKind::Any => "create.any",
            CreateKind::File => "create.file",
            CreateKind::Folder => "create.folder",
            CreateKind::Other => "create.other",
        }
        .into(),
        EventKind::Modify(m) => match m {
            ModifyKind::Any => "modify.any".into(),
            ModifyKind::Data(d) => match d {
                DataChange::Any => "modify.data.any",
                DataChange::Size => "modify.data.size",
                DataChange::Content => "modify.data.content",
                DataChange::Other => "modify.data.other",
            }
            .into(),
            ModifyKind::Metadata(md) => match md {
                MetadataKind::Any => "modify.metadata.any",
                MetadataKind::AccessTime => "modify.metadata.access_time",
                MetadataKind::WriteTime => "modify.metadata.write_time",
                MetadataKind::Permissions => "modify.metadata.permissions",
                MetadataKind::Ownership => "modify.metadata.ownership",
                MetadataKind::Extended => "modify.metadata.extended",
                MetadataKind::Other => "modify.metadata.other",
            }
            .into(),
            ModifyKind::Name(r) => match r {
                RenameMode::Any => "modify.name.any",
                RenameMode::To => "modify.name.to",
                RenameMode::From => "modify.name.from",
                RenameMode::Both => "modify.name.both",
                RenameMode::Other => "modify.name.other",
            }
            .into(),
            ModifyKind::Other => "modify.other".into(),
        },
        EventKind::Remove(r) => match r {
            RemoveKind::Any => "remove.any",
            RemoveKind::File => "remove.file",
            RemoveKind::Folder => "remove.folder",
            RemoveKind::Other => "remove.other",
        }
        .into(),
        EventKind::Other => "other".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_are_dotted() {
        assert_eq!(
            kind_name(&EventKind::Modify(ModifyKind::Name(RenameMode::From))),
            "modify.name.from"
        );
        assert_eq!(
            kind_name(&EventKind::Access(AccessKind::Close(AccessMode::Write))),
            "access.close.write"
        );
        assert_eq!(
            kind_name(&EventKind::Create(CreateKind::File)),
            "create.file"
        );
    }

    #[test]
    fn scan_counts_and_respects_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        std::fs::write(dir.path().join("a/b.txt"), b"x").unwrap();
        std::fs::write(dir.path().join("c.txt"), b"y").unwrap();
        let mut index = HashMap::new();
        assert_eq!(scan(dir.path(), &mut index, 100), 3);
        let mut small = HashMap::new();
        assert_eq!(scan(dir.path(), &mut small, 1), 1);
    }
}
