//! `environment.json`: OS, filesystems, tool and DAW versions.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

use crate::error::ProbeError;
use crate::paths::{WatchRoot, home_dir, is_within};
use crate::protocol::Protocol;
use crate::session::{SessionState, write_atomic};

/// Facts about this computer, collected once per run.
#[derive(Debug, Clone, Default)]
pub struct SystemInfo {
    /// OS description.
    pub os: Value,
    /// Per watched folder: filesystem type and sync hints.
    pub folders: Vec<Value>,
}

impl SystemInfo {
    /// Collect (runs a few small system commands).
    pub fn collect(roots: &[WatchRoot]) -> Self {
        Self {
            os: os_info(),
            folders: roots
                .iter()
                .map(|r| {
                    json!({
                        "label": r.label,
                        "path": r.path.to_string_lossy(),
                        "filesystem": filesystem_type(&r.path),
                        "sync_hint": sync_hint(&r.path),
                    })
                })
                .collect(),
        }
    }
}

/// Write `environment.json`.
pub fn write(
    path: &Path,
    protocol: &Protocol,
    state: &SessionState,
    system: &SystemInfo,
    out_dir: &Path,
) -> Result<(), ProbeError> {
    let mut daw = serde_json::Map::new();
    daw.insert("name".into(), json!(protocol.daw));
    for step in &protocol.steps {
        for q in &step.questions {
            let Some(key) = &q.env else { continue };
            if let Some(v) = state.answer(&format!("{}.{}", step.id, q.id)) {
                daw.insert(key.clone(), json!(v));
            }
        }
    }
    let value = json!({
        "tool": { "name": "fsprobe", "version": crate::TOOL_VERSION },
        "protocol": { "id": protocol.id, "title": protocol.title, "revision": protocol.revision },
        "os": system.os,
        "watched_folders": system.folders,
        "daw": Value::Object(daw),
        "runs": state.runs.iter().map(|r| json!({
            "run": r.run, "started": r.started, "ended": r.ended, "outcome": r.outcome,
        })).collect::<Vec<_>>(),
        "results_folder": out_dir.to_string_lossy(),
    });
    let text = serde_json::to_string_pretty(&value).map_err(|e| ProbeError::ResultsDamaged {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    write_atomic(path, text.as_bytes())
}

/// Run a command and return trimmed stdout, if it worked.
fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn os_info() -> Value {
    let family = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    if cfg!(target_os = "macos") {
        json!({
            "family": family, "arch": arch,
            "name": output("sw_vers", &["-productName"]),
            "version": output("sw_vers", &["-productVersion"]),
            "build": output("sw_vers", &["-buildVersion"]),
        })
    } else if cfg!(windows) {
        json!({
            "family": family, "arch": arch,
            "name": "Windows",
            "version": output("cmd", &["/C", "ver"]),
        })
    } else {
        let pretty = std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                    .map(|v| v.trim_matches('"').to_string())
            });
        json!({
            "family": family, "arch": arch,
            "name": pretty,
            "kernel": output("uname", &["-r"]),
        })
    }
}

/// Filesystem type of the volume holding `path` (e.g. `apfs`, `ntfs`, `ext4`).
pub fn filesystem_type(path: &Path) -> Option<String> {
    let p = path.to_string_lossy().to_string();
    if cfg!(target_os = "macos") {
        // `df -P` names the device; `mount` lists "dev on /mnt (apfs, local, ...)".
        let df = output("df", &["-P", &p])?;
        let device = df.lines().nth(1)?.split_whitespace().next()?.to_string();
        let mounts = output("mount", &[])?;
        mounts.lines().find_map(|l| {
            let rest = l.strip_prefix(&format!("{device} on "))?;
            let open = rest.rfind(" (")?;
            let inner = rest.get(open + 2..)?;
            inner.split([',', ')']).next().map(|s| s.trim().to_string())
        })
    } else if cfg!(windows) {
        let drive: String = p.chars().take(2).collect();
        if !drive.ends_with(':') {
            return None;
        }
        let script = format!("[System.IO.DriveInfo]::new('{drive}\\').DriveFormat");
        output(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", &script],
        )
    } else {
        output("stat", &["-f", "-c", "%T", &p])
    }
}

/// A hint if the folder looks cloud-synced (sync clients add their own file
/// activity and can lock files, which would muddy the results).
pub fn sync_hint(path: &Path) -> Option<String> {
    let s = path.to_string_lossy().to_lowercase();
    for (needle, name) in [
        ("onedrive", "OneDrive"),
        ("dropbox", "Dropbox"),
        ("google drive", "Google Drive"),
        ("mobile documents", "iCloud Drive"),
        ("library/cloudstorage", "a cloud storage app"),
    ] {
        if s.contains(needle) {
            return Some(format!("path suggests {name}"));
        }
    }
    if cfg!(target_os = "macos")
        && let Some(home) = home_dir()
    {
        let icloud_docs = home.join("Library/Mobile Documents/com~apple~CloudDocs/Documents");
        let in_docs =
            is_within(path, &home.join("Documents")) || is_within(path, &home.join("Desktop"));
        if in_docs && icloud_docs.is_dir() {
            return Some("Desktop & Documents may be synced by iCloud Drive".into());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_hints_from_path() {
        assert!(sync_hint(Path::new("/Users/x/OneDrive/Documents/Image-Line")).is_some());
        assert!(sync_hint(Path::new("/srv/music/plain")).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn filesystem_type_is_found_for_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(filesystem_type(dir.path()).is_some());
    }
}
