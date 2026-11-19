//! Parser for `git status --porcelain=v2 -z --branch`.
//!
//! Every record ends with NUL. Rename/copy records (`2`) are followed by one
//! more NUL-terminated field holding the original path. Paths are raw bytes
//! (no quoting under `-z`), relative to the repository root.

use std::path::PathBuf;

use crate::error::ParseError;
use crate::paths::path_from_bytes;

const WHAT: &str = "status";

/// Parsed status output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusReport {
    /// Branch headers (empty unless `--branch` was given).
    pub branch: BranchInfo,
    /// Changed, untracked and ignored entries in output order.
    pub entries: Vec<StatusEntry>,
}

/// `# branch.*` headers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchInfo {
    /// Current commit; `None` before the first commit (`(initial)`) or when
    /// the header is absent.
    pub oid: Option<String>,
    /// Current branch; `None` when detached (`(detached)`) or absent.
    pub head: Option<String>,
    /// Whether `branch.head` reported `(detached)`.
    pub detached: bool,
    /// Upstream branch, if one is configured.
    pub upstream: Option<String>,
    /// Commits ahead of upstream, when the upstream exists.
    pub ahead: Option<u64>,
    /// Commits behind upstream, when the upstream exists.
    pub behind: Option<u64>,
}

/// One status entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusEntry {
    /// `1`: ordinary changed entry.
    Changed(ChangedEntry),
    /// `2`: renamed or copied entry.
    Renamed(RenamedEntry),
    /// `u`: unmerged entry.
    Unmerged(UnmergedEntry),
    /// `?`: untracked file.
    Untracked(PathBuf),
    /// `!`: ignored file.
    Ignored(PathBuf),
}

impl StatusEntry {
    /// The entry's (target) path.
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::Changed(e) => &e.path,
            Self::Renamed(e) => &e.path,
            Self::Unmerged(e) => &e.path,
            Self::Untracked(p) | Self::Ignored(p) => p,
        }
    }
}

/// State of one side of the `XY` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// `.`
    Unmodified,
    /// `M`
    Modified,
    /// `T`
    TypeChanged,
    /// `A`
    Added,
    /// `D`
    Deleted,
    /// `R`
    Renamed,
    /// `C`
    Copied,
    /// `U`
    UpdatedUnmerged,
}

/// The `XY` field: index (staged) and worktree (unstaged) state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusCode {
    /// `X`: index compared with `HEAD`.
    pub index: ChangeKind,
    /// `Y`: worktree compared with the index.
    pub worktree: ChangeKind,
}

/// The `<sub>` field.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SubmoduleState {
    /// The entry is a submodule.
    pub is_submodule: bool,
    /// The submodule's commit changed.
    pub commit_changed: bool,
    /// The submodule has tracked changes.
    pub modified: bool,
    /// The submodule has untracked files.
    pub untracked: bool,
}

/// `1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedEntry {
    /// `XY`.
    pub code: StatusCode,
    /// `<sub>`.
    pub submodule: SubmoduleState,
    /// File mode in `HEAD`.
    pub mode_head: u32,
    /// File mode in the index.
    pub mode_index: u32,
    /// File mode in the worktree.
    pub mode_worktree: u32,
    /// Object id in `HEAD`.
    pub oid_head: String,
    /// Object id in the index.
    pub oid_index: String,
    /// Path.
    pub path: PathBuf,
}

/// Whether a `2` entry is a rename or a copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameKind {
    /// `R<score>`
    Renamed,
    /// `C<score>`
    Copied,
}

/// `2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>\0<origPath>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamedEntry {
    /// `XY`.
    pub code: StatusCode,
    /// `<sub>`.
    pub submodule: SubmoduleState,
    /// File mode in `HEAD`.
    pub mode_head: u32,
    /// File mode in the index.
    pub mode_index: u32,
    /// File mode in the worktree.
    pub mode_worktree: u32,
    /// Object id in `HEAD`.
    pub oid_head: String,
    /// Object id in the index.
    pub oid_index: String,
    /// Rename or copy.
    pub kind: RenameKind,
    /// Similarity score, 0–100.
    pub score: u8,
    /// New path.
    pub path: PathBuf,
    /// Original path.
    pub orig_path: PathBuf,
}

/// `u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmergedEntry {
    /// `XY`.
    pub code: StatusCode,
    /// `<sub>`.
    pub submodule: SubmoduleState,
    /// Modes of stages 1 (base), 2 (ours), 3 (theirs).
    pub modes: [u32; 3],
    /// Worktree mode.
    pub mode_worktree: u32,
    /// Object ids of stages 1, 2, 3.
    pub oids: [String; 3],
    /// Path.
    pub path: PathBuf,
}

/// Parses the output of `git status --porcelain=v2 -z [--branch]`.
///
/// Unknown `#` headers are ignored for forward compatibility; anything else
/// that does not match the format is an error.
pub fn parse_status(data: &[u8]) -> Result<StatusReport, ParseError> {
    let mut report = StatusReport::default();
    let mut records = Records::new(data);
    while let Some(record) = records.next_record()? {
        let Some((&tag, rest)) = record.split_first() else {
            return Err(err("empty record"));
        };
        let rest = rest
            .strip_prefix(b" ")
            .ok_or_else(|| err("missing space after tag"))?;
        match tag {
            b'#' => parse_header(rest, &mut report.branch)?,
            b'1' => report
                .entries
                .push(StatusEntry::Changed(parse_changed(rest)?)),
            b'2' => {
                let orig = records
                    .next_record()?
                    .ok_or_else(|| err("rename record without original path"))?;
                report
                    .entries
                    .push(StatusEntry::Renamed(parse_renamed(rest, orig)?));
            }
            b'u' => report
                .entries
                .push(StatusEntry::Unmerged(parse_unmerged(rest)?)),
            b'?' => report.entries.push(StatusEntry::Untracked(path(rest)?)),
            b'!' => report.entries.push(StatusEntry::Ignored(path(rest)?)),
            _ => return Err(err(format!("unknown record tag {:?}", char::from(tag)))),
        }
    }
    Ok(report)
}

/// Iterates over NUL-terminated records; trailing bytes without a NUL are an
/// error.
struct Records<'a> {
    rest: &'a [u8],
}

impl<'a> Records<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { rest: data }
    }

    fn next_record(&mut self) -> Result<Option<&'a [u8]>, ParseError> {
        if self.rest.is_empty() {
            return Ok(None);
        }
        let end = self
            .rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| err("unterminated record"))?;
        let record = &self.rest[..end];
        self.rest = &self.rest[end + 1..];
        Ok(Some(record))
    }
}

fn err(message: impl Into<String>) -> ParseError {
    ParseError::new(WHAT, message)
}

fn parse_header(rest: &[u8], branch: &mut BranchInfo) -> Result<(), ParseError> {
    let (key, value) = match rest.iter().position(|&b| b == b' ') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, &b""[..]),
    };
    let text = |v: &[u8]| -> Result<String, ParseError> {
        if v.is_empty() {
            return Err(err("empty branch header value"));
        }
        String::from_utf8(v.to_vec()).map_err(|_| err("branch header is not UTF-8"))
    };
    match key {
        b"branch.oid" => {
            branch.oid = match value {
                b"(initial)" => None,
                v => Some(oid(v)?),
            }
        }
        b"branch.head" => match value {
            b"(detached)" => {
                branch.detached = true;
                branch.head = None;
            }
            v => branch.head = Some(text(v)?),
        },
        b"branch.upstream" => branch.upstream = Some(text(value)?),
        b"branch.ab" => {
            let mut parts = value.split(|&b| b == b' ');
            let ahead = parts.next().and_then(|p| p.strip_prefix(b"+"));
            let behind = parts.next().and_then(|p| p.strip_prefix(b"-"));
            match (
                ahead.and_then(number),
                behind.and_then(number),
                parts.next(),
            ) {
                (Some(a), Some(b), None) => {
                    branch.ahead = Some(a);
                    branch.behind = Some(b);
                }
                _ => return Err(err("malformed branch.ab header")),
            }
        }
        _ => {} // e.g. `# stash <n>`; ignored.
    }
    Ok(())
}

/// Splits `rest` into `n` fields separated by single spaces; the last field
/// takes the remainder (paths may contain spaces).
fn fields(rest: &[u8], n: usize) -> Result<Vec<&[u8]>, ParseError> {
    let parts: Vec<&[u8]> = rest.splitn(n, |&b| b == b' ').collect();
    if parts.len() != n {
        return Err(err(format!("expected {n} fields, found {}", parts.len())));
    }
    Ok(parts)
}

fn parse_changed(rest: &[u8]) -> Result<ChangedEntry, ParseError> {
    let f = fields(rest, 8)?;
    Ok(ChangedEntry {
        code: code(f[0])?,
        submodule: submodule(f[1])?,
        mode_head: mode(f[2])?,
        mode_index: mode(f[3])?,
        mode_worktree: mode(f[4])?,
        oid_head: oid(f[5])?,
        oid_index: oid(f[6])?,
        path: path(f[7])?,
    })
}

fn parse_renamed(rest: &[u8], orig: &[u8]) -> Result<RenamedEntry, ParseError> {
    let f = fields(rest, 9)?;
    let (kind, score) = rename_score(f[7])?;
    Ok(RenamedEntry {
        code: code(f[0])?,
        submodule: submodule(f[1])?,
        mode_head: mode(f[2])?,
        mode_index: mode(f[3])?,
        mode_worktree: mode(f[4])?,
        oid_head: oid(f[5])?,
        oid_index: oid(f[6])?,
        kind,
        score,
        path: path(f[8])?,
        orig_path: path(orig)?,
    })
}

fn parse_unmerged(rest: &[u8]) -> Result<UnmergedEntry, ParseError> {
    let f = fields(rest, 10)?;
    Ok(UnmergedEntry {
        code: code(f[0])?,
        submodule: submodule(f[1])?,
        modes: [mode(f[2])?, mode(f[3])?, mode(f[4])?],
        mode_worktree: mode(f[5])?,
        oids: [oid(f[6])?, oid(f[7])?, oid(f[8])?],
        path: path(f[9])?,
    })
}

fn code(field: &[u8]) -> Result<StatusCode, ParseError> {
    let kind = |b: u8| match b {
        b'.' => Ok(ChangeKind::Unmodified),
        b'M' => Ok(ChangeKind::Modified),
        b'T' => Ok(ChangeKind::TypeChanged),
        b'A' => Ok(ChangeKind::Added),
        b'D' => Ok(ChangeKind::Deleted),
        b'R' => Ok(ChangeKind::Renamed),
        b'C' => Ok(ChangeKind::Copied),
        b'U' => Ok(ChangeKind::UpdatedUnmerged),
        _ => Err(err("unknown XY status letter")),
    };
    match field {
        [x, y] => Ok(StatusCode {
            index: kind(*x)?,
            worktree: kind(*y)?,
        }),
        _ => Err(err("XY field must be two characters")),
    }
}

fn submodule(field: &[u8]) -> Result<SubmoduleState, ParseError> {
    match field {
        b"N..." => Ok(SubmoduleState::default()),
        [b'S', c, m, u] => {
            let flag = |actual: u8, set: u8| match actual {
                b'.' => Ok(false),
                x if x == set => Ok(true),
                _ => Err(err("malformed submodule field")),
            };
            Ok(SubmoduleState {
                is_submodule: true,
                commit_changed: flag(*c, b'C')?,
                modified: flag(*m, b'M')?,
                untracked: flag(*u, b'U')?,
            })
        }
        _ => Err(err("malformed submodule field")),
    }
}

fn mode(field: &[u8]) -> Result<u32, ParseError> {
    if field.is_empty() || field.len() > 7 || !field.iter().all(|b| (b'0'..=b'7').contains(b)) {
        return Err(err("malformed file mode"));
    }
    let text = std::str::from_utf8(field).map_err(|_| err("malformed file mode"))?;
    u32::from_str_radix(text, 8).map_err(|_| err("malformed file mode"))
}

/// Validates a SHA-1 (40) or SHA-256 (64) hex object id.
pub(crate) fn oid(field: &[u8]) -> Result<String, ParseError> {
    if !(field.len() == 40 || field.len() == 64) || !field.iter().all(u8::is_ascii_hexdigit) {
        return Err(err("malformed object id"));
    }
    String::from_utf8(field.to_vec()).map_err(|_| err("malformed object id"))
}

fn rename_score(field: &[u8]) -> Result<(RenameKind, u8), ParseError> {
    let (kind, digits) = match field.split_first() {
        Some((b'R', d)) => (RenameKind::Renamed, d),
        Some((b'C', d)) => (RenameKind::Copied, d),
        _ => return Err(err("malformed rename score")),
    };
    match number(digits) {
        Some(score @ 0..=100) => Ok((kind, u8::try_from(score).unwrap_or(100))),
        _ => Err(err("malformed rename score")),
    }
}

fn number(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

fn path(bytes: &[u8]) -> Result<PathBuf, ParseError> {
    if bytes.is_empty() {
        return Err(err("empty path"));
    }
    path_from_bytes(bytes, WHAT)
}

#[cfg(test)]
mod tests {
    use super::*;

    const H1: &str = "1111111111111111111111111111111111111111";
    const H2: &str = "2222222222222222222222222222222222222222";
    const Z: &str = "0000000000000000000000000000000000000000";

    fn input(records: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for r in records {
            out.extend_from_slice(r.as_bytes());
            out.push(0);
        }
        out
    }

    #[test]
    fn parses_branch_headers() {
        let data = input(&[
            &format!("# branch.oid {H1}"),
            "# branch.head main",
            "# branch.upstream origin/main",
            "# branch.ab +2 -3",
            "# stash 4",
        ]);
        let report = parse_status(&data).unwrap();
        assert_eq!(report.branch.oid.as_deref(), Some(H1));
        assert_eq!(report.branch.head.as_deref(), Some("main"));
        assert_eq!(report.branch.upstream.as_deref(), Some("origin/main"));
        assert_eq!(
            (report.branch.ahead, report.branch.behind),
            (Some(2), Some(3))
        );
        assert!(report.entries.is_empty());
    }

    #[test]
    fn parses_initial_and_detached() {
        let data = input(&["# branch.oid (initial)", "# branch.head (detached)"]);
        let b = parse_status(&data).unwrap().branch;
        assert_eq!(b.oid, None);
        assert_eq!(b.head, None);
        assert!(b.detached);
    }

    #[test]
    fn parses_every_entry_kind() {
        let data = input(&[
            &format!("1 .M N... 100644 100644 100644 {H1} {H1} dir/a file.wav"),
            &format!("2 R. N... 100644 100644 100644 {H1} {H1} R100 new name.flp"),
            "old name.flp",
            &format!("u UU N... 100644 100644 100644 100644 {H1} {H2} {H1} song.flp"),
            "? -leading dash.wav",
            "! build/cache.bin",
            &format!("1 A. SC.U 000000 160000 160000 {Z} {H2} sub"),
        ]);
        let report = parse_status(&data).unwrap();
        assert_eq!(report.entries.len(), 6);
        let StatusEntry::Changed(c) = &report.entries[0] else {
            panic!("expected changed")
        };
        assert_eq!(c.code.index, ChangeKind::Unmodified);
        assert_eq!(c.code.worktree, ChangeKind::Modified);
        assert_eq!(c.mode_head, 0o100644);
        assert_eq!(c.path, PathBuf::from("dir/a file.wav"));

        let StatusEntry::Renamed(r) = &report.entries[1] else {
            panic!("expected renamed")
        };
        assert_eq!(r.kind, RenameKind::Renamed);
        assert_eq!(r.score, 100);
        assert_eq!(r.path, PathBuf::from("new name.flp"));
        assert_eq!(r.orig_path, PathBuf::from("old name.flp"));

        let StatusEntry::Unmerged(u) = &report.entries[2] else {
            panic!("expected unmerged")
        };
        assert_eq!(u.code.index, ChangeKind::UpdatedUnmerged);
        assert_eq!(u.oids[1], H2);

        assert_eq!(
            report.entries[3],
            StatusEntry::Untracked(PathBuf::from("-leading dash.wav"))
        );
        assert_eq!(
            report.entries[4],
            StatusEntry::Ignored(PathBuf::from("build/cache.bin"))
        );
        let StatusEntry::Changed(s) = &report.entries[5] else {
            panic!("expected submodule")
        };
        assert!(s.submodule.is_submodule && s.submodule.commit_changed);
        assert!(!s.submodule.modified && s.submodule.untracked);
    }

    #[test]
    fn accepts_sha256_ids() {
        let h = "a".repeat(64);
        let data = input(&[&format!("1 M. N... 100644 100644 100644 {h} {h} x")]);
        assert_eq!(parse_status(&data).unwrap().entries.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn keeps_non_utf8_and_newline_paths() {
        use std::os::unix::ffi::OsStrExt;
        let mut data = b"? bad\xff\nname".to_vec();
        data.push(0);
        let report = parse_status(&data).unwrap();
        assert_eq!(
            report.entries[0].path().as_os_str().as_bytes(),
            b"bad\xff\nname"
        );
    }

    #[test]
    fn empty_input_is_empty_report() {
        assert_eq!(parse_status(b"").unwrap(), StatusReport::default());
    }

    #[test]
    fn rejects_malformed_input() {
        let rename_missing_orig = format!("2 R. N... 100644 100644 100644 {H1} {H1} R100 new\0");
        let cases: Vec<Vec<u8>> = vec![
            b"? unterminated".to_vec(),
            b"\0".to_vec(),
            b"?\0".to_vec(),
            b"? \0".to_vec(),
            b"x foo\0".to_vec(),
            b"1 .M N...\0".to_vec(),
            format!("1 .Q N... 100644 100644 100644 {H1} {H1} a\0").into_bytes(),
            format!("1 .M X... 100644 100644 100644 {H1} {H1} a\0").into_bytes(),
            format!("1 .M N... 100648 100644 100644 {H1} {H1} a\0").into_bytes(),
            format!("1 .M N... 100644 100644 100644 {H1} zz a\0").into_bytes(),
            format!("1 .M N... 100644 100644 100644 {H1} {H1} \0").into_bytes(),
            rename_missing_orig.into_bytes(),
            format!("2 R. N... 100644 100644 100644 {H1} {H1} R101 new\0old\0").into_bytes(),
            format!("2 R. N... 100644 100644 100644 {H1} {H1} X50 new\0old\0").into_bytes(),
            b"# branch.ab +1\0".to_vec(),
            b"# branch.ab +x -1\0".to_vec(),
            b"# branch.oid nothex\0".to_vec(),
            b"# branch.head \0".to_vec(),
            vec![0xff, 0xfe, 0x00],
        ];
        for case in cases {
            assert!(
                parse_status(&case).is_err(),
                "accepted {:?}",
                String::from_utf8_lossy(&case)
            );
        }
    }
}
