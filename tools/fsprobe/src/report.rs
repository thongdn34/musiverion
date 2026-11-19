//! Plain-language summaries: the one-line "what the tool saw" after each step
//! and `summary.md`.

use std::fmt::Write as _;

use crate::classify::{ClassifyOptions, FileChange, Observation, Pattern, classify};
use crate::error::ProbeError;
use crate::paths::human_size;
use crate::protocol::Protocol;
use crate::session::{OutFiles, SessionState, StepStatus, write_atomic};
use crate::trace::{PathRef, Record, gaps, read_trace, run_os, step_window};

/// Classify one step from the full trace (`None` if it never finished).
pub fn observe_step(
    records: &[Record],
    step_id: &str,
    opts: &ClassifyOptions,
) -> Option<Observation> {
    let window = step_window(records, step_id)?;
    Some(classify(
        window,
        &opts.clone().for_os(run_os(records, window)),
    ))
}

/// How a path is shown: plain inside the first watched folder, prefixed with
/// the folder's label elsewhere.
pub fn show_path(p: &PathRef, labels: &[String]) -> String {
    match p.root {
        Some(0) => p.rel.clone(),
        Some(i) => format!(
            "{}: {}",
            labels.get(i).map(String::as_str).unwrap_or("folder"),
            p.rel
        ),
        None => p.rel.clone(),
    }
}

/// One plain-language sentence for one changed file.
pub fn describe_change(c: &FileChange, labels: &[String]) -> String {
    let p = format!("`{}`", show_path(&c.path, labels));
    let other = |r: &Option<PathRef>| {
        r.as_ref()
            .map(|r| format!("`{}`", show_path(r, labels)))
            .unwrap_or_else(|| "?".into())
    };
    let mut s = match c.pattern {
        Pattern::TempThenRename if c.temp_outside => {
            format!("{p} was replaced by a file written somewhere else and moved into place")
        }
        Pattern::TempThenRename => {
            let verb = if c.existed_before {
                "replaced"
            } else {
                "created"
            };
            format!(
                "{p} was {verb} via a temporary file: {} -> {p}",
                other(&c.temp)
            )
        }
        Pattern::InPlaceWrite => format!("{p} was overwritten in place (same file, new contents)"),
        Pattern::DeleteThenCreate => format!("{p} was deleted and then written again from scratch"),
        Pattern::MovedAsideThenCreate => format!(
            "the old {p} was moved to {}, then a new {p} was written",
            other(&c.moved_to)
        ),
        Pattern::NewFile if c.dir => format!("new folder {p}"),
        Pattern::NewFile if c.renamed_from.is_some() => format!(
            "new file {p} (the previous version of {})",
            other(&c.renamed_from)
        ),
        Pattern::NewFile => match c.size_after {
            Some(size) => format!("new file {p} ({})", human_size(size)),
            None => format!("new file {p}"),
        },
        Pattern::Renamed => format!("{} was renamed to {p}", other(&c.renamed_from)),
        Pattern::Removed if c.moved_to.is_some() => {
            format!("{p} was moved to {}", other(&c.moved_to))
        }
        Pattern::Removed if c.dir => format!("folder {p} was deleted"),
        Pattern::Removed => format!("{p} was deleted"),
        Pattern::MetadataOnly => format!("{p}: only its attributes changed"),
        Pattern::Transient if c.dir => format!("temporary folder {p} appeared and disappeared"),
        Pattern::Transient => format!("temporary file {p} appeared and disappeared"),
        Pattern::Unchanged => format!("{p} was touched but not changed"),
        Pattern::Unclassified => format!(
            "{p} changed in a way the tool does not recognise ({})",
            c.evidence
        ),
    };
    if c.times > 1 {
        let _ = write!(s, " ({} times)", c.times);
    }
    if c.readonly_changed {
        match c.readonly_after {
            Some(false) => s.push_str("; its read-only flag was removed"),
            Some(true) => s.push_str("; it became read-only"),
            None => {}
        }
    } else if c.readonly_after == Some(true) && c.pattern != Pattern::MetadataOnly {
        s.push_str("; still read-only");
    }
    if let Some(ms) = c.settle_ms
        && ms >= 500
    {
        let _ = write!(s, "; took {:.1} s to finish writing", ms as f64 / 1000.0);
    }
    s
}

/// The line used when nothing changed.
pub const NO_CHANGES: &str = "no file changes in the watched folders.";

/// The "what the tool saw" line printed after a step.
pub fn one_line(obs: &Observation, labels: &[String]) -> String {
    let shown: Vec<&FileChange> = obs
        .files
        .iter()
        .filter(|c| c.pattern != Pattern::Unchanged)
        .collect();
    let mut s = if shown.is_empty() {
        NO_CHANGES.to_string()
    } else {
        let mut parts: Vec<String> = shown
            .iter()
            .take(3)
            .map(|c| describe_change(c, labels))
            .collect();
        if shown.len() > 3 {
            parts.push(format!("and {} more (see summary.md)", shown.len() - 3));
        }
        format!("{}.", parts.join("; "))
    };
    if obs.missed_events {
        s.push_str(
            " (The system reported that it dropped some events, so this may be incomplete.)",
        );
    }
    s
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', "<br>")
}

fn yes_no(b: Option<bool>) -> &'static str {
    match b {
        Some(true) => "yes",
        Some(false) => "no",
        None => "-",
    }
}

/// Regenerate `summary.md` from `trace.jsonl` and `answers.json`.
pub fn write_summary(
    files: &OutFiles,
    protocol: &Protocol,
    state: &SessionState,
) -> Result<(), ProbeError> {
    let (records, damaged) = read_trace(&files.trace)?;
    let text = render_summary(protocol, state, &records, damaged);
    write_atomic(&files.summary, text.as_bytes())
}

/// Build `summary.md`.
pub fn render_summary(
    protocol: &Protocol,
    state: &SessionState,
    records: &[Record],
    damaged: usize,
) -> String {
    let labels: Vec<String> = state.watch.iter().map(|w| w.label.clone()).collect();
    let opts = ClassifyOptions {
        ignore: protocol.settings.ignore.clone(),
        ..ClassifyOptions::default()
    };
    let mut md = String::new();
    let _ = writeln!(md, "# fsprobe results: {}\n", protocol.title);
    let _ = writeln!(md, "- DAW: {}", protocol.daw);
    let _ = writeln!(
        md,
        "- Protocol: `{}`{}",
        protocol.id,
        protocol
            .revision
            .as_deref()
            .map(|r| format!(" (revision {r})"))
            .unwrap_or_default()
    );
    let _ = writeln!(md, "- Tool: fsprobe {}", crate::TOOL_VERSION);
    if let (Some(first), Some(last)) = (state.runs.first(), state.runs.last()) {
        let _ = writeln!(
            md,
            "- Sessions: {} (first started {}, last ended {})",
            state.runs.len(),
            first.started,
            last.ended.as_deref().unwrap_or("not yet")
        );
    }
    let _ = writeln!(md, "- Watched folders:");
    for (i, w) in state.watch.iter().enumerate() {
        let note = if i == 0 {
            " (paths below are relative to this folder)"
        } else {
            ""
        };
        let _ = writeln!(md, "  - {}: `{}`{note}", w.label, w.path);
    }
    if damaged > 0 {
        let _ = writeln!(
            md,
            "- Note: {damaged} damaged line(s) in trace.jsonl were skipped."
        );
    }
    let _ = writeln!(
        md,
        "\nSee `environment.json` for OS, filesystem and DAW version details.\n"
    );

    let _ = writeln!(md, "## Overview\n");
    let _ = writeln!(
        md,
        "| Step | What you did | What the tool saw | Your answers |"
    );
    let _ = writeln!(md, "|---|---|---|---|");
    let mut observations = Vec::new();
    for (i, step) in protocol.steps.iter().enumerate() {
        let label = protocol.step_label(i);
        let result = state.steps.get(&step.id);
        let obs = observe_step(records, &step.id, &opts);
        let seen = match (result.map(|r| r.status), &obs) {
            (None, _) => "not done yet".to_string(),
            (Some(StepStatus::NotApplicable), _) => "not applicable".to_string(),
            (Some(StepStatus::Skipped), None) => "skipped".to_string(),
            (_, Some(o)) => one_line(o, &labels),
            (_, None) => "-".to_string(),
        };
        let answers = result
            .map(|r| {
                r.answers
                    .iter()
                    .map(|a| format!("{}: {}", a.id, a.value.as_deref().unwrap_or("(skipped)")))
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default();
        let _ = writeln!(
            md,
            "| {label} | {} | {} | {} |",
            cell(&step.title),
            cell(&seen),
            cell(&answers)
        );
        observations.push(obs);
    }

    let between: Vec<(String, String)> = gaps(records)
        .into_iter()
        .filter_map(|(after, window)| {
            let obs = classify(window, &opts.clone().for_os(run_os(records, window)));
            if obs.files.iter().all(|c| c.pattern == Pattern::Unchanged) {
                return None;
            }
            let when = match after {
                Some(id) => match protocol.steps.iter().position(|s| s.id == id) {
                    Some(i) => format!("after step {}", protocol.step_label(i)),
                    None => format!("after step {id}"),
                },
                None => "before the first step of a session".to_string(),
            };
            Some((when, one_line(&obs, &labels)))
        })
        .collect();
    if !between.is_empty() {
        let _ = writeln!(
            md,
            "\n## Between steps\n\nChanges seen while no step was running (for example autosaves while you read or answered questions).\n"
        );
        let _ = writeln!(md, "| When | What the tool saw |");
        let _ = writeln!(md, "|---|---|");
        for (when, seen) in &between {
            let _ = writeln!(md, "| {} | {} |", cell(when), cell(seen));
        }
    }

    let _ = writeln!(md, "\n## Step details");
    for (i, step) in protocol.steps.iter().enumerate() {
        let label = protocol.step_label(i);
        let _ = writeln!(md, "\n### Step {label}: {}\n", step.title);
        if let Some(p) = &step.purpose {
            let _ = writeln!(md, "Why: {}\n", p.trim());
        }
        let Some(result) = state.steps.get(&step.id) else {
            let _ = writeln!(md, "Not done yet.");
            continue;
        };
        let status = match result.status {
            StepStatus::Done => "done",
            StepStatus::Skipped => "skipped",
            StepStatus::NotApplicable => "not applicable (its condition did not match)",
        };
        let _ = writeln!(
            md,
            "Status: {status}{}{}\n",
            result
                .started
                .as_deref()
                .map(|s| format!(", started {s}"))
                .unwrap_or_default(),
            result
                .finished
                .as_deref()
                .map(|s| format!(", finished {s}"))
                .unwrap_or_default()
        );
        if let Some(why) = &result.skip_reason {
            let _ = writeln!(md, "Reason given for skipping: {}\n", why);
        }
        if let Some(Some(obs)) = observations.get(i) {
            render_observation(&mut md, obs, &labels);
        }
        if !result.actions.is_empty() {
            let _ = writeln!(md, "Tool actions:\n");
            for a in &result.actions {
                let mark = if a.ok { "" } else { " (did not work)" };
                let _ = writeln!(
                    md,
                    "- `{}` ({}){mark}: {}",
                    a.action,
                    a.when,
                    a.message.replace('\n', "; ")
                );
            }
            let _ = writeln!(md);
        }
        if !result.answers.is_empty() {
            let _ = writeln!(md, "Answers:\n");
            for a in &result.answers {
                let _ = writeln!(
                    md,
                    "- {} **{}**",
                    a.ask.replace('\n', " "),
                    a.value.as_deref().unwrap_or("(skipped)")
                );
            }
        }
    }
    md
}

fn render_observation(md: &mut String, obs: &Observation, labels: &[String]) {
    let _ = writeln!(
        md,
        "Events: {} from the system, {} caused by the tool's own actions (left out below).{}\n",
        obs.events,
        obs.tool_events,
        if obs.missed_events {
            " The system reported dropped events."
        } else {
            ""
        }
    );
    for e in &obs.watch_errors {
        let _ = writeln!(md, "Watcher error: {e}\n");
    }
    let files: Vec<&FileChange> = obs
        .files
        .iter()
        .filter(|c| c.pattern != Pattern::Unchanged)
        .collect();
    if files.is_empty() {
        let _ = writeln!(md, "No file changes.\n");
        return;
    }
    let _ = writeln!(
        md,
        "| File | Pattern | Detail | Size before -> after | Read-only before -> after | Settle |"
    );
    let _ = writeln!(md, "|---|---|---|---|---|---|");
    for c in &files {
        let size = |s: Option<u64>| s.map(human_size).unwrap_or_else(|| "-".into());
        let mut detail = Vec::new();
        if let Some(t) = &c.temp_pattern {
            detail.push(format!("temp name `{t}`"));
        }
        if let Some(t) = &c.temp {
            detail.push(format!("temp `{}`", show_path(t, labels)));
        }
        if let Some(m) = &c.moved_to {
            detail.push(format!("old file moved to `{}`", show_path(m, labels)));
        }
        if let Some(r) = &c.renamed_from {
            detail.push(format!("from `{}`", show_path(r, labels)));
        }
        if c.times > 1 {
            detail.push(format!("{} times", c.times));
        }
        detail.push(c.evidence.clone());
        let _ = writeln!(
            md,
            "| `{}`{} | {} | {} | {} -> {} | {} -> {} | {} |",
            cell(&show_path(&c.path, labels)),
            if c.dir { " (folder)" } else { "" },
            c.pattern.as_str(),
            cell(&detail.join("; ")),
            size(c.size_before),
            size(c.size_after),
            yes_no(c.readonly_before),
            yes_no(c.readonly_after),
            c.settle_ms
                .map(|m| format!("{m} ms"))
                .unwrap_or_else(|| "-".into())
        );
    }
    let _ = writeln!(md);
    for c in files.iter().filter(|c| c.pattern == Pattern::Unclassified) {
        let _ = writeln!(
            md,
            "Raw sequence for `{}`:\n\n```",
            show_path(&c.path, labels)
        );
        for line in &c.sequence {
            let _ = writeln!(md, "{line}");
        }
        let _ = writeln!(md, "```\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(pattern: Pattern) -> FileChange {
        FileChange {
            path: PathRef {
                root: Some(0),
                rel: "Project/Song A.flp".into(),
            },
            dir: false,
            pattern,
            existed_before: true,
            exists_after: true,
            temp: Some(PathRef {
                root: Some(0),
                rel: "Project/Song A.flp.tmp".into(),
            }),
            temp_pattern: Some("{name}.tmp".into()),
            temp_outside: false,
            renamed_from: None,
            moved_to: None,
            size_before: Some(1000),
            size_after: Some(2000),
            readonly_before: Some(false),
            readonly_after: Some(false),
            readonly_changed: false,
            settle_ms: Some(1500),
            times: 1,
            evidence: String::new(),
            sequence: vec![],
        }
    }

    #[test]
    fn temp_rename_sentence() {
        let s = describe_change(&change(Pattern::TempThenRename), &[]);
        assert_eq!(
            s,
            "`Project/Song A.flp` was replaced via a temporary file: `Project/Song A.flp.tmp` -> `Project/Song A.flp`; took 1.5 s to finish writing"
        );
    }

    #[test]
    fn other_roots_are_labelled() {
        let p = PathRef {
            root: Some(1),
            rel: "Projects/Backup/x.flp".into(),
        };
        assert_eq!(
            show_path(&p, &["Test".into(), "FL Studio data".into()]),
            "FL Studio data: Projects/Backup/x.flp"
        );
    }

    #[test]
    fn empty_observation_line() {
        assert_eq!(
            one_line(&Observation::default(), &[]),
            "no file changes in the watched folders."
        );
    }
}
