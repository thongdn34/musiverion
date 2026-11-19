//! Git LFS progress: parsing `GIT_LFS_PROGRESS` lines and tailing the file
//! while a run is in progress.
//!
//! git-lfs appends one line per update to the file named by
//! `GIT_LFS_PROGRESS`:
//!
//! ```text
//! <direction> <current>/<total files> <downloaded>/<total bytes> <name>
//! ```
//!
//! `<name>` is the rest of the line and may contain spaces.

use std::path::PathBuf;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_util::sync::{CancellationToken, DropGuard};

use crate::error::ParseError;
use crate::paths::path_from_bytes;

const WHAT: &str = "lfs progress";

/// Direction of an LFS transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfsDirection {
    /// Uploading to the remote (push).
    Upload,
    /// Downloading from the remote (fetch, pull, smudge).
    Download,
    /// Writing downloaded objects into the working tree.
    Checkout,
    /// Running the clean filter (hashing a working-tree file into the LFS
    /// store), for example while `git lfs pull` refreshes the index.
    Clean,
}

/// One parsed progress line.
///
/// The byte counts are reported as git-lfs writes them, and their meaning
/// depends on the direction: for `download`/`upload`/`clean` they are the
/// current file's progress (`done/size`); for `checkout` they are the
/// current file's size over the total size of all files being checked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsProgress {
    /// Transfer direction.
    pub direction: LfsDirection,
    /// Index of the current file (1-based).
    pub current_file: u64,
    /// Total files in this transfer.
    pub total_files: u64,
    /// First byte count (see the type docs).
    pub bytes_done: u64,
    /// Second byte count (see the type docs).
    pub bytes_total: u64,
    /// Repository path of the file.
    pub name: PathBuf,
}

/// Parses one progress line (without or with its trailing newline).
pub fn parse_lfs_progress_line(line: &[u8]) -> Result<LfsProgress, ParseError> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let mut parts = line.splitn(4, |&b| b == b' ');
    let direction = match parts.next() {
        Some(b"upload") => LfsDirection::Upload,
        Some(b"download") => LfsDirection::Download,
        Some(b"checkout") => LfsDirection::Checkout,
        Some(b"clean") => LfsDirection::Clean,
        _ => return Err(ParseError::new(WHAT, "unknown direction")),
    };
    let (current_file, total_files) = parse_fraction(parts.next(), "file counts")?;
    let (bytes_done, bytes_total) = parse_fraction(parts.next(), "byte counts")?;
    let name = match parts.next() {
        Some(name) if !name.is_empty() => path_from_bytes(name, WHAT)?,
        _ => return Err(ParseError::new(WHAT, "missing file name")),
    };
    Ok(LfsProgress {
        direction,
        current_file,
        total_files,
        bytes_done,
        bytes_total,
        name,
    })
}

/// Parses every complete line of a progress file. Fails on the first
/// malformed line.
pub fn parse_lfs_progress(data: &[u8]) -> Result<Vec<LfsProgress>, ParseError> {
    data.split(|&b| b == b'\n')
        .filter(|line| !line.is_empty())
        .map(parse_lfs_progress_line)
        .collect()
}

fn parse_fraction(field: Option<&[u8]>, label: &str) -> Result<(u64, u64), ParseError> {
    let field = field.ok_or_else(|| ParseError::new(WHAT, format!("missing {label}")))?;
    let mut halves = field.splitn(2, |&b| b == b'/');
    let a = halves.next().and_then(parse_u64);
    let b = halves.next().and_then(parse_u64);
    match (a, b) {
        (Some(a), Some(b)) => Ok((a, b)),
        _ => Err(ParseError::new(WHAT, format!("malformed {label}"))),
    }
}

fn parse_u64(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

/// Follows a `GIT_LFS_PROGRESS` file in a background task and sends each
/// parsed line to a channel.
///
/// Malformed lines are skipped (logged at debug level). Dropping the tail
/// stops it after a final read; [`LfsProgressTail::finish`] does the same and
/// waits for the last events to be sent.
///
/// The channel applies backpressure: receive concurrently, or `finish`
/// waits until there is room for the remaining events.
#[derive(Debug)]
pub struct LfsProgressTail {
    stop: Option<DropGuard>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl LfsProgressTail {
    /// Starts tailing `path`, polling every `poll`. Must be called inside a
    /// tokio runtime. A missing file is waited for.
    pub fn spawn(path: PathBuf, events: mpsc::Sender<LfsProgress>, poll: Duration) -> Self {
        let token = CancellationToken::new();
        let task = tokio::spawn(tail_loop(path, events, poll, token.clone()));
        Self {
            stop: Some(token.drop_guard()),
            task: Some(task),
        }
    }

    /// Stops tailing after reading whatever is left in the file, and waits
    /// for the remaining events to be delivered.
    pub async fn finish(mut self) {
        drop(self.stop.take());
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

async fn tail_loop(
    path: PathBuf,
    events: mpsc::Sender<LfsProgress>,
    poll: Duration,
    stop: CancellationToken,
) {
    let mut file = None;
    let mut pending = Vec::new();
    loop {
        let stopping = stop.is_cancelled();
        if file.is_none() {
            file = tokio::fs::File::open(&path).await.ok();
        }
        if let Some(f) = file.as_mut() {
            let mut fresh = Vec::new();
            if let Err(err) = f.read_to_end(&mut fresh).await {
                tracing::debug!(error = %err, "reading lfs progress file failed");
            }
            pending.extend_from_slice(&fresh);
            while let Some(end) = pending.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = pending.drain(..=end).collect();
                match parse_lfs_progress_line(&line) {
                    Ok(progress) => {
                        if events.send(progress).await.is_err() {
                            return;
                        }
                    }
                    Err(err) => tracing::debug!(error = %err, "skipping lfs progress line"),
                }
            }
        }
        if stopping {
            return;
        }
        tokio::select! {
            () = stop.cancelled() => {}
            () = tokio::time::sleep(poll) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_direction() {
        let p = parse_lfs_progress_line(b"upload 1/3 1024/2048 audio/kick drum.wav\n").unwrap();
        assert_eq!(p.direction, LfsDirection::Upload);
        assert_eq!((p.current_file, p.total_files), (1, 3));
        assert_eq!((p.bytes_done, p.bytes_total), (1024, 2048));
        assert_eq!(p.name, PathBuf::from("audio/kick drum.wav"));

        let p = parse_lfs_progress_line(b"download 2/2 0/5 x.wav").unwrap();
        assert_eq!(p.direction, LfsDirection::Download);
        let p = parse_lfs_progress_line("checkout 1/1 5/5 Bản thu âm.wav".as_bytes()).unwrap();
        assert_eq!(p.direction, LfsDirection::Checkout);
        assert_eq!(p.name, PathBuf::from("Bản thu âm.wav"));
    }

    #[test]
    fn parses_recorded_sample() {
        // Recorded with git-lfs 3.7.1: `git lfs pull` (clean, checkout) and a
        // smudging `git clone` (download) from a local bare remote.
        let sample = b"clean 1/1 200000/200000 a.wav\n\
clean 1/1 50000/50000 b c.wav\n\
checkout 1/2 200000/250000 a.wav\n\
checkout 2/2 50000/250000 b c.wav\n\
download 1/1 200000/200000 a.wav\n\
download 1/1 50000/50000 b c.wav\n";
        let events = parse_lfs_progress(sample).unwrap();
        assert_eq!(events.len(), 6);
        assert_eq!(events[0].direction, LfsDirection::Clean);
        assert_eq!(events[3].direction, LfsDirection::Checkout);
        assert_eq!((events[3].current_file, events[3].total_files), (2, 2));
        assert_eq!(
            (events[3].bytes_done, events[3].bytes_total),
            (50000, 250000)
        );
        assert_eq!(events[5].direction, LfsDirection::Download);
        assert_eq!(events[5].name, PathBuf::from("b c.wav"));
    }

    #[test]
    fn rejects_malformed_lines() {
        for bad in [
            &b""[..],
            b"upload",
            b"sideways 1/1 1/1 a",
            b"upload 1 1/1 a",
            b"upload 1/ 1/1 a",
            b"upload 1/1 x/1 a",
            b"upload 1/1 1/1",
            b"upload 1/1 1/1 ",
            b"upload -1/1 1/1 a",
            b"upload 99999999999999999999999/1 1/1 a",
        ] {
            assert!(parse_lfs_progress_line(bad).is_err(), "accepted {bad:?}");
        }
        assert!(parse_lfs_progress(b"upload 1/1 1/1 a\ngarbage\n").is_err());
    }

    #[tokio::test]
    async fn tail_emits_appended_lines() {
        use tokio::io::AsyncWriteExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("progress");
        let (tx, mut rx) = mpsc::channel(16);
        let tail = LfsProgressTail::spawn(path.clone(), tx, Duration::from_millis(10));

        let mut file = tokio::fs::File::create(&path).await.unwrap();
        file.write_all(b"download 1/1 0/10 a.wav\ndownload 1/1 5/")
            .await
            .unwrap();
        file.flush().await.unwrap();
        let first = rx.recv().await.unwrap();
        assert_eq!(first.bytes_done, 0);

        file.write_all(b"10 a.wav\nnot a line\ndownload 1/1 10/10 a.wav\n")
            .await
            .unwrap();
        file.flush().await.unwrap();
        tail.finish().await;
        let rest: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(rest.len(), 2);
        assert_eq!(rest[1].bytes_done, 10);
    }
}
