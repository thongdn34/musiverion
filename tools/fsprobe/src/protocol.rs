//! Protocol files: the steps of a guided session, as TOML data.
//!
//! The schema is documented for protocol authors in `protocols/README.md`.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::Deserialize;

use crate::error::ProbeError;

/// Schema version this tool understands.
pub const SCHEMA: u32 = 1;

/// A whole protocol.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Protocol {
    /// Must be [`SCHEMA`].
    pub schema: u32,
    /// Stable id, e.g. `fl-studio`. Results folders are tied to it.
    pub id: String,
    /// Title shown at the start.
    pub title: String,
    /// The DAW under test, e.g. `FL Studio`.
    pub daw: String,
    /// Protocol revision, recorded in the results.
    #[serde(default)]
    pub revision: Option<String>,
    /// Shown once at the start of a new session.
    #[serde(default)]
    pub intro: String,
    /// Shown when every step is done.
    #[serde(default)]
    pub outro: String,
    /// Timing and filtering.
    #[serde(default)]
    pub settings: Settings,
    /// Folders to watch, asked for at the start when `--watch` is not given.
    #[serde(default)]
    pub watch: Vec<WatchSpec>,
    /// Named files that actions refer to.
    #[serde(default)]
    pub targets: BTreeMap<String, TargetSpec>,
    /// The steps, in order.
    pub steps: Vec<Step>,
}

/// Timing and filtering settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Nothing may change for this long before a step's questions are asked.
    #[serde(default = "default_quiet_ms")]
    pub quiet_ms: u64,
    /// Stop waiting for quiet after this long and carry on.
    #[serde(default = "default_max_wait_ms")]
    pub max_wait_ms: u64,
    /// File names (or relative-path globs) left out of summaries.
    #[serde(default)]
    pub ignore: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            quiet_ms: default_quiet_ms(),
            max_wait_ms: default_max_wait_ms(),
            ignore: Vec::new(),
        }
    }
}

fn default_quiet_ms() -> u64 {
    3000
}

fn default_max_wait_ms() -> u64 {
    120_000
}

fn default_true() -> bool {
    true
}

/// A folder to watch.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchSpec {
    /// Id used by targets and `{watch.ID}` placeholders.
    pub id: String,
    /// Short label ("Test folder").
    pub label: String,
    /// Why this folder is watched and how to find it.
    #[serde(default)]
    pub explain: String,
    /// Suggested path (`~` allowed), used when the OS-specific one is absent.
    #[serde(default)]
    pub suggest: Option<String>,
    /// Suggested path on macOS.
    #[serde(default)]
    pub suggest_macos: Option<String>,
    /// Suggested path on Windows.
    #[serde(default)]
    pub suggest_windows: Option<String>,
    /// Suggested path on Linux.
    #[serde(default)]
    pub suggest_linux: Option<String>,
    /// Offer to create the folder (and `subfolders`) if it does not exist.
    #[serde(default)]
    pub create: bool,
    /// Subfolders to create with it.
    #[serde(default)]
    pub subfolders: Vec<String>,
    /// Warn if the folder already contains files.
    #[serde(default)]
    pub expect_empty: bool,
    /// The user may skip this folder.
    #[serde(default)]
    pub optional: bool,
}

impl WatchSpec {
    /// The suggestion for the current OS.
    pub fn suggestion(&self) -> Option<&str> {
        let os = if cfg!(target_os = "macos") {
            &self.suggest_macos
        } else if cfg!(windows) {
            &self.suggest_windows
        } else {
            &self.suggest_linux
        };
        os.as_deref().or(self.suggest.as_deref())
    }
}

/// A named file that actions act on.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetSpec {
    /// How the file is described to the user ("your test project file").
    pub label: String,
    /// Glob relative to the watched folders (`Project/*.flp`; no `/` = any folder).
    #[serde(default)]
    pub pattern: Option<String>,
    /// Restrict `pattern` to one watched folder (its id).
    #[serde(default)]
    pub root: Option<String>,
    /// Preselect a match with exactly this file name.
    #[serde(default)]
    pub prefer: Option<String>,
    /// Absolute globs outside the watched folders (`~` allowed), for `copy_in`.
    #[serde(default)]
    pub search: Vec<String>,
}

/// One step.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Stable id (answers are stored under it).
    pub id: String,
    /// Label shown as "Step 10b"; defaults to the position.
    #[serde(default)]
    pub number: Option<String>,
    /// Title.
    pub title: String,
    /// What the user should do. Placeholders: `{mod}`, `{opt}`, `{watch.ID}`.
    #[serde(default)]
    pub text: String,
    /// Extra text on macOS.
    #[serde(default)]
    pub macos: Option<String>,
    /// Extra text on Windows.
    #[serde(default)]
    pub windows: Option<String>,
    /// Extra text on Linux.
    #[serde(default)]
    pub linux: Option<String>,
    /// Why the step exists (shown in `summary.md`, not to the user).
    #[serde(default)]
    pub purpose: Option<String>,
    /// Marked optional when shown.
    #[serde(default)]
    pub optional: bool,
    /// `true`: the user does something and presses Enter when finished, and
    /// the tool then waits for the files to settle. `false`: questions only.
    #[serde(default = "default_true")]
    pub user_action: bool,
    /// Replaces "Press Enter to start this step".
    #[serde(default)]
    pub start_prompt: Option<String>,
    /// Replaces "Press Enter when you have finished".
    #[serde(default)]
    pub done_prompt: Option<String>,
    /// Only run the step when earlier answers match (`"s00.trial" = "yes"`).
    #[serde(default)]
    pub when: BTreeMap<String, String>,
    /// Leave the step out when any of these earlier answers match.
    #[serde(default)]
    pub unless: BTreeMap<String, String>,
    /// Helper actions run right after the step starts.
    #[serde(default)]
    pub before: Vec<ActionSpec>,
    /// Helper actions run after the user finished and the files settled.
    #[serde(default)]
    pub after: Vec<ActionSpec>,
    /// Questions asked at the end of the step.
    #[serde(default)]
    pub questions: Vec<Question>,
    /// Overrides `settings.quiet_ms` for this step.
    #[serde(default)]
    pub quiet_ms: Option<u64>,
}

impl Step {
    /// Instructions for the current OS with placeholders filled in.
    pub fn instructions(&self, vars: &dyn Fn(&str) -> Option<String>) -> String {
        let extra = if cfg!(target_os = "macos") {
            &self.macos
        } else if cfg!(windows) {
            &self.windows
        } else {
            &self.linux
        };
        let mut text = self.text.trim_end().to_string();
        if let Some(extra) = extra {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(extra.trim_end());
        }
        fill(&text, vars)
    }
}

/// Replace `{name}` placeholders using `vars`; unknown ones stay as written.
pub fn fill(text: &str, vars: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) if !after[..close].contains(['{', ' ', '\n']) => {
                let name = &after[..close];
                match vars(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Standard placeholders for the current OS.
pub fn os_var(name: &str) -> Option<String> {
    let mac = cfg!(target_os = "macos");
    match name {
        "mod" => Some(if mac { "Cmd" } else { "Ctrl" }.into()),
        "opt" => Some(if mac { "Option" } else { "Alt" }.into()),
        "os" => Some(
            if mac {
                "macOS"
            } else if cfg!(windows) {
                "Windows"
            } else {
                "Linux"
            }
            .into(),
        ),
        _ => None,
    }
}

/// A helper action.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSpec {
    /// Which action.
    pub action: ActionKind,
    /// A named target from `[targets]`.
    #[serde(default)]
    pub target: Option<String>,
    /// An inline glob instead of a named target.
    #[serde(default)]
    pub pattern: Option<String>,
    /// Backup slot name for `backup_file` / `restore_file` / `compare_with_backup`.
    #[serde(default)]
    pub slot: Option<String>,
    /// `restore_file`: `replace` (delete, then write the copy; like git) or
    /// `overwrite` (rewrite the same file in place).
    #[serde(default)]
    pub method: Option<RestoreMethod>,
    /// `wait`: how long.
    #[serde(default)]
    pub seconds: Option<u64>,
    /// `wait`: what to tell the user while waiting.
    #[serde(default)]
    pub message: Option<String>,
    /// `copy_in`: destination path relative to the watched folder `root`.
    #[serde(default)]
    pub to: Option<String>,
    /// `copy_in`: watched folder id of the destination.
    #[serde(default)]
    pub root: Option<String>,
}

/// The helper actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Make a file read-only (like an unlocked Git LFS `lockable` file).
    SetReadonly,
    /// Make a file writable again.
    ClearReadonly,
    /// List programs with files open in the watched folders.
    ListOpenHandles,
    /// Save a listing of every file in the watched folders.
    SnapshotTree,
    /// Copy a file aside (outside the results folder).
    BackupFile,
    /// Put a copy taken with `backup_file` back over the original.
    RestoreFile,
    /// Tell whether a file still equals the copy taken with `backup_file`.
    CompareWithBackup,
    /// Show size, read-only flag and modification time.
    StatFile,
    /// Count down while the DAW works (Enter ends early).
    Wait,
    /// Copy a file from outside the watched folders into one.
    CopyIn,
}

impl ActionKind {
    /// Snake-case name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SetReadonly => "set_readonly",
            Self::ClearReadonly => "clear_readonly",
            Self::ListOpenHandles => "list_open_handles",
            Self::SnapshotTree => "snapshot_tree",
            Self::BackupFile => "backup_file",
            Self::RestoreFile => "restore_file",
            Self::CompareWithBackup => "compare_with_backup",
            Self::StatFile => "stat_file",
            Self::Wait => "wait",
            Self::CopyIn => "copy_in",
        }
    }

    /// Changes files in the watched folders (its events are tagged and left
    /// out of the step's classification).
    pub fn modifies_files(self) -> bool {
        matches!(
            self,
            Self::SetReadonly | Self::ClearReadonly | Self::RestoreFile | Self::CopyIn
        )
    }

    /// Reads files in the watched folders (tagged too, since reading shows up
    /// as access events on Linux).
    pub fn reads_files(self) -> bool {
        matches!(self, Self::BackupFile | Self::CompareWithBackup)
    }

    fn needs_file(self) -> bool {
        matches!(
            self,
            Self::SetReadonly
                | Self::ClearReadonly
                | Self::BackupFile
                | Self::StatFile
                | Self::CopyIn
        )
    }
}

/// How `restore_file` writes the copy back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RestoreMethod {
    /// Delete the file, then write the copy as a new file (what git does).
    #[default]
    Replace,
    /// Rewrite the existing file in place.
    Overwrite,
}

/// A question.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Question {
    /// Id, unique within the step.
    pub id: String,
    /// The question text (placeholders allowed).
    pub ask: String,
    /// Answer type.
    #[serde(rename = "type")]
    pub kind: QuestionKind,
    /// Choices for `choice`.
    #[serde(default)]
    pub options: Vec<String>,
    /// `choice`: also offer "something else" with free text.
    #[serde(default)]
    pub other: bool,
    /// Only ask when earlier answers match (`warned = "yes"`, or `"s00.trial" = "yes"`).
    #[serde(default)]
    pub when: BTreeMap<String, String>,
    /// Copy the answer into `environment.json` under `daw.<env>`.
    #[serde(default)]
    pub env: Option<String>,
}

/// Answer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    /// yes / no.
    YesNo,
    /// One of `options`.
    Choice,
    /// Free text.
    Text,
}

impl Protocol {
    /// Read and check a protocol file.
    pub fn load(path: &Path) -> Result<Self, ProbeError> {
        let text = std::fs::read_to_string(path).map_err(|source| ProbeError::ProtocolRead {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text).map_err(|message| ProbeError::ProtocolInvalid {
            path: path.to_path_buf(),
            message,
        })
    }

    /// Parse and check protocol text.
    pub fn parse(text: &str) -> Result<Self, String> {
        let p: Protocol = toml::from_str(text).map_err(|e| e.to_string())?;
        p.validate()?;
        Ok(p)
    }

    /// Structural checks beyond what the TOML types enforce.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SCHEMA {
            return Err(format!(
                "schema = {} but this tool understands schema = {SCHEMA}",
                self.schema
            ));
        }
        if self.steps.is_empty() {
            return Err("there are no [[steps]]".into());
        }
        let mut watch_ids = HashSet::new();
        for w in &self.watch {
            if !watch_ids.insert(w.id.as_str()) {
                return Err(format!("watch id '{}' is used twice", w.id));
            }
        }
        for (name, t) in &self.targets {
            if t.pattern.is_none() && t.search.is_empty() {
                return Err(format!("target '{name}' needs `pattern` or `search`"));
            }
            if let Some(r) = &t.root
                && !watch_ids.contains(r.as_str())
            {
                return Err(format!("target '{name}' uses unknown watch id '{r}'"));
            }
        }
        // Answers seen so far, as "step.question".
        let mut known: HashSet<String> = HashSet::new();
        let mut step_ids = HashSet::new();
        for step in &self.steps {
            let sid = &step.id;
            if sid.contains('.') || sid.is_empty() {
                return Err(format!(
                    "step id '{sid}' must be non-empty and contain no '.'"
                ));
            }
            if !step_ids.insert(sid.as_str()) {
                return Err(format!("step id '{sid}' is used twice"));
            }
            for key in step.when.keys().chain(step.unless.keys()) {
                if !known.contains(key) {
                    return Err(format!(
                        "step '{sid}': `when`/`unless` refers to '{key}', which is not an earlier question (use \"step.question\")"
                    ));
                }
            }
            for a in step.before.iter().chain(&step.after) {
                self.validate_action(sid, a, &watch_ids)?;
            }
            let mut local = HashSet::new();
            for q in &step.questions {
                if !local.insert(q.id.as_str()) {
                    return Err(format!(
                        "step '{sid}': question id '{}' is used twice",
                        q.id
                    ));
                }
                if q.kind == QuestionKind::Choice && q.options.is_empty() {
                    return Err(format!(
                        "step '{sid}': question '{}' is a choice without options",
                        q.id
                    ));
                }
                for key in q.when.keys() {
                    let full = if key.contains('.') {
                        key.clone()
                    } else {
                        format!("{sid}.{key}")
                    };
                    if !known.contains(&full) {
                        return Err(format!(
                            "step '{sid}': question '{}' depends on '{key}', which is not asked before it",
                            q.id
                        ));
                    }
                }
                known.insert(format!("{sid}.{}", q.id));
            }
        }
        Ok(())
    }

    fn validate_action(
        &self,
        sid: &str,
        a: &ActionSpec,
        watch_ids: &HashSet<&str>,
    ) -> Result<(), String> {
        let name = a.action.as_str();
        if let Some(t) = &a.target
            && !self.targets.contains_key(t)
        {
            return Err(format!("step '{sid}': {name} uses unknown target '{t}'"));
        }
        if a.action.needs_file() && a.target.is_none() && a.pattern.is_none() {
            return Err(format!("step '{sid}': {name} needs `target` or `pattern`"));
        }
        match a.action {
            ActionKind::Wait if a.seconds.is_none() => {
                Err(format!("step '{sid}': wait needs `seconds`"))
            }
            ActionKind::CopyIn => match (&a.to, &a.root) {
                (Some(_), Some(r)) if watch_ids.contains(r.as_str()) => Ok(()),
                _ => Err(format!(
                    "step '{sid}': copy_in needs `to` and a `root` that is a watch id"
                )),
            },
            _ => Ok(()),
        }
    }

    /// Display label for step `i` (0-based).
    pub fn step_label(&self, i: usize) -> String {
        self.steps
            .get(i)
            .and_then(|s| s.number.clone())
            .unwrap_or_else(|| (i + 1).to_string())
    }

    /// The "of N" shown with step labels: the last step's number without
    /// sub-step letters ("15" for 7a, 7b, ... 15), or the step count.
    pub fn steps_total(&self) -> String {
        let last = self
            .steps
            .last()
            .and_then(|s| s.number.as_deref())
            .map(|n| {
                n.trim_end_matches(|c: char| c.is_ascii_alphabetic())
                    .to_string()
            })
            .filter(|n| !n.is_empty());
        last.unwrap_or_else(|| self.steps.len().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
schema = 1
id = "t"
title = "T"
daw = "Test DAW"

[targets.project]
label = "project"
pattern = "*.prj"

[[steps]]
id = "a"
title = "A"
text = "Press {mod}+S in {watch.test}"
after = [{ action = "set_readonly", target = "project" }]

[[steps.questions]]
id = "ok"
ask = "OK?"
type = "yes_no"

[[steps.questions]]
id = "why"
ask = "Why?"
type = "text"
when = { ok = "no" }

[[steps]]
id = "b"
title = "B"
when = { "a.ok" = "yes" }
"#;

    #[test]
    fn minimal_protocol_parses() {
        let p = Protocol::parse(MINIMAL).unwrap();
        assert_eq!(p.steps.len(), 2);
        assert_eq!(p.settings.quiet_ms, 3000);
        assert!(p.steps[1].user_action);
        assert_eq!(p.step_label(1), "2");
    }

    #[test]
    fn unknown_fields_and_bad_references_are_rejected() {
        let bad = MINIMAL.replace("type = \"yes_no\"", "type = \"yes_no\"\ncolour = 1");
        assert!(Protocol::parse(&bad).unwrap_err().contains("colour"));
        let bad = MINIMAL.replace("target = \"project\"", "target = \"nope\"");
        assert!(
            Protocol::parse(&bad)
                .unwrap_err()
                .contains("unknown target")
        );
        let bad = MINIMAL.replace("\"a.ok\"", "\"a.nope\"");
        assert!(Protocol::parse(&bad).unwrap_err().contains("a.nope"));
        let bad = MINIMAL.replace("schema = 1", "schema = 2");
        assert!(Protocol::parse(&bad).unwrap_err().contains("schema"));
    }

    #[test]
    fn placeholders_fill() {
        let vars = |n: &str| match n {
            "mod" => Some("Cmd".to_string()),
            _ => None,
        };
        assert_eq!(
            fill("Press {mod}+S {x} {not closed", &vars),
            "Press Cmd+S {x} {not closed"
        );
        assert_eq!(fill("{ spaced }", &vars), "{ spaced }");
    }

    #[test]
    fn shipped_protocols_are_valid() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("protocols");
        let mut n = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "toml") {
                Protocol::load(&path).unwrap_or_else(|e| panic!("{e}"));
                n += 1;
            }
        }
        assert!(n >= 1);
    }
}
