//! Path helpers: watch roots, relative paths, globs, typed-in folder paths.

use std::path::{Component, Path, PathBuf};

use crate::error::ProbeError;
use crate::trace::PathRef;

/// A watched folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchRoot {
    /// Short label shown to the user ("Test folder").
    pub label: String,
    /// Absolute, resolved path.
    pub path: PathBuf,
}

/// Resolve a folder to the absolute form the OS reports in events
/// (`/var` → `/private/var` on macOS). On Windows the `\\?\` prefix that
/// `canonicalize` adds is avoided.
pub fn resolve_dir(path: &Path) -> Result<PathBuf, ProbeError> {
    let meta = std::fs::metadata(path).map_err(|e| ProbeError::WatchFolder {
        path: path.to_path_buf(),
        message: if e.kind() == std::io::ErrorKind::NotFound {
            "it does not exist".to_string()
        } else {
            e.to_string()
        },
    })?;
    if !meta.is_dir() {
        return Err(ProbeError::WatchFolder {
            path: path.to_path_buf(),
            message: "it is a file, not a folder".to_string(),
        });
    }
    resolve_existing(path)
}

#[cfg(not(windows))]
fn resolve_existing(path: &Path) -> Result<PathBuf, ProbeError> {
    std::fs::canonicalize(path).map_err(|e| ProbeError::WatchFolder {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

#[cfg(windows)]
fn resolve_existing(path: &Path) -> Result<PathBuf, ProbeError> {
    std::path::absolute(path).map_err(|e| ProbeError::WatchFolder {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

/// The user's home folder.
pub fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Expand a leading `~`.
pub fn expand_tilde(text: &str) -> PathBuf {
    let rest = text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\"));
    match (rest, home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ if text == "~" => home_dir().unwrap_or_else(|| PathBuf::from(text)),
        _ => PathBuf::from(text),
    }
}

/// Clean a path the user typed or dragged into the terminal: trims spaces,
/// strips surrounding quotes and (on macOS/Linux) the backslash escapes the
/// terminal adds for spaces, then expands `~`.
pub fn clean_typed_path(input: &str) -> PathBuf {
    let mut s = input.trim().to_string();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            s = s[1..s.len() - 1].to_string();
        }
    }
    if !cfg!(windows) {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else {
                out.push(c);
            }
        }
        s = out;
    }
    let trimmed = s.trim_end_matches(['/', '\\']);
    let s = if trimmed.is_empty() {
        s.as_str()
    } else {
        trimmed
    };
    expand_tilde(s)
}

/// `true` if `inner` is `outer` or lies inside it (by path components).
pub fn is_within(inner: &Path, outer: &Path) -> bool {
    let fold = |p: &Path| -> Vec<String> {
        p.components()
            .filter(|c| !matches!(c, Component::CurDir))
            .map(|c| {
                let s = c.as_os_str().to_string_lossy().to_string();
                if cfg!(any(windows, target_os = "macos")) {
                    s.to_lowercase()
                } else {
                    s
                }
            })
            .collect()
    };
    let (i, o) = (fold(inner), fold(outer));
    i.len() >= o.len() && i[..o.len()] == o[..]
}

/// Express `path` relative to the innermost root that contains it.
pub fn to_ref(path: &Path, roots: &[WatchRoot]) -> PathRef {
    let mut best: Option<(usize, usize)> = None;
    for (i, root) in roots.iter().enumerate() {
        if is_within(path, &root.path) {
            let depth = root.path.components().count();
            if best.is_none_or(|(_, d)| depth > d) {
                best = Some((i, depth));
            }
        }
    }
    match best {
        Some((i, depth)) => {
            let rel: Vec<String> = path
                .components()
                .skip(depth)
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            PathRef {
                root: Some(i),
                rel: rel.join("/"),
            }
        }
        None => PathRef {
            root: None,
            rel: path.to_string_lossy().replace('\\', "/"),
        },
    }
}

/// Turn a [`PathRef`] back into an absolute path.
pub fn from_ref(r: &PathRef, roots: &[WatchRoot]) -> PathBuf {
    match r.root.and_then(|i| roots.get(i)) {
        Some(root) => r
            .rel
            .split('/')
            .filter(|p| !p.is_empty())
            .fold(root.path.clone(), |acc, part| acc.join(part)),
        None => PathBuf::from(&r.rel),
    }
}

/// Match `text` against a glob with `*` (any run of characters except `/`),
/// `**` (any run including `/`) and `?` (one character). Case-insensitive on
/// macOS and Windows, whose default filesystems are.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let fold = |s: &str| -> Vec<char> {
        if cfg!(any(windows, target_os = "macos")) {
            s.to_lowercase().chars().collect()
        } else {
            s.chars().collect()
        }
    };
    glob_rec(&fold(pattern), &fold(text))
}

fn glob_rec(p: &[char], t: &[char]) -> bool {
    match p.first() {
        None => t.is_empty(),
        Some('*') => {
            let double = p.get(1) == Some(&'*');
            let rest = if double { &p[2..] } else { &p[1..] };
            // `**/` may also match nothing at all.
            if double && rest.first() == Some(&'/') && glob_rec(&rest[1..], t) {
                return true;
            }
            for i in 0..=t.len() {
                if glob_rec(rest, &t[i..]) {
                    return true;
                }
                if !double && t.get(i) == Some(&'/') {
                    return false;
                }
            }
            false
        }
        Some('?') => t.first().is_some_and(|c| *c != '/') && glob_rec(&p[1..], &t[1..]),
        Some(c) => t.first() == Some(c) && glob_rec(&p[1..], &t[1..]),
    }
}

/// Does a path match a pattern? Patterns without `/` match the file name in
/// any folder; patterns with `/` match the whole relative path.
pub fn path_matches(pattern: &str, r: &PathRef) -> bool {
    if pattern.contains('/') {
        glob_match(pattern, &r.rel)
    } else {
        glob_match(pattern, r.file_name())
    }
}

/// Short human form of a byte count.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    let digits = if value < 10.0 { 1 } else { 0 };
    format!("{value:.digits$} {}", UNITS.get(unit).unwrap_or(&"TB"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob_match("*.flp", "Song A.flp"));
        assert!(!glob_match("*.flp", "Song A.flp.bak"));
        assert!(glob_match("Project/*.flp", "Project/Song A.flp"));
        assert!(!glob_match("*.flp", "Project/Song A.flp"));
        assert!(glob_match("**/*.flp", "Project/Song A.flp"));
        assert!(glob_match("**/*.flp", "Song A.flp"));
        assert!(glob_match("._*", "._Song A.flp"));
        assert!(glob_match("Song ?.flp", "Song A.flp"));
        assert!(!glob_match("Song ?.flp", "Song AB.flp"));
    }

    #[test]
    fn pattern_without_slash_matches_file_name_anywhere() {
        let r = PathRef {
            root: Some(0),
            rel: "Projects/Backup/Song A (autosave).flp".into(),
        };
        assert!(path_matches("*.flp", &r));
        assert!(path_matches("Projects/Backup/*.flp", &r));
        assert!(!path_matches("Project/*.flp", &r));
    }

    #[test]
    fn refs_round_trip_and_pick_innermost_root() {
        let roots = vec![
            WatchRoot {
                label: "outer".into(),
                path: PathBuf::from("/a"),
            },
            WatchRoot {
                label: "inner".into(),
                path: PathBuf::from("/a/b"),
            },
        ];
        let r = to_ref(Path::new("/a/b/c/d.txt"), &roots);
        assert_eq!(r.root, Some(1));
        assert_eq!(r.rel, "c/d.txt");
        assert_eq!(from_ref(&r, &roots), PathBuf::from("/a/b/c/d.txt"));
        let outside = to_ref(Path::new("/z/y"), &roots);
        assert_eq!(outside.root, None);
    }

    #[test]
    fn typed_paths_are_cleaned() {
        if !cfg!(windows) {
            assert_eq!(
                clean_typed_path("/Users/me/Music/Song\\ A/ "),
                PathBuf::from("/Users/me/Music/Song A")
            );
        }
        assert_eq!(clean_typed_path("'/tmp/x y'"), PathBuf::from("/tmp/x y"));
        assert_eq!(clean_typed_path("\"/tmp/q\""), PathBuf::from("/tmp/q"));
    }

    #[test]
    fn within() {
        assert!(is_within(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(is_within(Path::new("/a/b"), Path::new("/a/b")));
        assert!(!is_within(Path::new("/a/bc"), Path::new("/a/b")));
        assert!(!is_within(Path::new("/a"), Path::new("/a/b")));
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(12), "12 bytes");
        assert_eq!(human_size(1_500), "1.5 KB");
        assert_eq!(human_size(23_000_000), "23 MB");
    }
}
