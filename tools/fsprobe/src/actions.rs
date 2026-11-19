//! Helper actions a protocol step can run, so the user never needs a second
//! terminal: read-only flags, open-file listing, tree snapshots, safety
//! copies and restores, waiting, copying a stand-in file in.
//!
//! Failures are reported as a plain-language [`Outcome`] with `ok: false`;
//! only pausing or the end of input stops the session.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde_json::json;

use crate::console::{Console, say};
use crate::error::ProbeError;
use crate::paths::{
    WatchRoot, clean_typed_path, expand_tilde, glob_match, human_size, path_matches, to_ref,
};
use crate::protocol::{ActionKind, ActionSpec, Protocol, RestoreMethod};
use crate::recorder::{Recorder, scan};
use crate::session::{BackupInfo, SessionState};
use crate::trace::{Stat, wall_now};

/// What an action did.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// It worked.
    pub ok: bool,
    /// Plain-language result shown to the user.
    pub message: String,
    /// Structured details for the trace.
    pub detail: serde_json::Value,
}

impl Outcome {
    fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
            detail: serde_json::Value::Null,
        }
    }

    fn fail(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
            detail: serde_json::Value::Null,
        }
    }

    fn with(mut self, detail: serde_json::Value) -> Self {
        self.detail = detail;
        self
    }
}

/// Everything an action may touch.
pub struct Ctx<'a> {
    /// The protocol (for named targets).
    pub protocol: &'a Protocol,
    /// Watched folders.
    pub roots: &'a [WatchRoot],
    /// Protocol watch id of each root (same order), if known.
    pub watch_ids: &'a [Option<String>],
    /// The recorder (for tagging the tool's own changes).
    pub recorder: &'a Recorder,
    /// Input, for picking among several matching files.
    pub console: &'a mut Console,
    /// Session state (remembered targets, backups).
    pub state: &'a mut SessionState,
    /// Results folder.
    pub out_dir: &'a Path,
    /// Where safety copies go (outside the results folder).
    pub backups_dir: &'a Path,
    /// Current step id.
    pub step_id: &'a str,
    /// How long the tool waits for its own changes to show up as events.
    pub settle: Duration,
}

/// Run one action.
pub fn run(ctx: &mut Ctx<'_>, spec: &ActionSpec) -> Result<Outcome, ProbeError> {
    let tagged = spec.action.modifies_files() || spec.action.reads_files();
    let file = if needs_target(spec.action) {
        match resolve(ctx, spec)? {
            Some(p) => Some(p),
            None => {
                return Ok(Outcome::fail(format!(
                    "Skipped \"{}\": no file was chosen.",
                    spec.action.as_str()
                )));
            }
        }
    } else {
        None
    };
    if tagged {
        ctx.recorder.set_tool(Some(spec.action.as_str()));
    }
    let outcome = perform(ctx, spec, file.as_deref());
    if tagged {
        // Let the OS report our own change before untagging.
        ctx.recorder.wait_quiet(ctx.settle, Duration::from_secs(5));
        ctx.recorder.set_tool(None);
    }
    outcome
}

fn needs_target(kind: ActionKind) -> bool {
    matches!(
        kind,
        ActionKind::SetReadonly
            | ActionKind::ClearReadonly
            | ActionKind::BackupFile
            | ActionKind::StatFile
            | ActionKind::CopyIn
    )
}

fn perform(
    ctx: &mut Ctx<'_>,
    spec: &ActionSpec,
    file: Option<&Path>,
) -> Result<Outcome, ProbeError> {
    let slot = spec.slot.as_deref().unwrap_or("default");
    Ok(match (spec.action, file) {
        (ActionKind::SetReadonly, Some(f)) => set_readonly(ctx, f, true),
        (ActionKind::ClearReadonly, Some(f)) => set_readonly(ctx, f, false),
        (ActionKind::ListOpenHandles, _) => list_open_handles(ctx.roots),
        (ActionKind::SnapshotTree, _) => snapshot_tree(ctx),
        (ActionKind::BackupFile, Some(f)) => backup_file(ctx, f, slot),
        (ActionKind::RestoreFile, _) => restore_file(ctx, slot, spec.method.unwrap_or_default()),
        (ActionKind::CompareWithBackup, _) => compare_with_backup(ctx, slot),
        (ActionKind::StatFile, Some(f)) => stat_file(ctx, f),
        (ActionKind::Wait, _) => wait(ctx, spec.seconds.unwrap_or(0), spec.message.as_deref())?,
        (ActionKind::CopyIn, Some(f)) => copy_in(ctx, f, spec),
        (kind, None) => Outcome::fail(format!("Skipped \"{}\": no file.", kind.as_str())),
    })
}

fn display(ctx: &Ctx<'_>, path: &Path) -> String {
    let r = to_ref(path, ctx.roots);
    match r.root {
        Some(0) => r.rel,
        Some(i) => format!(
            "{}: {}",
            ctx.roots.get(i).map(|x| x.label.as_str()).unwrap_or("?"),
            r.rel
        ),
        None => path.display().to_string(),
    }
}

// ---------------------------------------------------------------- targets

/// Resolve the file an action works on, asking the user if needed.
fn resolve(ctx: &mut Ctx<'_>, spec: &ActionSpec) -> Result<Option<PathBuf>, ProbeError> {
    if let Some(name) = &spec.target {
        let Some(t) = ctx.protocol.targets.get(name).cloned() else {
            return Ok(None);
        };
        if let Some(saved) = ctx.state.targets.get(name)
            && Path::new(saved).is_file()
        {
            return Ok(Some(PathBuf::from(saved)));
        }
        let candidates = if let Some(pattern) = &t.pattern {
            let root = t.root.as_deref();
            find_in_roots(ctx, pattern, root)
        } else {
            find_outside(&t.search)
        };
        let chosen = pick(ctx, &t.label, candidates, t.prefer.as_deref())?;
        if let Some(c) = &chosen {
            ctx.state
                .targets
                .insert(name.clone(), c.to_string_lossy().to_string());
        }
        Ok(chosen)
    } else if let Some(pattern) = &spec.pattern {
        let candidates = find_in_roots(ctx, pattern, None);
        pick(ctx, &format!("a file matching {pattern}"), candidates, None)
    } else {
        Ok(None)
    }
}

/// Files under the roots matching `pattern`, newest first.
fn find_in_roots(
    ctx: &Ctx<'_>,
    pattern: &str,
    root_id: Option<&str>,
) -> Vec<(PathBuf, Option<u64>)> {
    let mut found = Vec::new();
    for (i, root) in ctx.roots.iter().enumerate() {
        if let Some(id) = root_id
            && ctx.watch_ids.get(i).and_then(Option::as_deref) != Some(id)
        {
            continue;
        }
        let mut index = HashMap::new();
        scan(&root.path, &mut index, 200_000);
        for (path, stat) in index {
            if stat.dir {
                continue;
            }
            let r = to_ref(&path, ctx.roots);
            if r.root == Some(i) && path_matches(pattern, &r) {
                found.push((path, stat.mtime_ns));
            }
        }
    }
    found.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    found
}

/// Expand absolute globs (one `*`/`?` level per path component).
fn find_outside(patterns: &[String]) -> Vec<(PathBuf, Option<u64>)> {
    let mut found = Vec::new();
    for pattern in patterns {
        let expanded = expand_tilde(pattern).to_string_lossy().replace('\\', "/");
        let parts: Vec<&str> = expanded.split('/').collect();
        let first_glob = parts
            .iter()
            .position(|p| p.contains(['*', '?']))
            .unwrap_or(parts.len());
        let base = parts
            .get(..first_glob)
            .map(|b| b.join("/"))
            .unwrap_or_default();
        let base = if base.is_empty() {
            "/".to_string()
        } else {
            base
        };
        let rest = parts.get(first_glob..).unwrap_or(&[]);
        expand(Path::new(&base), rest, &mut found);
    }
    found.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    found.dedup_by(|a, b| a.0 == b.0);
    found
}

fn expand(base: &Path, rest: &[&str], found: &mut Vec<(PathBuf, Option<u64>)>) {
    let Some((head, tail)) = rest.split_first() else {
        let stat = Stat::of(base);
        if stat.exists && !stat.dir {
            found.push((base.to_path_buf(), stat.mtime_ns));
        }
        return;
    };
    if !head.contains(['*', '?']) {
        expand(&base.join(head), tail, found);
        return;
    }
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if glob_match(head, &name) {
            expand(&entry.path(), tail, found);
        }
    }
}

fn time_of(mtime_ns: Option<u64>) -> String {
    mtime_ns
        .map(|ns| {
            let t = UNIX_EPOCH + Duration::from_nanos(ns);
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.format("%Y-%m-%d %H:%M:%S").to_string()
        })
        .unwrap_or_else(|| "unknown time".into())
}

/// Choose one file; asks when there are several (Enter = the suggested one).
fn pick(
    ctx: &mut Ctx<'_>,
    label: &str,
    candidates: Vec<(PathBuf, Option<u64>)>,
    prefer: Option<&str>,
) -> Result<Option<PathBuf>, ProbeError> {
    match candidates.len() {
        0 => {
            say(&format!("I could not find {label}."));
            for _ in 0..3 {
                let input = ctx.console.ask(
                    "Type the file's location, or drag the file into this window, then press Enter.\n(Or type skip to carry on without it.)",
                )?;
                if is_skip(&input) || input.trim().is_empty() {
                    return Ok(None);
                }
                let path = clean_typed_path(&input);
                if path.is_file() {
                    return Ok(Some(path));
                }
                say("That file does not exist. Please check and try again.");
            }
            Ok(None)
        }
        1 => {
            let path = candidates.into_iter().next().map(|c| c.0);
            if let Some(p) = &path {
                say(&format!("Using {}.", display(ctx, p)));
            }
            Ok(path)
        }
        n => {
            let shown = n.min(15);
            let default = prefer
                .and_then(|name| {
                    candidates.iter().take(shown).position(|(p, _)| {
                        p.file_name()
                            .is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(name))
                    })
                })
                .unwrap_or(0);
            say(&format!("Which file is {label}?"));
            for (i, (p, m)) in candidates.iter().take(shown).enumerate() {
                let mark = if i == default { "   <- suggested" } else { "" };
                say(&format!(
                    "  {}. {}  (changed {}){mark}",
                    i + 1,
                    display(ctx, p),
                    time_of(*m)
                ));
            }
            loop {
                let input = ctx.console.ask(&format!(
                    "Press Enter for {}, or type another number (or skip):",
                    default + 1
                ))?;
                let t = input.trim();
                if is_skip(t) {
                    return Ok(None);
                }
                let choice = if t.is_empty() {
                    Some(default)
                } else {
                    t.parse::<usize>().ok().and_then(|k| k.checked_sub(1))
                };
                if let Some(k) = choice.filter(|k| *k < shown) {
                    return Ok(candidates.into_iter().nth(k).map(|c| c.0));
                }
                say(&format!("Please type a number from 1 to {shown}."));
            }
        }
    }
}

/// "skip" in any capitalisation.
pub fn is_skip(input: &str) -> bool {
    input.trim().eq_ignore_ascii_case("skip")
}

// ---------------------------------------------------------------- actions

fn set_readonly(ctx: &Ctx<'_>, file: &Path, readonly: bool) -> Outcome {
    let name = display(ctx, file);
    match apply_readonly(file, readonly) {
        Ok(()) => {
            let now = Stat::of(file).readonly;
            let detail = json!({ "path": name, "readonly": now });
            if now == Some(readonly) {
                Outcome::ok(if readonly {
                    format!("{name} is now read-only (like a file nobody has checked out).")
                } else {
                    format!("{name} can be written to again.")
                })
                .with(detail)
            } else {
                Outcome::fail(format!(
                    "Tried to change the read-only flag of {name}, but it did not stick."
                ))
                .with(detail)
            }
        }
        Err(e) => Outcome::fail(format!(
            "Could not change the read-only flag of {name}: {e}"
        )),
    }
}

/// Set or clear the read-only flag the way Git LFS does for `lockable` files:
/// on macOS/Linux remove all write bits (or give the owner write access back);
/// on Windows toggle the read-only attribute.
pub fn apply_readonly(file: &Path, readonly: bool) -> std::io::Result<()> {
    let mut perms = std::fs::metadata(file)?.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = perms.mode();
        perms.set_mode(if readonly {
            mode & !0o222
        } else {
            mode | 0o200
        });
    }
    #[cfg(not(unix))]
    {
        // Windows has a single read-only attribute; clearing it is what we want.
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(readonly);
    }
    std::fs::set_permissions(file, perms)
}

fn list_open_handles(roots: &[WatchRoot]) -> Outcome {
    if cfg!(windows) {
        return Outcome::fail(
            "Checking open files is not supported on Windows yet, so this check was skipped.",
        )
        .with(json!({ "supported": false }));
    }
    say(
        "Checking which programs have files open in the watched folders (this can take a few seconds)...",
    );
    let output = std::process::Command::new("lsof")
        .args(["-n", "-P", "-w", "-F", "pcan"])
        .output();
    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return Outcome::fail(format!("Could not run lsof to check open files: {e}"))
                .with(json!({ "supported": false, "error": e.to_string() }));
        }
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let open = parse_lsof(&text, roots);
    let detail = json!({
        "supported": true,
        "open": open.iter().map(|h| json!({
            "process": h.process, "pid": h.pid, "access": h.access, "path": h.path,
        })).collect::<Vec<_>>(),
    });
    if open.is_empty() {
        return Outcome::ok("No program has a file open in the watched folders right now.")
            .with(detail);
    }
    let mut by_proc: Vec<(String, Vec<String>)> = Vec::new();
    for h in &open {
        let entry = format!("{} ({})", h.path, access_word(&h.access));
        match by_proc.iter_mut().find(|(p, _)| *p == h.process) {
            Some((_, files)) => files.push(entry),
            None => by_proc.push((h.process.clone(), vec![entry])),
        }
    }
    let lines: Vec<String> = by_proc
        .iter()
        .map(|(p, files)| format!("{p} has open: {}", files.join(", ")))
        .collect();
    Outcome::ok(lines.join("\n")).with(detail)
}

fn access_word(a: &str) -> &'static str {
    match a {
        "r" => "reading",
        "w" => "writing",
        "u" => "reading and writing",
        _ => "open",
    }
}

/// A file some process has open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenHandle {
    /// Process name.
    pub process: String,
    /// Process id.
    pub pid: u32,
    /// `r`, `w`, `u` or empty (directories, memory maps).
    pub access: String,
    /// Path relative to its watch root (label-prefixed outside the first root).
    pub path: String,
}

/// Parse `lsof -F pcan` output, keeping files inside the roots.
pub fn parse_lsof(text: &str, roots: &[WatchRoot]) -> Vec<OpenHandle> {
    let mut out: Vec<OpenHandle> = Vec::new();
    let (mut pid, mut cmd, mut access) = (0u32, String::new(), String::new());
    for line in text.lines() {
        let mut chars = line.chars();
        let Some(tag) = chars.next() else { continue };
        let value: String = chars.collect();
        match tag {
            'p' => {
                pid = value.parse().unwrap_or(0);
                cmd.clear();
            }
            'c' => cmd = value,
            'f' => access.clear(),
            'a' => access = value.trim().to_string(),
            'n' => {
                let path = Path::new(&value);
                let r = to_ref(path, roots);
                if let Some(i) = r.root {
                    let shown = if i == 0 {
                        r.rel
                    } else {
                        format!(
                            "{}: {}",
                            roots.get(i).map(|x| x.label.as_str()).unwrap_or("?"),
                            r.rel
                        )
                    };
                    let handle = OpenHandle {
                        process: cmd.clone(),
                        pid,
                        access: access.clone(),
                        path: if shown.is_empty() {
                            "(the folder itself)".into()
                        } else {
                            shown
                        },
                    };
                    if !out.contains(&handle) {
                        out.push(handle);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn snapshot_tree(ctx: &Ctx<'_>) -> Outcome {
    let mut entries = Vec::new();
    for root in ctx.roots {
        let mut index = HashMap::new();
        scan(&root.path, &mut index, 200_000);
        let mut items: Vec<(PathBuf, Stat)> = index.into_iter().collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, stat) in items {
            let r = to_ref(&path, ctx.roots);
            entries.push(json!({
                "root": r.root, "path": r.rel, "dir": stat.dir, "size": stat.size,
                "mtime": stat.mtime_ns.map(|n| time_of(Some(n))), "readonly": stat.readonly,
            }));
        }
    }
    let dir = ctx.out_dir.join("snapshots");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Outcome::fail(format!("Could not create {}: {e}", dir.display()));
    }
    let mut k = 1;
    let file = loop {
        let f = dir.join(format!("{}-{k}.json", ctx.step_id));
        if !f.exists() {
            break f;
        }
        k += 1;
    };
    let text = match serde_json::to_string_pretty(&entries) {
        Ok(t) => t,
        Err(e) => return Outcome::fail(format!("Could not build the file list: {e}")),
    };
    match std::fs::write(&file, text) {
        Ok(()) => {
            let name = format!(
                "snapshots/{}",
                file.file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default()
            );
            Outcome::ok(format!(
                "Saved a list of the {} files and folders in the watched folders ({name}).",
                entries.len()
            ))
            .with(json!({ "file": name, "count": entries.len() }))
        }
        Err(e) => Outcome::fail(format!("Could not save the file list: {e}")),
    }
}

fn backup_file(ctx: &mut Ctx<'_>, file: &Path, slot: &str) -> Outcome {
    let name = display(ctx, file);
    let dir = ctx.backups_dir.join(slot);
    let file_name = file
        .file_name()
        .map(|f| f.to_os_string())
        .unwrap_or_default();
    let copy = dir.join(&file_name);
    let res = std::fs::create_dir_all(&dir).and_then(|()| {
        if copy.exists() {
            let _ = apply_readonly(&copy, false);
            std::fs::remove_file(&copy)?;
        }
        std::fs::copy(file, &copy)
    });
    match res {
        Ok(size) => {
            ctx.state.backups.insert(
                slot.to_string(),
                BackupInfo {
                    original: file.to_string_lossy().to_string(),
                    copy: copy.to_string_lossy().to_string(),
                    size,
                    taken: wall_now(),
                },
            );
            Outcome::ok(format!(
                "Took a safety copy of {name} ({}).",
                human_size(size)
            ))
            .with(json!({ "path": name, "slot": slot, "size": size }))
        }
        Err(e) => Outcome::fail(format!("Could not take a safety copy of {name}: {e}")),
    }
}

fn restore_file(ctx: &Ctx<'_>, slot: &str, method: RestoreMethod) -> Outcome {
    let Some(info) = ctx.state.backups.get(slot) else {
        return Outcome::fail(
            "There is no safety copy to put back (an earlier step was skipped), so this was skipped.",
        );
    };
    let original = PathBuf::from(&info.original);
    let copy = PathBuf::from(&info.copy);
    let name = display(ctx, &original);
    let res = match method {
        RestoreMethod::Replace => replace_with(&original, &copy),
        RestoreMethod::Overwrite => std::fs::read(&copy).and_then(|bytes| {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&original)?;
            f.write_all(&bytes)?;
            f.sync_all()
        }),
    };
    let how = match method {
        RestoreMethod::Replace => {
            "deleted the current file and wrote the older copy in its place, the way a version download would"
        }
        RestoreMethod::Overwrite => "rewrote the current file with the older contents",
    };
    let detail =
        json!({ "path": name, "slot": slot, "method": format!("{method:?}").to_lowercase() });
    match res {
        Ok(()) => Outcome::ok(format!(
            "Put the older copy of {name} back on disk ({how})."
        ))
        .with(detail),
        Err(e) => {
            let hint = if cfg!(windows) {
                " On Windows this usually means the program still has the file open, which is useful to know."
            } else {
                ""
            };
            Outcome::fail(format!(
                "Could not put the older copy of {name} back: {e}.{hint}"
            ))
            .with(json!({ "path": name, "error": e.to_string() }))
        }
    }
}

/// Delete `original` (if present) and copy `copy` to its place.
fn replace_with(original: &Path, copy: &Path) -> std::io::Result<()> {
    if original.exists()
        && let Err(e) = std::fs::remove_file(original)
    {
        // Windows refuses to delete read-only files; git clears the flag and retries.
        if cfg!(windows) && e.kind() == std::io::ErrorKind::PermissionDenied {
            apply_readonly(original, false)?;
            std::fs::remove_file(original)?;
        } else {
            return Err(e);
        }
    }
    std::fs::copy(copy, original).map(|_| ())
}

fn compare_with_backup(ctx: &Ctx<'_>, slot: &str) -> Outcome {
    let Some(info) = ctx.state.backups.get(slot) else {
        return Outcome::fail(
            "There is no safety copy to compare with (an earlier step was skipped), so this was skipped.",
        );
    };
    let original = PathBuf::from(&info.original);
    let name = display(ctx, &original);
    let now = Stat::of(&original);
    if !now.exists {
        return Outcome::ok(format!("{name} no longer exists."))
            .with(json!({ "path": name, "exists": false }));
    }
    let same = match (std::fs::read(&original), std::fs::read(&info.copy)) {
        (Ok(a), Ok(b)) => a == b,
        (Err(e), _) | (_, Err(e)) => {
            return Outcome::fail(format!(
                "Could not compare {name} with its safety copy: {e}"
            ));
        }
    };
    let ro = if now.readonly == Some(true) {
        " It is read-only."
    } else {
        " It is not read-only."
    };
    let size_now = now.size.unwrap_or(0);
    let msg = if same {
        format!("{name} is exactly the same as the safety copy taken earlier.{ro}")
    } else {
        format!(
            "{name} has changed since the safety copy was taken ({} -> {}).{ro}",
            human_size(info.size),
            human_size(size_now)
        )
    };
    Outcome::ok(msg).with(json!({
        "path": name, "exists": true, "same": same, "size_copy": info.size,
        "size_now": size_now, "readonly_now": now.readonly,
    }))
}

fn stat_file(ctx: &Ctx<'_>, file: &Path) -> Outcome {
    let name = display(ctx, file);
    let s = Stat::of(file);
    if !s.exists {
        return Outcome::ok(format!("{name} does not exist."))
            .with(json!({ "path": name, "exists": false }));
    }
    let ro = match s.readonly {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    };
    Outcome::ok(format!(
        "{name}: {}, read-only: {ro}, last changed {}.",
        human_size(s.size.unwrap_or(0)),
        time_of(s.mtime_ns)
    ))
    .with(json!({ "path": name, "exists": true, "size": s.size, "readonly": s.readonly, "mtime_ns": s.mtime_ns }))
}

fn clock(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn wait(ctx: &mut Ctx<'_>, seconds: u64, message: Option<&str>) -> Result<Outcome, ProbeError> {
    if let Some(m) = message {
        say(m);
    }
    let total = Duration::from_secs(seconds);
    let started = Instant::now();
    say(&format!(
        "Waiting {}. Leave the tool running. (Press Enter to stop waiting early.)",
        clock(seconds)
    ));
    loop {
        let left = total.saturating_sub(started.elapsed());
        if left.is_zero() {
            return Ok(Outcome::ok(format!("Waited {}.", clock(seconds)))
                .with(json!({ "seconds": seconds, "early": false })));
        }
        let chunk = left.min(Duration::from_secs(30));
        if ctx.console.line_timeout(chunk)?.is_some() {
            let waited = started.elapsed().as_secs();
            return Ok(Outcome::ok(format!(
                "Stopped waiting after {} (you pressed Enter).",
                clock(waited)
            ))
            .with(json!({ "seconds": waited, "early": true })));
        }
        let left = total.saturating_sub(started.elapsed()).as_secs();
        if left > 0 {
            say(&format!("  {} left...", clock(left)));
        }
    }
}

fn copy_in(ctx: &Ctx<'_>, source: &Path, spec: &ActionSpec) -> Outcome {
    let (Some(to), Some(root_id)) = (&spec.to, &spec.root) else {
        return Outcome::fail("copy_in is missing `to` or `root`.");
    };
    let Some(i) = ctx
        .watch_ids
        .iter()
        .position(|w| w.as_deref() == Some(root_id.as_str()))
    else {
        return Outcome::fail(format!(
            "Skipped: the folder \"{root_id}\" is not being watched."
        ));
    };
    let Some(root) = ctx.roots.get(i) else {
        return Outcome::fail("Skipped: unknown folder.");
    };
    let dest = to
        .split('/')
        .filter(|p| !p.is_empty())
        .fold(root.path.clone(), |acc, part| acc.join(part));
    let name = display(ctx, &dest);
    let res = (|| -> std::io::Result<u64> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if dest.exists() {
            let _ = apply_readonly(&dest, false);
            std::fs::remove_file(&dest)?;
        }
        let size = std::fs::copy(source, &dest)?;
        if Stat::of(&dest).readonly == Some(true) {
            apply_readonly(&dest, false)?;
        }
        Ok(size)
    })();
    let src_name = source
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    match res {
        Ok(size) => Outcome::ok(format!(
            "Copied \"{src_name}\" to {name} ({}).",
            human_size(size)
        ))
        .with(json!({ "from": source.to_string_lossy(), "to": name, "size": size })),
        Err(e) => Outcome::fail(format!("Could not copy \"{src_name}\" to {name}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsof_output_is_filtered_to_roots() {
        let roots = vec![
            WatchRoot {
                label: "Test".into(),
                path: PathBuf::from("/Users/me/Music/t"),
            },
            WatchRoot {
                label: "FL data".into(),
                path: PathBuf::from("/Users/me/Documents/Image-Line"),
            },
        ];
        let text = "p123\ncFL Studio\nfcwd\na \nn/Users/me\nf12\nar\nn/Users/me/Music/t/Project/Song A.flp\n\
                    f13\nau\nn/Users/me/Documents/Image-Line/FL Studio/Audio/Recorded/x.wav\np9\ncmds\nf3\nar\nn/etc/hosts\n";
        let open = parse_lsof(text, &roots);
        assert_eq!(open.len(), 2);
        assert_eq!(open[0].process, "FL Studio");
        assert_eq!(open[0].pid, 123);
        assert_eq!(open[0].access, "r");
        assert_eq!(open[0].path, "Project/Song A.flp");
        assert_eq!(open[1].path, "FL data: FL Studio/Audio/Recorded/x.wav");
        assert_eq!(open[1].access, "u");
    }

    #[test]
    fn readonly_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.flp");
        std::fs::write(&f, b"x").unwrap();
        apply_readonly(&f, true).unwrap();
        assert_eq!(Stat::of(&f).readonly, Some(true));
        apply_readonly(&f, false).unwrap();
        assert_eq!(Stat::of(&f).readonly, Some(false));
    }

    #[test]
    fn outside_globs_expand_per_component() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("FL Studio 2026.app/Demo songs");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("One.flp"), b"x").unwrap();
        std::fs::write(base.join("readme.txt"), b"x").unwrap();
        let pattern = format!("{}/FL Studio*.app/Demo songs/*.flp", dir.path().display());
        let found = find_outside(&[pattern]);
        assert_eq!(found.len(), 1);
        assert!(found[0].0.ends_with("One.flp"));
    }

    #[test]
    fn replace_with_creates_a_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let orig = dir.path().join("s.flp");
        let copy = dir.path().join("copy.flp");
        std::fs::write(&orig, b"new").unwrap();
        std::fs::write(&copy, b"old").unwrap();
        replace_with(&orig, &copy).unwrap();
        assert_eq!(std::fs::read(&orig).unwrap(), b"old");
    }

    #[test]
    fn skip_word() {
        assert!(is_skip(" SKIP "));
        assert!(!is_skip("skipped"));
    }
}
