//! Pure classification of the events between two step markers.
//!
//! The OS backends disagree on how much they report:
//!
//! - Linux (inotify) pairs rename halves with a cookie and is precise.
//! - Windows (ReadDirectoryChangesW) reports `from` immediately followed by
//!   `to`, without a cookie, and no file identity.
//! - macOS (FSEvents) reports a bare "renamed" flag on each side of a rename
//!   and may merge several changes into one event, so its flags alone are not
//!   trustworthy.
//!
//! The classifier therefore leans on the stats the recorder takes with every
//! event (existence, size, mtime, inode) and uses event kinds as hints.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::paths::glob_match;
use crate::trace::{Body, PathRef, Record, Stat};

/// Classification settings.
#[derive(Debug, Clone)]
pub struct ClassifyOptions {
    /// File-name or relative-path globs to leave out (`.DS_Store`, ...).
    pub ignore: Vec<String>,
    /// How close in time two rename halves must be to be paired by timing.
    pub rename_window_us: u64,
    /// A pause longer than this splits a file's activity into separate writes.
    pub burst_gap_us: u64,
    /// The trace comes from macOS FSEvents, whose create/rename/delete flags
    /// can be left over from earlier changes to the same path.
    pub sticky_flags: bool,
}

impl Default for ClassifyOptions {
    fn default() -> Self {
        Self {
            ignore: Vec::new(),
            rename_window_us: 2_000_000,
            burst_gap_us: 2_000_000,
            sticky_flags: false,
        }
    }
}

impl ClassifyOptions {
    /// Options for a trace recorded on `os` (as stored in the run's start record).
    pub fn for_os(mut self, os: Option<&str>) -> Self {
        self.sticky_flags = os == Some("macos");
        self
    }
}

/// Events for one path reported within this many microseconds of each other
/// come from one OS notification (FSEvents reports one flag set per path,
/// which `notify` splits into several events).
const BATCH_US: u64 = 5_000;

/// How a file changed during a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    /// Written through a temporary file that was then renamed onto it.
    TempThenRename,
    /// Same file (same identity) rewritten in place.
    InPlaceWrite,
    /// Deleted, then created again under the same name.
    DeleteThenCreate,
    /// The old file was renamed away (a backup), then a new one was written.
    MovedAsideThenCreate,
    /// Changed in a way none of the known patterns explain.
    Unclassified,
    /// Did not exist before the step, exists after.
    NewFile,
    /// An existing file was renamed or moved to this path.
    Renamed,
    /// Existed before the step, gone after.
    Removed,
    /// Only attributes changed (read-only flag, permissions).
    MetadataOnly,
    /// Appeared and disappeared within the step.
    Transient,
    /// Events were reported but nothing visible changed.
    Unchanged,
}

impl Pattern {
    /// Snake-case name as written to JSON and `summary.md`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TempThenRename => "temp_then_rename",
            Self::InPlaceWrite => "in_place_write",
            Self::DeleteThenCreate => "delete_then_create",
            Self::MovedAsideThenCreate => "moved_aside_then_create",
            Self::Unclassified => "unclassified",
            Self::NewFile => "new_file",
            Self::Renamed => "renamed",
            Self::Removed => "removed",
            Self::MetadataOnly => "metadata_only",
            Self::Transient => "transient",
            Self::Unchanged => "unchanged",
        }
    }
}

/// One file's change during a step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileChange {
    /// The file.
    pub path: PathRef,
    /// It is a folder.
    pub dir: bool,
    /// What happened.
    pub pattern: Pattern,
    /// Existed when the step started.
    pub existed_before: bool,
    /// Exists when the step ended.
    pub exists_after: bool,
    /// Temporary file renamed onto this one (`temp_then_rename`).
    pub temp: Option<PathRef>,
    /// Temporary name relative to the target, e.g. `{name}.tmp`, `~{stem}.tmp`.
    pub temp_pattern: Option<String>,
    /// The temporary file was not inside any watched folder.
    pub temp_outside: bool,
    /// For `renamed` / backup copies: where this file came from.
    pub renamed_from: Option<PathRef>,
    /// For `moved_aside_then_create`: where the old file went.
    pub moved_to: Option<PathRef>,
    /// Size at the start of the step.
    pub size_before: Option<u64>,
    /// Size at the end of the step.
    pub size_after: Option<u64>,
    /// Read-only flag at the start.
    pub readonly_before: Option<bool>,
    /// Read-only flag at the end.
    pub readonly_after: Option<bool>,
    /// The read-only flag differs between start and end.
    pub readonly_changed: bool,
    /// Longest time from the first sign of a write until size and mtime
    /// stopped changing, in milliseconds.
    pub settle_ms: Option<u64>,
    /// Number of separate bursts of activity (e.g. three saves).
    pub times: u32,
    /// Why the classifier decided this.
    pub evidence: String,
    /// The raw observations, oldest first (`+ms kind state`).
    pub sequence: Vec<String>,
}

/// Everything seen in one step window.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    /// Changed files, most significant first.
    pub files: Vec<FileChange>,
    /// Filesystem events from the OS in the window (excluding the tool's own).
    pub events: usize,
    /// Events caused by the tool's helper actions (left out of `files`).
    pub tool_events: usize,
    /// The OS reported dropped events.
    pub missed_events: bool,
    /// Watcher errors in the window.
    pub watch_errors: Vec<String>,
}

const MAX_SEQUENCE: usize = 40;

#[derive(Debug, Clone)]
struct Obs {
    t: u64,
    /// Event kind; `poll` / `final` for those records.
    kind: String,
    stat: Stat,
    prev: Stat,
    idx: usize,
}

#[derive(Debug, Default)]
struct Track {
    obs: Vec<Obs>,
    created: bool,
    removed: bool,
    renamed: bool,
    renamed_to: bool,
    data_event: bool,
    /// Flags discounted as left over from earlier changes (macOS).
    leftover: Vec<&'static str>,
}

/// One OS notification for one path (tool-caused ones included).
#[derive(Debug)]
struct Batch {
    t: u64,
    tool: bool,
    created: bool,
    removed: bool,
    renamed: bool,
    pre: Stat,
    post: Stat,
}

impl Batch {
    /// A create, delete or rename at this path always changes which file is
    /// there; if the identity is unchanged, such flags are left over.
    fn identity_kept(&self) -> bool {
        self.pre.exists
            && self.post.exists
            && self.pre.inode.is_some()
            && self.pre.inode == self.post.inode
    }
}

impl Track {
    fn before(&self) -> Stat {
        self.obs.first().map(|o| o.prev.clone()).unwrap_or_default()
    }

    fn after(&self) -> Stat {
        self.obs.last().map(|o| o.stat.clone()).unwrap_or_default()
    }

    fn is_dir(&self) -> bool {
        self.before().dir || self.obs.iter().any(|o| o.stat.dir)
    }

    /// Inodes this path had at any point, including before the step.
    fn inodes(&self) -> BTreeSet<u64> {
        let mut set: BTreeSet<u64> = self
            .obs
            .iter()
            .filter(|o| o.stat.exists)
            .filter_map(|o| o.stat.inode)
            .collect();
        if let Some(i) = self.before().inode {
            set.insert(i);
        }
        set
    }

    /// Inodes seen during the step that differ from the starting one.
    fn new_inodes(&self) -> BTreeSet<u64> {
        let before = self.before();
        self.obs
            .iter()
            .filter(|o| o.stat.exists)
            .filter_map(|o| o.stat.inode)
            .filter(|i| !(before.exists && before.inode == Some(*i)))
            .collect()
    }

    fn rename_times(&self) -> Vec<u64> {
        self.obs
            .iter()
            .filter(|o| o.kind.starts_with("modify.name"))
            .map(|o| o.t)
            .collect()
    }
}

/// An OS event, as needed for rename pairing.
struct Ev<'a> {
    kind: &'a str,
    paths: &'a [PathRef],
    tracker: Option<usize>,
    t: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PairHow {
    Event,
    Cookie,
    Adjacent,
    Inode,
    Timing,
}

impl PairHow {
    fn as_str(self) -> &'static str {
        match self {
            Self::Event => "paired rename event",
            Self::Cookie => "rename cookie",
            Self::Adjacent => "rename from/to reported back to back",
            Self::Inode => "same file identity (inode)",
            Self::Timing => "rename events at the same moment",
        }
    }
}

/// Classify the records of one step window (see [`crate::trace::step_window`]).
pub fn classify(records: &[Record], opts: &ClassifyOptions) -> Observation {
    let mut obs_out = Observation::default();
    let base = records.first().map(|r| r.t_us).unwrap_or(0);
    let mut tracks: BTreeMap<PathRef, Track> = BTreeMap::new();
    // Event records in order, for pairing.
    let mut events: Vec<Ev<'_>> = Vec::new();
    // Per path, OS notifications (tool-caused included) for leftover flags.
    let mut batches: BTreeMap<PathRef, Vec<Batch>> = BTreeMap::new();

    for (idx, rec) in records.iter().enumerate() {
        match &rec.body {
            Body::Event {
                kind,
                paths,
                stat,
                prev,
                tracker,
                rescan,
                tool,
                ..
            } => {
                if let [path] = paths.as_slice() {
                    add_to_batch(
                        batches.entry(path.clone()).or_default(),
                        rec.t_us,
                        tool.is_some(),
                        kind,
                        prev.first(),
                        stat.first(),
                    );
                }
                if tool.is_some() {
                    obs_out.tool_events += 1;
                    continue;
                }
                obs_out.events += 1;
                if *rescan {
                    obs_out.missed_events = true;
                }
                events.push(Ev {
                    kind: kind.as_str(),
                    paths: paths.as_slice(),
                    tracker: *tracker,
                    t: rec.t_us,
                });
                for (i, path) in paths.iter().enumerate() {
                    let track = tracks.entry(path.clone()).or_default();
                    let k = kind.as_str();
                    if paths.len() > 1 {
                        // Paired renames (Linux) are always real.
                        track.renamed = true;
                    }
                    track.renamed_to |=
                        k == "modify.name.to" || (k == "modify.name.both" && i == 1);
                    track.data_event |= k.starts_with("modify.data")
                        || k == "modify.any"
                        || k == "access.close.write";
                    track.obs.push(Obs {
                        t: rec.t_us,
                        kind: k.to_string(),
                        stat: stat.get(i).cloned().unwrap_or_default(),
                        prev: prev.get(i).cloned().unwrap_or_default(),
                        idx,
                    });
                }
            }
            Body::Poll {
                path,
                stat,
                prev,
                tool,
            } => {
                if tool.is_some() {
                    continue;
                }
                tracks.entry(path.clone()).or_default().obs.push(Obs {
                    t: rec.t_us,
                    kind: "poll".into(),
                    stat: stat.clone(),
                    prev: prev.clone(),
                    idx,
                });
            }
            Body::Final { path, stat, prev } => {
                if let Some(track) = tracks.get_mut(path) {
                    track.obs.push(Obs {
                        t: rec.t_us,
                        kind: "final".into(),
                        stat: stat.clone(),
                        prev: prev.clone(),
                        idx,
                    });
                }
            }
            Body::WatchError { message } => obs_out.watch_errors.push(message.clone()),
            _ => {}
        }
    }

    tracks.retain(|path, _| !path.rel.is_empty() && !ignored(path, &opts.ignore));
    for (path, track) in tracks.iter_mut() {
        if let Some(b) = batches.get(path) {
            count_flags(track, b, opts.sticky_flags);
        }
    }

    let pairs = find_pairs(&tracks, &events, opts);

    // Sources whose content now lives at their destination.
    let mut consumed: HashSet<PathRef> = HashSet::new();
    let mut sources_of: HashMap<PathRef, Vec<(PathRef, PairHow)>> = HashMap::new();
    let mut dest_of: HashMap<PathRef, (PathRef, PairHow)> = HashMap::new();
    for (src, dst, how) in &pairs {
        let (Some(s), Some(d)) = (tracks.get(src), tracks.get(dst)) else {
            continue;
        };
        if !s.after().exists && d.after().exists {
            consumed.insert(src.clone());
        }
        sources_of
            .entry(dst.clone())
            .or_default()
            .push((src.clone(), *how));
        dest_of.insert(src.clone(), (dst.clone(), *how));
    }

    for (path, track) in &tracks {
        if consumed.contains(path) {
            continue;
        }
        if let Some(change) = classify_track(
            path,
            track,
            &tracks,
            &consumed,
            sources_of.get(path).map(Vec::as_slice).unwrap_or(&[]),
            dest_of.get(path),
            base,
            opts,
        ) {
            obs_out.files.push(change);
        }
    }
    obs_out
        .files
        .sort_by(|a, b| (a.pattern, &a.path).cmp(&(b.pattern, &b.path)));
    obs_out
}

fn add_to_batch(
    list: &mut Vec<Batch>,
    t: u64,
    tool: bool,
    kind: &str,
    prev: Option<&Stat>,
    stat: Option<&Stat>,
) {
    let joins = list
        .last()
        .is_some_and(|b| b.tool == tool && t.saturating_sub(b.t) <= BATCH_US);
    if !joins {
        list.push(Batch {
            t,
            tool,
            created: false,
            removed: false,
            renamed: false,
            pre: prev.cloned().unwrap_or_default(),
            post: Stat::default(),
        });
    }
    if let Some(b) = list.last_mut() {
        b.created |= kind.starts_with("create");
        b.removed |= kind.starts_with("remove");
        b.renamed |= kind.starts_with("modify.name");
        b.post = stat.cloned().unwrap_or_default();
    }
}

/// Decide which create/delete/rename flags count for a path.
///
/// Without sticky flags every reported flag counts. With them (macOS), a flag
/// reported in a notification where the file's identity did not change is
/// left over; once seen like that, later reports of the same flag in this
/// window are not trusted either.
fn count_flags(track: &mut Track, batches: &[Batch], sticky: bool) {
    let mut leftover = [false; 3];
    for b in batches {
        let flags = [b.created, b.removed, b.renamed];
        for (i, flag) in flags.iter().enumerate() {
            if !*flag {
                continue;
            }
            if sticky && b.identity_kept() {
                leftover[i] = true;
            } else if !b.tool && !(sticky && leftover[i]) {
                match i {
                    0 => track.created = true,
                    1 => track.removed = true,
                    _ => track.renamed = true,
                }
            }
        }
    }
    for (i, name) in ["create", "delete", "rename"].iter().enumerate() {
        if leftover[i] {
            track.leftover.push(name);
        }
    }
}

fn ignored(path: &PathRef, ignore: &[String]) -> bool {
    ignore.iter().any(|g| {
        crate::paths::path_matches(g, path)
            || path
                .rel
                .split('/')
                .any(|part| glob_match(g, part) && !g.contains('/'))
    })
}

/// Pair the two sides of renames.
fn find_pairs(
    tracks: &BTreeMap<PathRef, Track>,
    events: &[Ev<'_>],
    opts: &ClassifyOptions,
) -> Vec<(PathRef, PathRef, PairHow)> {
    let mut pairs: Vec<(PathRef, PathRef, PairHow)> = Vec::new();
    let add = |pairs: &mut Vec<(PathRef, PathRef, PairHow)>, s: &PathRef, d: &PathRef, how| {
        if s != d
            && tracks.contains_key(s)
            && tracks.contains_key(d)
            && !pairs.iter().any(|(ps, pd, _)| ps == s && pd == d)
        {
            pairs.push((s.clone(), d.clone(), how));
        }
    };

    // Explicit pairs from the OS.
    let mut by_cookie: HashMap<usize, PathRef> = HashMap::new();
    let mut pending_from: Option<(PathRef, u64)> = None;
    for Ev {
        kind,
        paths,
        tracker,
        t,
    } in events
    {
        match (*kind, paths.first(), paths.get(1)) {
            ("modify.name.both", Some(s), Some(d)) => add(&mut pairs, s, d, PairHow::Event),
            ("modify.name.from", Some(s), _) => match tracker {
                Some(c) => {
                    by_cookie.insert(*c, s.clone());
                }
                None => pending_from = Some((s.clone(), *t)),
            },
            ("modify.name.to", Some(d), _) => match tracker {
                Some(c) => {
                    if let Some(s) = by_cookie.get(c) {
                        add(&mut pairs, s, d, PairHow::Cookie);
                    }
                }
                None => {
                    if let Some((s, ts)) = pending_from.take()
                        && t.saturating_sub(ts) <= 1_000_000
                    {
                        add(&mut pairs, &s, d, PairHow::Adjacent);
                    }
                }
            },
            _ => {}
        }
    }

    // Inferred pairs for bare "renamed" flags (macOS).
    let explicit_src: HashSet<PathRef> = pairs.iter().map(|p| p.0.clone()).collect();
    let explicit_dst: HashSet<PathRef> = pairs.iter().map(|p| p.1.clone()).collect();
    let bare = |t: &Track| t.renamed && t.obs.iter().any(|o| o.kind == "modify.name.any");
    let sources: Vec<&PathRef> = tracks
        .iter()
        .filter(|(p, t)| bare(t) && !explicit_src.contains(*p))
        .filter(|(_, t)| {
            let (b, a) = (t.before(), t.after());
            !a.exists || (b.exists && b.inode.is_some() && a.inode != b.inode)
        })
        .map(|(p, _)| p)
        .collect();
    let dests: Vec<&PathRef> = tracks
        .iter()
        .filter(|(p, t)| bare(t) && !explicit_dst.contains(*p) && t.after().exists)
        .filter(|(_, t)| {
            let (b, a) = (t.before(), t.after());
            !b.exists || a.inode.is_none() || a.inode != b.inode
        })
        .map(|(p, _)| p)
        .collect();
    for src in sources {
        let Some(s) = tracks.get(src) else { continue };
        let s_inodes = s.inodes();
        let s_times = s.rename_times();
        let mut by_inode: Option<&PathRef> = None;
        let mut by_time: Option<(u64, bool, &PathRef)> = None;
        for dst in &dests {
            if *dst == src {
                continue;
            }
            let Some(d) = tracks.get(*dst) else { continue };
            if !s_inodes.is_disjoint(&d.new_inodes()) {
                by_inode = Some(dst);
                break;
            }
            let gap = s_times
                .iter()
                .flat_map(|a| d.rename_times().into_iter().map(move |b| a.abs_diff(b)))
                .min();
            if let Some(gap) = gap
                && gap <= opts.rename_window_us
            {
                let same_dir = src.root == dst.root && src.parent() == dst.parent();
                let better = match by_time {
                    None => true,
                    Some((g, sd, _)) => (same_dir && !sd) || (same_dir == sd && gap < g),
                };
                if better {
                    by_time = Some((gap, same_dir, dst));
                }
            }
        }
        if let Some(dst) = by_inode {
            add(&mut pairs, src, dst, PairHow::Inode);
        } else if let Some((_, _, dst)) = by_time {
            // Timing alone is only trusted when identities cannot contradict it.
            let d_new = tracks.get(dst).map(Track::new_inodes).unwrap_or_default();
            if s_inodes.is_empty() || d_new.is_empty() {
                add(&mut pairs, src, dst, PairHow::Timing);
            }
        }
    }
    pairs
}

#[allow(clippy::too_many_arguments)]
fn classify_track(
    path: &PathRef,
    track: &Track,
    tracks: &BTreeMap<PathRef, Track>,
    consumed: &HashSet<PathRef>,
    sources: &[(PathRef, PairHow)],
    dest: Option<&(PathRef, PairHow)>,
    base: u64,
    opts: &ClassifyOptions,
) -> Option<FileChange> {
    let before = track.before();
    let after = track.after();
    let dir = track.is_dir();
    let mut change = FileChange {
        path: path.clone(),
        dir,
        pattern: Pattern::Unchanged,
        existed_before: before.exists,
        exists_after: after.exists,
        temp: None,
        temp_pattern: None,
        temp_outside: false,
        renamed_from: None,
        moved_to: None,
        size_before: before.size.filter(|_| before.exists),
        size_after: after.size.filter(|_| after.exists),
        readonly_before: before.readonly.filter(|_| before.exists),
        readonly_after: after.readonly.filter(|_| after.exists),
        readonly_changed: false,
        settle_ms: None,
        times: 0,
        evidence: String::new(),
        sequence: sequence(track, base),
    };
    change.readonly_changed = matches!(
        (change.readonly_before, change.readonly_after),
        (Some(a), Some(b)) if a != b
    );

    let consumed_src: Vec<&(PathRef, PairHow)> = sources
        .iter()
        .filter(|(s, _)| consumed.contains(s))
        .collect();
    let kept_src: Vec<&(PathRef, PairHow)> = sources
        .iter()
        .filter(|(s, _)| !consumed.contains(s))
        .collect();
    let src_track = |p: &PathRef| tracks.get(p);

    let (pattern, evidence) = match (before.exists, after.exists) {
        (false, false) => {
            if dir {
                (Pattern::Transient, "folder appeared and disappeared".into())
            } else {
                (
                    Pattern::Transient,
                    "appeared and disappeared during the step".into(),
                )
            }
        }
        (false, true) => {
            if let Some((s, how)) = consumed_src.first() {
                let s_existed = src_track(s).is_some_and(|t| t.before().exists);
                if s_existed {
                    change.renamed_from = Some(s.clone());
                    (
                        Pattern::Renamed,
                        format!("renamed from another file ({})", how.as_str()),
                    )
                } else {
                    set_temp(&mut change, s);
                    (
                        Pattern::TempThenRename,
                        format!(
                            "new file written as a temporary file first ({})",
                            how.as_str()
                        ),
                    )
                }
            } else if let Some((s, how)) = kept_src.first() {
                change.renamed_from = Some(s.clone());
                (
                    Pattern::NewFile,
                    format!(
                        "holds the previous version of another file ({})",
                        how.as_str()
                    ),
                )
            } else {
                (Pattern::NewFile, "did not exist before the step".into())
            }
        }
        (true, false) => {
            if let Some((d, how)) = dest {
                change.moved_to = Some(d.clone());
                if tracks.get(d).is_some_and(|t| t.before().exists) {
                    // Renamed over an existing file: reported on the target.
                    return None;
                }
                (Pattern::Removed, format!("renamed away ({})", how.as_str()))
            } else {
                (Pattern::Removed, "gone at the end of the step".into())
            }
        }
        (true, true) if dir => {
            if let Some((d, _)) = dest {
                change.moved_to = Some(d.clone());
                (
                    Pattern::MovedAsideThenCreate,
                    "folder renamed away and created again".into(),
                )
            } else {
                return None;
            }
        }
        (true, true) => {
            if let Some((s, how)) = consumed_src.first() {
                set_temp(&mut change, s);
                if let Some((d, _)) = dest {
                    change.moved_to = Some(d.clone());
                }
                (
                    Pattern::TempThenRename,
                    format!("another file was renamed onto it ({})", how.as_str()),
                )
            } else if let Some((d, how)) = dest {
                change.moved_to = Some(d.clone());
                (
                    Pattern::MovedAsideThenCreate,
                    format!(
                        "old file renamed away ({}), then a new one written",
                        how.as_str()
                    ),
                )
            } else {
                unpaired_existing(track, &before, &after, &mut change)
            }
        }
    };
    change.pattern = pattern;
    change.evidence = if track.leftover.is_empty() {
        evidence
    } else {
        format!(
            "{evidence}; ignored left-over {} flag(s) from earlier changes",
            track.leftover.join("/")
        )
    };

    if matches!(
        pattern,
        Pattern::TempThenRename
            | Pattern::InPlaceWrite
            | Pattern::DeleteThenCreate
            | Pattern::MovedAsideThenCreate
            | Pattern::NewFile
            | Pattern::Renamed
            | Pattern::Unclassified
    ) && !dir
    {
        let mut involved: Vec<&Track> = vec![track];
        for (s, _) in &consumed_src {
            if let Some(t) = tracks.get(s) {
                involved.push(t);
            }
        }
        let (settle, times) = settle(&involved, opts.burst_gap_us);
        change.settle_ms = settle;
        change.times = times;
    }
    Some(change)
}

/// An existing file that still exists, with no rename partner found.
fn unpaired_existing(
    track: &Track,
    before: &Stat,
    after: &Stat,
    change: &mut FileChange,
) -> (Pattern, String) {
    let inode_changed = matches!((before.inode, after.inode), (Some(a), Some(b)) if a != b);
    let missing_midway = track.obs.iter().any(|o| !o.stat.exists);
    let content_changed = track.obs.iter().any(|o| {
        o.stat.exists && (o.stat.size != before.size || o.stat.mtime_ns != before.mtime_ns)
    });
    let attrs_changed = before.readonly != after.readonly || before.mode != after.mode;
    if inode_changed || missing_midway {
        let why = if inode_changed {
            "file identity (inode) changed"
        } else {
            "briefly missing"
        };
        match (track.removed, track.renamed) {
            (true, false) => (Pattern::DeleteThenCreate, format!("{why}; delete reported")),
            (true, true) if track.created => (
                Pattern::DeleteThenCreate,
                format!(
                    "{why}; delete and create reported (the rename flag is probably left over)"
                ),
            ),
            (false, true) => {
                change.temp_outside = true;
                change.temp_pattern = Some("outside the watched folders".into());
                (
                    Pattern::TempThenRename,
                    format!("{why}; renamed onto it from outside the watched folders"),
                )
            }
            (false, false) if track.created => (
                Pattern::DeleteThenCreate,
                format!("{why}; create reported without a delete"),
            ),
            (false, false) => (
                Pattern::Unclassified,
                format!("{why}, but neither a delete nor a rename was reported"),
            ),
            (true, true) => (
                Pattern::Unclassified,
                format!("{why}; both a delete and a rename were reported"),
            ),
        }
    } else if track.removed {
        (
            Pattern::DeleteThenCreate,
            "delete reported, then the file was there again".into(),
        )
    } else if track.renamed_to && before.inode.is_none() {
        change.temp_outside = true;
        change.temp_pattern = Some("outside the watched folders".into());
        (
            Pattern::TempThenRename,
            "renamed onto it from outside the watched folders".into(),
        )
    } else if content_changed {
        let how = if before.inode.is_some() {
            "same file identity (inode), new size or modification time"
        } else {
            "new size or modification time, no delete or rename reported"
        };
        (Pattern::InPlaceWrite, how.into())
    } else if attrs_changed {
        (Pattern::MetadataOnly, "only attributes changed".into())
    } else if track.data_event {
        (
            Pattern::Unchanged,
            "a write was reported but size and modification time are the same".into(),
        )
    } else {
        (Pattern::Unchanged, "no visible change".into())
    }
}

fn set_temp(change: &mut FileChange, temp: &PathRef) {
    change.temp_pattern = Some(temp_pattern(temp, &change.path));
    change.temp = Some(temp.clone());
}

/// Describe a temporary file's name relative to its target:
/// `Song A.flp.tmp` for `Song A.flp` → `{name}.tmp`.
pub fn temp_pattern(temp: &PathRef, target: &PathRef) -> String {
    let name = target.file_name();
    let stem = match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    };
    let t = temp.file_name();
    let pattern = if !name.is_empty() && t.contains(name) {
        t.replacen(name, "{name}", 1)
    } else if !stem.is_empty() && t.contains(stem) {
        t.replacen(stem, "{stem}", 1)
    } else {
        t.to_string()
    };
    if temp.root == target.root && temp.parent() == target.parent() {
        pattern
    } else if temp.parent().is_empty() {
        format!("(top folder)/{pattern}")
    } else {
        format!("{}/{pattern}", temp.parent())
    }
}

/// Longest burst from first activity to the last size/mtime change, and the
/// number of bursts that changed something.
fn settle(tracks: &[&Track], gap_us: u64) -> (Option<u64>, u32) {
    // (time, changed?) for every observation of every involved path.
    let mut points: Vec<(u64, bool)> = Vec::new();
    for track in tracks {
        let mut last = track.before();
        for o in &track.obs {
            let changed = o.stat.exists != last.exists
                || o.stat.size != last.size
                || o.stat.mtime_ns != last.mtime_ns
                || o.stat.inode != last.inode;
            points.push((o.t, changed));
            last = o.stat.clone();
        }
    }
    points.sort_unstable();
    let mut best: Option<u64> = None;
    let mut bursts = 0u32;
    let mut start: Option<u64> = None;
    let mut last_t = 0u64;
    let mut last_change: Option<u64> = None;
    let mut close = |start: Option<u64>, last_change: Option<u64>, best: &mut Option<u64>| {
        if let (Some(s), Some(c)) = (start, last_change) {
            bursts += 1;
            let ms = c.saturating_sub(s) / 1000;
            *best = Some(best.map_or(ms, |b| b.max(ms)));
        }
    };
    for (t, changed) in points {
        if start.is_some() && t.saturating_sub(last_t) > gap_us {
            close(start, last_change, &mut best);
            start = None;
            last_change = None;
        }
        if start.is_none() {
            start = Some(t);
        }
        if changed {
            last_change = Some(t);
        }
        last_t = t;
    }
    close(start, last_change, &mut best);
    (best, bursts)
}

fn sequence(track: &Track, base: u64) -> Vec<String> {
    let mut out: Vec<String> = track
        .obs
        .iter()
        .take(MAX_SEQUENCE)
        .map(|o| {
            let ms = o.t.saturating_sub(base) / 1000;
            format!("+{ms}ms #{} {} {}", o.idx, o.kind, describe(&o.stat))
        })
        .collect();
    if track.obs.len() > MAX_SEQUENCE {
        out.push(format!("... {} more", track.obs.len() - MAX_SEQUENCE));
    }
    out
}

fn describe(s: &Stat) -> String {
    if !s.exists {
        return "missing".into();
    }
    let mut parts = Vec::new();
    if s.dir {
        parts.push("folder".to_string());
    }
    if let Some(size) = s.size {
        parts.push(format!("size={size}"));
    }
    if let Some(ino) = s.inode {
        parts.push(format!("ino={ino}"));
    }
    if s.readonly == Some(true) {
        parts.push("read-only".into());
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::Record;

    /// Builds synthetic windows.
    struct W {
        recs: Vec<Record>,
        t: u64,
    }

    fn p(rel: &str) -> PathRef {
        PathRef {
            root: Some(0),
            rel: rel.into(),
        }
    }

    fn file(size: u64, mtime: u64, ino: Option<u64>) -> Stat {
        Stat {
            exists: true,
            size: Some(size),
            mtime_ns: Some(mtime),
            readonly: Some(false),
            inode: ino,
            ..Stat::default()
        }
    }

    impl W {
        fn new() -> Self {
            Self {
                recs: Vec::new(),
                t: 1_000,
            }
        }

        fn push(&mut self, body: Body, dt_ms: u64) {
            self.t += dt_ms * 1000;
            self.recs.push(Record {
                seq: self.recs.len() as u64,
                run: 1,
                t_us: self.t,
                wall: String::new(),
                body,
            });
        }

        fn ev(&mut self, kind: &str, rel: &str, prev: Stat, stat: Stat) -> &mut Self {
            self.ev_with(kind, rel, prev, stat, None);
            self
        }

        fn ev_with(
            &mut self,
            kind: &str,
            rel: &str,
            prev: Stat,
            stat: Stat,
            tracker: Option<usize>,
        ) {
            self.push(
                Body::Event {
                    kind: kind.into(),
                    paths: vec![p(rel)],
                    stat: vec![stat],
                    prev: vec![prev],
                    tracker,
                    info: None,
                    rescan: false,
                    tool: None,
                },
                10,
            );
        }

        fn poll(&mut self, rel: &str, prev: Stat, stat: Stat, dt_ms: u64) -> &mut Self {
            self.push(
                Body::Poll {
                    path: p(rel),
                    stat,
                    prev,
                    tool: None,
                },
                dt_ms,
            );
            self
        }

        /// An event `dt_ms` after the previous one (0 = same notification).
        fn raw(
            &mut self,
            kind: &str,
            rel: &str,
            prev: Stat,
            stat: Stat,
            dt_ms: u64,
            tool: Option<&str>,
        ) {
            self.push(
                Body::Event {
                    kind: kind.into(),
                    paths: vec![p(rel)],
                    stat: vec![stat],
                    prev: vec![prev],
                    tracker: None,
                    info: None,
                    rescan: false,
                    tool: tool.map(str::to_string),
                },
                dt_ms,
            );
        }

        fn run(&self) -> Observation {
            classify(&self.recs, &ClassifyOptions::default())
        }

        fn run_macos(&self) -> Observation {
            classify(
                &self.recs,
                &ClassifyOptions::default().for_os(Some("macos")),
            )
        }
    }

    #[test]
    fn macos_leftover_rename_flag_does_not_hide_delete_then_create() {
        // Recorded on macOS: the tool's chmod re-reported an old rename flag
        // (same inode), then delete + create arrived as one notification that
        // also claimed "renamed".
        let mut ro = file(2000, 1, Some(610));
        ro.readonly = Some(true);
        let new = file(1500, 2, Some(667));
        let mut w = W::new();
        w.raw(
            "modify.name.any",
            "Song.prj",
            ro.clone(),
            ro.clone(),
            0,
            Some("set_readonly"),
        );
        w.raw(
            "modify.metadata.ownership",
            "Song.prj",
            ro.clone(),
            ro.clone(),
            0,
            Some("set_readonly"),
        );
        w.raw("remove.file", "Song.prj", ro, new.clone(), 2000, None);
        w.raw(
            "modify.name.any",
            "Song.prj",
            new.clone(),
            new.clone(),
            0,
            None,
        );
        w.raw("create.file", "Song.prj", new.clone(), new.clone(), 0, None);
        w.raw("modify.data.content", "Song.prj", new.clone(), new, 0, None);
        let o = w.run_macos();
        let c = only(&o);
        assert_eq!(c.pattern, Pattern::DeleteThenCreate, "{c:#?}");
        assert!(c.evidence.contains("left-over rename"), "{}", c.evidence);
        assert!(c.readonly_changed);
    }

    #[test]
    fn macos_rename_over_target_with_leftover_create_flag() {
        // One notification: a create flag left over from history plus the rename.
        let mut w = W::new();
        w.raw(
            "create.file",
            "Song.flp",
            file(10, 1, Some(5)),
            file(12, 2, Some(9)),
            10,
            None,
        );
        w.raw(
            "modify.name.any",
            "Song.flp",
            file(12, 2, Some(9)),
            file(12, 2, Some(9)),
            0,
            None,
        );
        let o = w.run_macos();
        let c = only(&o);
        assert_eq!(c.pattern, Pattern::TempThenRename, "{c:#?}");
        assert!(c.temp_outside);
    }

    #[test]
    fn macos_in_place_write_with_leftover_flags_stays_in_place() {
        let mut w = W::new();
        let a = file(10, 1, Some(5));
        let b = file(20, 2, Some(5));
        w.raw("create.file", "Song.flp", a.clone(), b.clone(), 10, None);
        w.raw("modify.name.any", "Song.flp", b.clone(), b.clone(), 0, None);
        w.raw("modify.data.content", "Song.flp", b.clone(), b, 0, None);
        let o = w.run_macos();
        let c = only(&o);
        assert_eq!(c.pattern, Pattern::InPlaceWrite, "{c:#?}");
    }

    fn only(o: &Observation) -> &FileChange {
        assert_eq!(
            o.files.len(),
            1,
            "expected exactly one change: {:#?}",
            o.files
        );
        &o.files[0]
    }

    #[test]
    fn in_place_write_keeps_identity() {
        let mut w = W::new();
        w.ev(
            "modify.data.content",
            "Song.flp",
            file(10, 1, Some(5)),
            file(20, 2, Some(5)),
        );
        let o = w.run();
        let c = only(&o);
        assert_eq!(c.pattern, Pattern::InPlaceWrite);
        assert_eq!(c.size_before, Some(10));
        assert_eq!(c.size_after, Some(20));
        assert_eq!(c.times, 1);
    }

    #[test]
    fn in_place_write_without_inodes_windows_style() {
        let mut w = W::new();
        w.ev(
            "modify.any",
            "Song.flp",
            file(10, 1, None),
            file(10, 2, None),
        );
        assert_eq!(only(&w.run()).pattern, Pattern::InPlaceWrite);
    }

    #[test]
    fn temp_then_rename_macos_style_inode_match() {
        // FSEvents: bare "renamed" flags on both sides, temp seen with its inode.
        let mut w = W::new();
        let old = file(10, 1, Some(5));
        let temp = file(12, 3, Some(9));
        w.ev("create.file", "Song.flp.tmp", Stat::missing(), temp.clone())
            .ev(
                "modify.name.any",
                "Song.flp.tmp",
                temp.clone(),
                Stat::missing(),
            )
            .ev("modify.name.any", "Song.flp", old, temp);
        let o = w.run();
        let c = only(&o);
        assert_eq!(c.pattern, Pattern::TempThenRename);
        assert_eq!(
            c.temp.as_ref().map(|t| t.rel.as_str()),
            Some("Song.flp.tmp")
        );
        assert_eq!(c.temp_pattern.as_deref(), Some("{name}.tmp"));
        assert!(c.evidence.contains("inode"), "{}", c.evidence);
    }

    #[test]
    fn temp_then_rename_macos_style_by_timing_when_temp_was_never_seen() {
        // The temp was already gone when its event arrived: no inode for it.
        let mut w = W::new();
        let old = file(10, 1, Some(5));
        w.ev("create.file", "~Song.tmp", Stat::missing(), Stat::missing())
            .ev(
                "modify.name.any",
                "~Song.tmp",
                Stat::missing(),
                Stat::missing(),
            )
            .ev("modify.name.any", "Song.flp", old, file(12, 3, Some(9)));
        let o = w.run();
        let c = only(&o);
        assert_eq!(c.pattern, Pattern::TempThenRename);
        assert_eq!(c.temp_pattern.as_deref(), Some("~{stem}.tmp"));
    }

    #[test]
    fn temp_then_rename_linux_style_cookie() {
        let mut w = W::new();
        let old = file(10, 1, Some(5));
        let temp = file(12, 3, Some(9));
        w.ev(
            "create.file",
            "a/.Song.flp.x1",
            Stat::missing(),
            temp.clone(),
        );
        w.ev_with(
            "modify.name.from",
            "a/.Song.flp.x1",
            temp.clone(),
            Stat::missing(),
            Some(77),
        );
        w.ev_with("modify.name.to", "a/Song.flp", old, temp, Some(77));
        let c = w.run();
        let c = only(&c);
        assert_eq!(c.pattern, Pattern::TempThenRename);
        assert_eq!(c.temp_pattern.as_deref(), Some(".{name}.x1"));
        assert!(c.evidence.contains("cookie"));
    }

    #[test]
    fn temp_then_rename_windows_style_adjacent() {
        let mut w = W::new();
        let old = file(10, 1, None);
        let temp = file(12, 3, None);
        w.ev("create.any", "Song.flp.tmp", Stat::missing(), temp.clone())
            .ev("modify.any", "Song.flp.tmp", temp.clone(), temp.clone())
            .ev("remove.any", "Song.flp", old.clone(), temp.clone())
            .ev(
                "modify.name.from",
                "Song.flp.tmp",
                temp.clone(),
                Stat::missing(),
            )
            .ev("modify.name.to", "Song.flp", old, temp);
        let o = w.run();
        let c = only(&o);
        assert_eq!(c.pattern, Pattern::TempThenRename);
        assert!(c.evidence.contains("back to back"));
    }

    #[test]
    fn rename_onto_target_from_outside_watched_folders() {
        let mut w = W::new();
        w.ev(
            "modify.name.any",
            "Song.flp",
            file(10, 1, Some(5)),
            file(12, 3, Some(9)),
        );
        let c = w.run();
        let c = only(&c);
        assert_eq!(c.pattern, Pattern::TempThenRename);
        assert!(c.temp_outside);
    }

    #[test]
    fn delete_then_create_with_new_inode() {
        let mut w = W::new();
        w.ev(
            "remove.file",
            "Song.flp",
            file(10, 1, Some(5)),
            Stat::missing(),
        )
        .ev(
            "create.file",
            "Song.flp",
            Stat::missing(),
            file(11, 2, Some(6)),
        );
        assert_eq!(only(&w.run()).pattern, Pattern::DeleteThenCreate);
    }

    #[test]
    fn delete_then_create_with_reused_inode_linux_style() {
        let mut w = W::new();
        w.ev(
            "remove.file",
            "Song.flp",
            file(10, 1, Some(5)),
            file(11, 2, Some(5)),
        )
        .ev(
            "create.file",
            "Song.flp",
            file(11, 2, Some(5)),
            file(11, 2, Some(5)),
        );
        assert_eq!(only(&w.run()).pattern, Pattern::DeleteThenCreate);
    }

    #[test]
    fn delete_then_create_merged_into_one_macos_event() {
        // FSEvents merged remove+create; we only see the new identity.
        let mut w = W::new();
        w.ev(
            "create.file",
            "Song.flp",
            file(10, 1, Some(5)),
            file(11, 2, Some(6)),
        )
        .ev(
            "remove.file",
            "Song.flp",
            file(11, 2, Some(6)),
            file(11, 2, Some(6)),
        );
        assert_eq!(only(&w.run()).pattern, Pattern::DeleteThenCreate);
    }

    #[test]
    fn moved_aside_then_create() {
        let mut w = W::new();
        let old = file(10, 1, Some(5));
        w.ev("modify.name.any", "Song.flp", old.clone(), Stat::missing())
            .ev(
                "modify.name.any",
                "Backup/Song (old).flp",
                Stat::missing(),
                old,
            )
            .ev(
                "create.file",
                "Song.flp",
                Stat::missing(),
                file(12, 3, Some(9)),
            );
        let o = w.run();
        assert_eq!(o.files.len(), 2, "{:#?}", o.files);
        let main = o.files.iter().find(|f| f.path.rel == "Song.flp").unwrap();
        assert_eq!(main.pattern, Pattern::MovedAsideThenCreate);
        assert_eq!(
            main.moved_to.as_ref().map(|m| m.rel.as_str()),
            Some("Backup/Song (old).flp")
        );
        let backup = o
            .files
            .iter()
            .find(|f| f.path.rel.starts_with("Backup"))
            .unwrap();
        assert_eq!(backup.pattern, Pattern::NewFile);
        assert_eq!(
            backup.renamed_from.as_ref().map(|m| m.rel.as_str()),
            Some("Song.flp")
        );
    }

    #[test]
    fn new_file_and_removed_file() {
        let mut w = W::new();
        w.ev(
            "create.file",
            "Song_2.flp",
            Stat::missing(),
            file(5, 1, Some(3)),
        )
        .ev(
            "remove.file",
            "old.wav",
            file(9, 1, Some(4)),
            Stat::missing(),
        );
        let o = w.run();
        let new = o.files.iter().find(|f| f.path.rel == "Song_2.flp").unwrap();
        assert_eq!(new.pattern, Pattern::NewFile);
        let gone = o.files.iter().find(|f| f.path.rel == "old.wav").unwrap();
        assert_eq!(gone.pattern, Pattern::Removed);
    }

    #[test]
    fn plain_rename_is_one_change() {
        let mut w = W::new();
        let f = file(9, 1, Some(4));
        w.ev("modify.name.any", "a.wav", f.clone(), Stat::missing())
            .ev("modify.name.any", "b.wav", Stat::missing(), f);
        let o = w.run();
        let c = only(&o);
        assert_eq!(c.pattern, Pattern::Renamed);
        assert_eq!(c.path.rel, "b.wav");
        assert_eq!(
            c.renamed_from.as_ref().map(|r| r.rel.as_str()),
            Some("a.wav")
        );
    }

    #[test]
    fn transient_lock_file() {
        let mut w = W::new();
        w.ev(
            "create.file",
            "Song.flp.lock",
            Stat::missing(),
            file(0, 1, Some(3)),
        )
        .ev(
            "remove.file",
            "Song.flp.lock",
            file(0, 1, Some(3)),
            Stat::missing(),
        );
        assert_eq!(only(&w.run()).pattern, Pattern::Transient);
    }

    #[test]
    fn readonly_toggle_is_metadata_only() {
        let mut w = W::new();
        let mut ro = file(10, 1, Some(5));
        ro.readonly = Some(true);
        w.ev("modify.metadata.any", "Song.flp", file(10, 1, Some(5)), ro);
        let o = w.run();
        let c = only(&o);
        assert_eq!(c.pattern, Pattern::MetadataOnly);
        assert!(c.readonly_changed);
        assert_eq!(c.readonly_after, Some(true));
    }

    #[test]
    fn readonly_flag_lost_by_save_is_reported() {
        let mut ro = file(10, 1, Some(5));
        ro.readonly = Some(true);
        let mut w = W::new();
        w.ev("remove.file", "Song.flp", ro, Stat::missing()).ev(
            "create.file",
            "Song.flp",
            Stat::missing(),
            file(11, 2, Some(6)),
        );
        let o = w.run();
        let c = only(&o);
        assert_eq!(c.pattern, Pattern::DeleteThenCreate);
        assert!(c.readonly_changed);
        assert_eq!(c.readonly_before, Some(true));
        assert_eq!(c.readonly_after, Some(false));
    }

    #[test]
    fn slow_write_has_settle_time() {
        let mut w = W::new();
        let a = file(0, 1, Some(5));
        let b = file(100, 2, Some(5));
        let c = file(200, 3, Some(5));
        let d = file(300, 4, Some(5));
        w.ev("modify.data.content", "mix.wav", a, b.clone())
            .poll("mix.wav", b, c.clone(), 400)
            .poll("mix.wav", c, d, 400);
        let o = w.run();
        let ch = only(&o);
        assert_eq!(ch.pattern, Pattern::InPlaceWrite);
        assert_eq!(ch.settle_ms, Some(800));
    }

    #[test]
    fn separate_saves_count_as_separate_times() {
        let mut w = W::new();
        w.ev(
            "modify.data.content",
            "s.flp",
            file(1, 1, Some(5)),
            file(2, 2, Some(5)),
        );
        w.push(
            Body::Event {
                kind: "modify.data.content".into(),
                paths: vec![p("s.flp")],
                stat: vec![file(3, 3, Some(5))],
                prev: vec![file(2, 2, Some(5))],
                tracker: None,
                info: None,
                rescan: false,
                tool: None,
            },
            10_000,
        );
        let o = w.run();
        let c = only(&o);
        assert_eq!(c.times, 2);
        assert_eq!(c.settle_ms, Some(0));
    }

    #[test]
    fn tool_events_and_ignored_names_are_left_out() {
        let mut w = W::new();
        w.push(
            Body::Event {
                kind: "modify.metadata.any".into(),
                paths: vec![p("Song.flp")],
                stat: vec![file(1, 1, Some(1))],
                prev: vec![file(1, 1, Some(1))],
                tracker: None,
                info: None,
                rescan: false,
                tool: Some("set_readonly".into()),
            },
            1,
        );
        w.ev(
            "create.file",
            ".DS_Store",
            Stat::missing(),
            file(1, 1, Some(2)),
        );
        let opts = ClassifyOptions {
            ignore: vec![".DS_Store".into()],
            ..ClassifyOptions::default()
        };
        let o = classify(&w.recs, &opts);
        assert!(o.files.is_empty(), "{:#?}", o.files);
        assert_eq!(o.tool_events, 1);
        assert_eq!(o.events, 1);
    }

    #[test]
    fn unknown_replacement_is_unclassified_with_raw_sequence() {
        let mut w = W::new();
        w.ev(
            "modify.metadata.any",
            "Song.flp",
            file(10, 1, Some(5)),
            file(10, 2, Some(6)),
        );
        let o = w.run();
        let c = only(&o);
        assert_eq!(c.pattern, Pattern::Unclassified);
        assert_eq!(c.sequence.len(), 1);
        assert!(c.sequence[0].contains("modify.metadata.any"));
    }

    #[test]
    fn rescans_are_flagged() {
        let mut w = W::new();
        w.push(
            Body::Event {
                kind: "other".into(),
                paths: vec![],
                stat: vec![],
                prev: vec![],
                tracker: None,
                info: Some("rescan: kernel dropped".into()),
                rescan: true,
                tool: None,
            },
            1,
        );
        assert!(w.run().missed_events);
    }

    #[test]
    fn temp_pattern_shapes() {
        assert_eq!(
            temp_pattern(&p("x/Song A.flp.tmp"), &p("x/Song A.flp")),
            "{name}.tmp"
        );
        assert_eq!(
            temp_pattern(&p("x/Song A.tmp"), &p("x/Song A.flp")),
            "{stem}.tmp"
        );
        assert_eq!(
            temp_pattern(&p("x/a1b2.tmp"), &p("x/Song A.flp")),
            "a1b2.tmp"
        );
        assert_eq!(
            temp_pattern(&p("t/Song A.flp~"), &p("x/Song A.flp")),
            "t/{name}~"
        );
    }
}
