//! Real filesystem operations under a running recorder, classified from the
//! trace it writes. These run on macOS (FSEvents) and Linux (inotify); on
//! Windows they compile and can be run by hand with `-- --ignored`.

// Tests may unwrap (clippy.toml allows it in #[test] fns; this covers helpers).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use fsprobe::actions::apply_readonly;
use fsprobe::classify::{ClassifyOptions, FileChange, Observation, Pattern, classify};
use fsprobe::paths::{WatchRoot, resolve_dir};
use fsprobe::recorder::{Recorder, RecorderOptions};
use fsprobe::trace::{Phase, read_trace, run_os, step_window};

struct Probe {
    _watched: tempfile::TempDir,
    _out: tempfile::TempDir,
    root: PathBuf,
    trace: PathBuf,
    recorder: Option<Recorder>,
}

impl Probe {
    /// Prepare files, then start recording.
    fn start(prepare: impl FnOnce(&Path)) -> Self {
        let watched = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let root = resolve_dir(watched.path()).unwrap();
        prepare(&root);
        // Let the OS deliver anything from the setup before watching starts.
        std::thread::sleep(Duration::from_millis(300));
        let trace = out.path().join("trace.jsonl");
        let recorder = Recorder::start(
            vec![WatchRoot {
                label: "Test".into(),
                path: root.clone(),
            }],
            &trace,
            1,
            RecorderOptions {
                poll_every: Duration::from_millis(100),
                ..RecorderOptions::default()
            },
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(500));
        Self {
            _watched: watched,
            _out: out,
            root,
            trace,
            recorder: Some(recorder),
        }
    }

    /// Run `op` between two markers and classify what happened.
    fn step(&mut self, id: &str, op: impl FnOnce(&Path)) -> Observation {
        let rec = self.recorder.as_ref().unwrap();
        rec.marker(id, Phase::Start);
        op(&self.root);
        std::thread::sleep(Duration::from_millis(300));
        assert!(rec.wait_quiet(Duration::from_millis(1500), Duration::from_secs(20)));
        rec.marker(id, Phase::End);
        rec.flush().unwrap();
        let (records, damaged) = read_trace(&self.trace).unwrap();
        assert_eq!(damaged, 0);
        let window = step_window(&records, id).expect("step window");
        let opts = ClassifyOptions::default().for_os(run_os(&records, window));
        classify(window, &opts)
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        if let Some(r) = self.recorder.take() {
            r.stop().unwrap();
        }
    }
}

fn find<'a>(obs: &'a Observation, rel: &str) -> &'a FileChange {
    obs.files
        .iter()
        .find(|f| f.path.rel == rel)
        .unwrap_or_else(|| panic!("no change recorded for {rel}: {:#?}", obs.files))
}

fn write_bytes(path: &Path, n: usize, fill: u8) {
    std::fs::write(path, vec![fill; n]).unwrap();
}

#[test]
#[cfg_attr(windows, ignore = "run by hand on Windows")]
fn in_place_overwrite() {
    let mut p = Probe::start(|r| write_bytes(&r.join("Song.flp"), 1000, b'a'));
    let obs = p.step("inplace", |r| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(r.join("Song.flp"))
            .unwrap();
        f.write_all(&[b'b'; 2000]).unwrap();
        f.sync_all().unwrap();
    });
    let c = find(&obs, "Song.flp");
    assert_eq!(c.pattern, Pattern::InPlaceWrite, "{c:#?}");
    assert_eq!(c.size_after, Some(2000));
    assert_eq!(obs.files.len(), 1, "{:#?}", obs.files);
}

#[test]
#[cfg_attr(windows, ignore = "run by hand on Windows")]
fn temp_file_then_rename_over_target() {
    let mut p = Probe::start(|r| write_bytes(&r.join("Song.flp"), 1000, b'a'));
    let obs = p.step("rename", |r| {
        let tmp = r.join("Song.flp.tmp");
        write_bytes(&tmp, 3000, b'c');
        std::fs::rename(&tmp, r.join("Song.flp")).unwrap();
    });
    let c = find(&obs, "Song.flp");
    assert_eq!(c.pattern, Pattern::TempThenRename, "{c:#?}");
    assert_eq!(
        c.temp.as_ref().map(|t| t.rel.as_str()),
        Some("Song.flp.tmp")
    );
    assert_eq!(c.temp_pattern.as_deref(), Some("{name}.tmp"));
    // The temp file is folded into the target, not reported on its own.
    assert_eq!(obs.files.len(), 1, "{:#?}", obs.files);
}

#[test]
#[cfg_attr(windows, ignore = "run by hand on Windows")]
fn delete_then_create() {
    let mut p = Probe::start(|r| write_bytes(&r.join("Song.flp"), 1000, b'a'));
    let obs = p.step("recreate", |r| {
        std::fs::remove_file(r.join("Song.flp")).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        write_bytes(&r.join("Song.flp"), 1500, b'd');
    });
    let c = find(&obs, "Song.flp");
    assert_eq!(c.pattern, Pattern::DeleteThenCreate, "{c:#?}");
}

#[test]
#[cfg_attr(windows, ignore = "run by hand on Windows")]
fn slow_multi_chunk_write_has_settle_time() {
    let mut p = Probe::start(|_| {});
    let obs = p.step("slow", |r| {
        let mut f = std::fs::File::create(r.join("mix.wav")).unwrap();
        for _ in 0..5 {
            f.write_all(&[0u8; 64 * 1024]).unwrap();
            f.sync_all().unwrap();
            std::thread::sleep(Duration::from_millis(300));
        }
    });
    let c = find(&obs, "mix.wav");
    assert_eq!(c.pattern, Pattern::NewFile, "{c:#?}");
    assert_eq!(c.size_after, Some(5 * 64 * 1024));
    let settle = c.settle_ms.unwrap_or(0);
    assert!(settle >= 600, "settle time {settle} ms: {c:#?}");
    assert_eq!(c.times, 1);
}

#[test]
#[cfg_attr(windows, ignore = "run by hand on Windows")]
fn read_only_toggle_is_detected() {
    let mut p = Probe::start(|r| write_bytes(&r.join("Song.flp"), 1000, b'a'));
    let obs = p.step("readonly", |r| {
        apply_readonly(&r.join("Song.flp"), true).unwrap()
    });
    let c = find(&obs, "Song.flp");
    assert_eq!(c.pattern, Pattern::MetadataOnly, "{c:#?}");
    assert!(c.readonly_changed);
    assert_eq!(c.readonly_before, Some(false));
    assert_eq!(c.readonly_after, Some(true));
}

#[test]
#[cfg_attr(windows, ignore = "run by hand on Windows")]
fn old_file_moved_aside_then_new_one_written() {
    let mut p = Probe::start(|r| {
        std::fs::create_dir(r.join("Backup")).unwrap();
        write_bytes(&r.join("Song.flp"), 1000, b'a');
    });
    let obs = p.step("aside", |r| {
        std::fs::rename(r.join("Song.flp"), r.join("Backup/Song (old).flp")).unwrap();
        write_bytes(&r.join("Song.flp"), 1200, b'e');
    });
    let c = find(&obs, "Song.flp");
    assert_eq!(c.pattern, Pattern::MovedAsideThenCreate, "{c:#?}");
    assert_eq!(
        c.moved_to.as_ref().map(|m| m.rel.as_str()),
        Some("Backup/Song (old).flp")
    );
    let b = find(&obs, "Backup/Song (old).flp");
    assert_eq!(b.pattern, Pattern::NewFile, "{b:#?}");
}

#[test]
#[cfg_attr(windows, ignore = "run by hand on Windows")]
fn consecutive_steps_are_separate_windows() {
    let mut p = Probe::start(|r| write_bytes(&r.join("Song.flp"), 1000, b'a'));
    let first = p.step("one", |r| write_bytes(&r.join("Song_2.flp"), 10, b'x'));
    assert_eq!(find(&first, "Song_2.flp").pattern, Pattern::NewFile);
    let second = p.step("two", |r| {
        std::fs::remove_file(r.join("Song_2.flp")).unwrap()
    });
    assert_eq!(find(&second, "Song_2.flp").pattern, Pattern::Removed);
    assert!(second.files.iter().all(|f| f.path.rel != "Song.flp"));
}
