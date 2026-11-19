//! Parser for `git log -z --format=`[`LOG_FORMAT`].
//!
//! Each commit is exactly [`LOG_FIELD_COUNT`] NUL-terminated fields: the
//! format separates fields with `%x00` and `-z` terminates each commit with
//! NUL. Because the count is fixed, subjects and bodies may contain any
//! character except NUL (which git does not allow in commit messages).

use crate::error::ParseError;
use crate::status::oid;

const WHAT: &str = "log";

/// The `--format` string [`parse_log`] expects; use it together with `-z`.
///
/// Fields: commit id, parent ids (space separated), author name, author
/// email, author time (unix seconds), author date (strict ISO 8601),
/// subject, body.
pub const LOG_FORMAT: &str = "%H%x00%P%x00%an%x00%ae%x00%at%x00%aI%x00%s%x00%b";

/// Number of NUL-terminated fields per commit in [`LOG_FORMAT`] output.
pub const LOG_FIELD_COUNT: usize = 8;

/// One commit from `git log`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// Commit id (hex).
    pub id: String,
    /// Parent ids (empty for a root commit).
    pub parents: Vec<String>,
    /// Author name (lossy UTF-8).
    pub author_name: String,
    /// Author email (lossy UTF-8).
    pub author_email: String,
    /// Author time as unix seconds.
    pub author_time: i64,
    /// Author date in strict ISO 8601 with the author's offset.
    pub author_date: String,
    /// Subject: the first paragraph of the message, lines joined by spaces.
    pub subject: String,
    /// Body: the message after the subject paragraph (may be empty).
    pub body: String,
}

/// Parses `git log -z --format=`[`LOG_FORMAT`] output.
pub fn parse_log(data: &[u8]) -> Result<Vec<Commit>, ParseError> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let body = data
        .strip_suffix(b"\0")
        .ok_or_else(|| ParseError::new(WHAT, "output does not end with NUL"))?;
    let fields: Vec<&[u8]> = body.split(|&b| b == 0).collect();
    if !fields.len().is_multiple_of(LOG_FIELD_COUNT) {
        return Err(ParseError::new(
            WHAT,
            format!(
                "field count {} is not a multiple of {LOG_FIELD_COUNT}",
                fields.len()
            ),
        ));
    }
    let (commits, _) = fields.as_chunks::<LOG_FIELD_COUNT>();
    commits.iter().map(parse_commit).collect()
}

fn parse_commit(f: &[&[u8]; LOG_FIELD_COUNT]) -> Result<Commit, ParseError> {
    let id = oid(f[0]).map_err(|_| ParseError::new(WHAT, "malformed commit id"))?;
    let parents = if f[1].is_empty() {
        Vec::new()
    } else {
        f[1].split(|&b| b == b' ')
            .map(|p| oid(p).map_err(|_| ParseError::new(WHAT, "malformed parent id")))
            .collect::<Result<_, _>>()?
    };
    let author_time = std::str::from_utf8(f[4])
        .ok()
        .and_then(|t| t.parse::<i64>().ok())
        .ok_or_else(|| ParseError::new(WHAT, "malformed author time"))?;
    let author_date = std::str::from_utf8(f[5])
        .ok()
        .filter(|d| d.len() >= 20 && d.as_bytes().get(10) == Some(&b'T'))
        .ok_or_else(|| ParseError::new(WHAT, "malformed author date"))?
        .to_owned();
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    Ok(Commit {
        id,
        parents,
        author_name: text(f[2]),
        author_email: text(f[3]),
        author_time,
        author_date,
        subject: text(f[6]),
        body: text(f[7]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccccccccccc";

    fn record(fields: [&str; LOG_FIELD_COUNT]) -> Vec<u8> {
        let mut out = Vec::new();
        for f in fields {
            out.extend_from_slice(f.as_bytes());
            out.push(0);
        }
        out
    }

    #[test]
    fn parses_commits() {
        let mut data = record([
            A,
            &format!("{B} {C}"),
            "Nguyễn Văn Ánh",
            "anh@example.com",
            "1700000000",
            "2023-11-14T22:13:20+07:00",
            "Merge mix: \"final\" | tabs\there",
            "Line one\n\nLine three with \x1e and \u{1F3B5}\n",
        ]);
        data.extend(record([
            B,
            "",
            "x",
            "",
            "-5",
            "1969-12-31T23:59:55Z",
            "",
            "",
        ]));
        let commits = parse_log(&data).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].id, A);
        assert_eq!(commits[0].parents, vec![B.to_owned(), C.to_owned()]);
        assert_eq!(commits[0].author_name, "Nguyễn Văn Ánh");
        assert_eq!(commits[0].author_time, 1_700_000_000);
        assert!(commits[0].body.contains("Line three"));
        assert!(commits[1].parents.is_empty());
        assert_eq!(commits[1].author_time, -5);
        assert_eq!(commits[1].body, "");
    }

    #[test]
    fn empty_output_is_no_commits() {
        assert!(parse_log(b"").unwrap().is_empty());
    }

    #[test]
    fn rejects_malformed_input() {
        let good = record([A, "", "n", "e", "1", "2023-11-14T22:13:20+07:00", "s", "b"]);
        let mut unterminated = good.clone();
        unterminated.pop();
        let mut extra = good.clone();
        extra.extend_from_slice(b"junk\0");
        let bad_id = record([
            "zz",
            "",
            "n",
            "e",
            "1",
            "2023-11-14T22:13:20+07:00",
            "s",
            "b",
        ]);
        let bad_parent = record([
            A,
            "nope",
            "n",
            "e",
            "1",
            "2023-11-14T22:13:20+07:00",
            "s",
            "b",
        ]);
        let bad_time = record([
            A,
            "",
            "n",
            "e",
            "soon",
            "2023-11-14T22:13:20+07:00",
            "s",
            "b",
        ]);
        let bad_date = record([A, "", "n", "e", "1", "yesterday", "s", "b"]);
        for case in [
            unterminated,
            extra,
            bad_id,
            bad_parent,
            bad_time,
            bad_date,
            b"\0".to_vec(),
        ] {
            assert!(parse_log(&case).is_err(), "accepted {case:?}");
        }
    }
}
