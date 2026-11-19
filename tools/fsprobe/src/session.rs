//! The guided session: folder setup, steps, questions, resume.
//!
//! Progress lives in `answers.json`, rewritten after every step, so a session
//! can be paused (type `pause`, close the window, Ctrl+C) and continued by
//! running the same command again: it picks up at the first step without an
//! answer.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::actions::{self, Ctx, is_skip};
use crate::classify::ClassifyOptions;
use crate::console::{Console, heading, say};
use crate::environment::{self, SystemInfo};
use crate::error::ProbeError;
use crate::paths::{WatchRoot, clean_typed_path, expand_tilde, is_within, resolve_dir};
use crate::protocol::{self, Protocol, Question, QuestionKind, Step, WatchSpec};
use crate::recorder::{Recorder, RecorderOptions};
use crate::report;
use crate::trace::{Body, Phase, read_trace, wall_now};

/// Current `answers.json` format.
pub const STATE_FORMAT: u32 = 1;

/// Files in a results folder.
#[derive(Debug, Clone)]
pub struct OutFiles {
    /// The results folder.
    pub dir: PathBuf,
    /// `trace.jsonl`.
    pub trace: PathBuf,
    /// `answers.json` (also the resume state).
    pub answers: PathBuf,
    /// `summary.md`.
    pub summary: PathBuf,
    /// `environment.json`.
    pub environment: PathBuf,
    /// Copy of the protocol that was run.
    pub protocol: PathBuf,
    /// Safety copies (a sibling folder; not part of the results).
    pub backups: PathBuf,
}

impl OutFiles {
    /// Paths for results folder `dir` (absolute).
    pub fn new(dir: &Path) -> Self {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "results".into());
        Self {
            dir: dir.to_path_buf(),
            trace: dir.join("trace.jsonl"),
            answers: dir.join("answers.json"),
            summary: dir.join("summary.md"),
            environment: dir.join("environment.json"),
            protocol: dir.join("protocol.toml"),
            backups: dir.with_file_name(format!("{name}-backups")),
        }
    }
}

/// `answers.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    /// [`STATE_FORMAT`].
    pub format: u32,
    /// Protocol id the folder belongs to.
    pub protocol_id: String,
    /// Protocol title.
    pub protocol_title: String,
    /// Protocol revision.
    #[serde(default)]
    pub protocol_revision: Option<String>,
    /// DAW under test.
    pub daw: String,
    /// Watched folders.
    pub watch: Vec<SavedRoot>,
    /// One entry per run (session start or resume).
    #[serde(default)]
    pub runs: Vec<RunInfo>,
    /// Remembered files for named targets: name → absolute path.
    #[serde(default)]
    pub targets: BTreeMap<String, String>,
    /// Safety copies by slot.
    #[serde(default)]
    pub backups: BTreeMap<String, BackupInfo>,
    /// Finished steps by id.
    #[serde(default)]
    pub steps: BTreeMap<String, StepResult>,
}

/// A watched folder as saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedRoot {
    /// Protocol watch id (absent when given with `--watch` beyond the protocol's list).
    #[serde(default)]
    pub id: Option<String>,
    /// Label.
    pub label: String,
    /// Absolute path.
    pub path: String,
}

/// One run of the tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInfo {
    /// Run number.
    pub run: u32,
    /// Start time.
    pub started: String,
    /// End time (absent if it was interrupted).
    #[serde(default)]
    pub ended: Option<String>,
    /// How it ended: `finished`, `paused`, `input_ended`, `error`.
    #[serde(default)]
    pub outcome: Option<String>,
}

/// A safety copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// The file that was copied.
    pub original: String,
    /// Where the copy is.
    pub copy: String,
    /// Bytes.
    pub size: u64,
    /// When.
    pub taken: String,
}

/// Status of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    /// Done.
    Done,
    /// The user skipped it.
    Skipped,
    /// Its `when` condition did not match.
    NotApplicable,
}

/// The outcome of a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Status.
    pub status: StepStatus,
    /// Run it happened in.
    pub run: u32,
    /// Start time.
    #[serde(default)]
    pub started: Option<String>,
    /// End time.
    #[serde(default)]
    pub finished: Option<String>,
    /// Answers in the order asked.
    #[serde(default)]
    pub answers: Vec<Answer>,
    /// Helper action results in order.
    #[serde(default)]
    pub actions: Vec<ActionResult>,
    /// One-line summary of what the tool saw.
    #[serde(default)]
    pub seen: Option<String>,
    /// Why it was skipped, if the user said.
    #[serde(default)]
    pub skip_reason: Option<String>,
}

/// One answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    /// Question id.
    pub id: String,
    /// Question text as asked.
    pub ask: String,
    /// The answer (`yes`/`no`, the chosen option, or text).
    #[serde(default)]
    pub value: Option<String>,
    /// The user typed skip (or left a text answer empty).
    #[serde(default)]
    pub skipped: bool,
}

/// One action result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// Action name.
    pub action: String,
    /// Phase: `before` or `after`.
    pub when: String,
    /// It worked.
    pub ok: bool,
    /// What the user was told.
    pub message: String,
}

impl SessionState {
    fn new(p: &Protocol) -> Self {
        Self {
            format: STATE_FORMAT,
            protocol_id: p.id.clone(),
            protocol_title: p.title.clone(),
            protocol_revision: p.revision.clone(),
            daw: p.daw.clone(),
            ..Self::default()
        }
    }

    /// The answer to `step.question`, if given.
    pub fn answer(&self, key: &str) -> Option<&str> {
        let (step, q) = key.split_once('.')?;
        self.steps
            .get(step)?
            .answers
            .iter()
            .find(|a| a.id == q)
            .and_then(|a| a.value.as_deref())
    }

    /// Load `answers.json` if present.
    pub fn load(path: &Path) -> Result<Option<Self>, ProbeError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(ProbeError::io("read", path, e)),
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| ProbeError::ResultsDamaged {
                path: path.to_path_buf(),
                message: e.to_string(),
            })
    }

    /// Write `answers.json` atomically.
    pub fn save(&self, path: &Path) -> Result<(), ProbeError> {
        let text = serde_json::to_string_pretty(self).map_err(|e| ProbeError::ResultsDamaged {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        write_atomic(path, text.as_bytes())
    }

    /// Watched folders as [`WatchRoot`]s.
    pub fn roots(&self) -> Vec<WatchRoot> {
        self.watch
            .iter()
            .map(|w| WatchRoot {
                label: w.label.clone(),
                path: PathBuf::from(&w.path),
            })
            .collect()
    }
}

/// Write a file via a temporary file and rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProbeError> {
    let tmp = path.with_extension("tmp-write");
    std::fs::write(&tmp, bytes).map_err(|e| ProbeError::io("write to", &tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| ProbeError::io("write to", path, e))
}

/// Arguments of `fsprobe run`.
#[derive(Debug, Clone)]
pub struct RunArgs {
    /// Protocol file.
    pub protocol: PathBuf,
    /// Folders from `--watch` (empty: ask).
    pub watch: Vec<PathBuf>,
    /// Results folder.
    pub out: PathBuf,
    /// Recorder timing.
    pub recorder: RecorderOptions,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    /// Every step has an answer.
    Finished,
    /// The user typed pause.
    Paused,
    /// Input ended (window closed or script finished).
    InputEnded,
}

/// Run (or continue) a guided session.
pub fn run(args: &RunArgs, console: &mut Console) -> Result<RunEnd, ProbeError> {
    let protocol = Protocol::load(&args.protocol)?;
    std::fs::create_dir_all(&args.out).map_err(|e| ProbeError::io("create", &args.out, e))?;
    let out_dir = resolve_dir(&args.out).map_err(|_| {
        ProbeError::Usage(format!(
            "cannot use {} as the results folder",
            args.out.display()
        ))
    })?;
    let files = OutFiles::new(&out_dir);

    let mut state = match SessionState::load(&files.answers)? {
        Some(s) => {
            if s.protocol_id != protocol.id {
                return Err(ProbeError::Usage(format!(
                    "the results folder {} belongs to the \"{}\" protocol, not \"{}\". Use a different --out folder.",
                    out_dir.display(),
                    s.protocol_id,
                    protocol.id
                )));
            }
            s
        }
        None => SessionState::new(&protocol),
    };
    let resuming = !state.watch.is_empty();

    if resuming {
        check_saved_roots(&state, &args.watch)?;
    } else {
        heading(&protocol.title);
        if !protocol.intro.trim().is_empty() {
            say(protocol.intro.trim());
            say("");
        }
        say("At any prompt you can type pause to stop and continue later with the same command.");
        let saved = if args.watch.is_empty() {
            ask_watch_folders(&protocol, console, &files)?
        } else {
            roots_from_args(&protocol, &args.watch, &files)?
        };
        state.watch = saved;
        state.protocol_revision = protocol.revision.clone();
        state.save(&files.answers)?;
    }
    let roots = state.roots();
    let watch_ids: Vec<Option<String>> = state.watch.iter().map(|w| w.id.clone()).collect();
    if let Ok(text) = std::fs::read(&args.protocol) {
        let _ = std::fs::write(&files.protocol, text);
    }

    let run_no = u32::try_from(state.runs.len()).unwrap_or(u32::MAX - 1) + 1;
    state.runs.push(RunInfo {
        run: run_no,
        started: wall_now(),
        ended: None,
        outcome: None,
    });
    state.save(&files.answers)?;
    let system = SystemInfo::collect(&roots);
    environment::write(&files.environment, &protocol, &state, &system, &out_dir)?;

    let recorder = Recorder::start(roots.clone(), &files.trace, run_no, args.recorder.clone())?;
    let mut session = Session {
        protocol: &protocol,
        roots: &roots,
        watch_ids: &watch_ids,
        recorder: &recorder,
        console,
        state: &mut state,
        files: &files,
        system: &system,
    };
    let result = session.steps(resuming);
    let outcome = match &result {
        Ok(RunEnd::Finished) => "finished",
        Ok(RunEnd::Paused) | Err(ProbeError::Paused) => "paused",
        Ok(RunEnd::InputEnded) | Err(ProbeError::InputEnded) => "input_ended",
        Err(_) => "error",
    };
    let stop = recorder.stop();
    if let Some(r) = state.runs.last_mut() {
        r.ended = Some(wall_now());
        r.outcome = Some(outcome.to_string());
    }
    state.save(&files.answers)?;
    let _ = environment::write(&files.environment, &protocol, &state, &system, &out_dir);
    report::write_summary(&files, &protocol, &state)?;
    stop?;

    let end = match result {
        Ok(end) => end,
        Err(ProbeError::Paused) => RunEnd::Paused,
        Err(ProbeError::InputEnded) => RunEnd::InputEnded,
        Err(e) => return Err(e),
    };
    match end {
        RunEnd::Finished => finished_message(&protocol, &files),
        RunEnd::Paused | RunEnd::InputEnded => paused_message(&files, end),
    }
    Ok(end)
}

fn finished_message(protocol: &Protocol, files: &OutFiles) {
    heading("All done");
    if !protocol.outro.trim().is_empty() {
        say(protocol.outro.trim());
        say("");
    }
    say("Thank you! Please send back this whole folder (you can zip it first):");
    say("");
    say(&format!("    {}", files.dir.display()));
    say("");
    say("It contains file names, folder paths, timings and your answers.");
    say("It does not contain any audio or the contents of your projects.");
    if files.backups.exists() {
        say("");
        say(&format!(
            "The safety copies the tool made are in {} - you do not need to send them, and you can delete that folder.",
            files.backups.display()
        ));
    }
}

fn paused_message(files: &OutFiles, end: RunEnd) {
    say("");
    if end == RunEnd::InputEnded {
        say("Input ended, so the session stopped here.");
    } else {
        say("Paused.");
    }
    say(&format!(
        "Everything so far is saved in {}.",
        files.dir.display()
    ));
    say(
        "To continue, run the same command again (press the Up arrow key in the terminal to get it back).",
    );
    say("It will pick up at the step you have not finished yet.");
}

/// On resume the watched folders come from `answers.json`.
fn check_saved_roots(state: &SessionState, args: &[PathBuf]) -> Result<(), ProbeError> {
    for w in &state.watch {
        if !Path::new(&w.path).is_dir() {
            return Err(ProbeError::Usage(format!(
                "the folder {} that this session watches no longer exists. Put it back, or start a new session with a different --out folder.",
                w.path
            )));
        }
    }
    if !args.is_empty() {
        let given: Vec<String> = args
            .iter()
            .filter_map(|a| resolve_dir(a).ok())
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        let saved: Vec<String> = state.watch.iter().map(|w| w.path.clone()).collect();
        if given != saved {
            return Err(ProbeError::Usage(format!(
                "this results folder was started with different folders to watch ({}). Run it again without --watch to continue, or use a different --out folder.",
                saved.join(", ")
            )));
        }
    }
    Ok(())
}

/// Why `candidate` cannot be watched, given the results folder and the
/// folders chosen so far.
fn overlap_problem(candidate: &Path, files: &OutFiles, chosen: &[SavedRoot]) -> Option<String> {
    if is_within(&files.dir, candidate) || is_within(&files.backups, candidate) {
        return Some(
            "the results folder is inside this folder, so the tool would record its own writing. Choose a different folder, or put the results somewhere else with --out.".into(),
        );
    }
    if is_within(candidate, &files.dir) {
        return Some("this folder is inside the results folder.".into());
    }
    for c in chosen {
        let other = Path::new(&c.path);
        if is_within(candidate, other) || is_within(other, candidate) {
            return Some(format!(
                "this folder overlaps with \"{}\" ({}), which is already watched.",
                c.label, c.path
            ));
        }
    }
    None
}

fn roots_from_args(
    protocol: &Protocol,
    args: &[PathBuf],
    files: &OutFiles,
) -> Result<Vec<SavedRoot>, ProbeError> {
    let mut saved: Vec<SavedRoot> = Vec::new();
    for (i, a) in args.iter().enumerate() {
        let path = resolve_dir(&expand_tilde(&a.to_string_lossy()))?;
        if let Some(problem) = overlap_problem(&path, files, &saved) {
            return Err(ProbeError::WatchFolder {
                path,
                message: problem,
            });
        }
        let spec = protocol.watch.get(i);
        saved.push(SavedRoot {
            id: spec.map(|s| s.id.clone()),
            label: spec
                .map(|s| s.label.clone())
                .unwrap_or_else(|| format!("Folder {}", i + 1)),
            path: path.to_string_lossy().to_string(),
        });
    }
    Ok(saved)
}

fn ask_watch_folders(
    protocol: &Protocol,
    console: &mut Console,
    files: &OutFiles,
) -> Result<Vec<SavedRoot>, ProbeError> {
    heading("Folders to watch");
    say("The tool watches a few folders and writes down every file that is created, changed,");
    say(
        "renamed or deleted in them, with sizes and times. It never reads or sends the contents of your files.",
    );
    let mut saved = Vec::new();
    for spec in &protocol.watch {
        if let Some(root) = ask_one_folder(spec, console, files, &saved)? {
            saved.push(root);
        }
    }
    if saved.is_empty() {
        return Err(ProbeError::Usage(
            "no folders to watch were chosen, so there is nothing to record.".into(),
        ));
    }
    Ok(saved)
}

fn ask_one_folder(
    spec: &WatchSpec,
    console: &mut Console,
    files: &OutFiles,
    chosen: &[SavedRoot],
) -> Result<Option<SavedRoot>, ProbeError> {
    say("");
    say(&format!("--- {} ---", spec.label));
    if !spec.explain.trim().is_empty() {
        say(spec.explain.trim());
    }
    let suggestion = spec.suggestion().map(expand_tilde);
    loop {
        let skip = if spec.optional {
            " (or type skip if you do not have this folder)"
        } else {
            ""
        };
        let prompt = match &suggestion {
            Some(s) => format!(
                "Press Enter to use {}\nor type (or drag in) a different folder{skip}:",
                s.display()
            ),
            None => {
                format!("Type the folder's location, or drag the folder into this window{skip}:")
            }
        };
        let input = console.ask(&prompt)?;
        if spec.optional && is_skip(&input) {
            say(&format!("Not watching the {}.", spec.label));
            return Ok(None);
        }
        let path = if input.trim().is_empty() {
            match &suggestion {
                Some(s) => s.clone(),
                None => continue,
            }
        } else {
            clean_typed_path(&input)
        };
        if !path.exists() {
            if spec.create {
                let sub = if spec.subfolders.is_empty() {
                    String::new()
                } else {
                    format!(" with the subfolders {}", spec.subfolders.join(", "))
                };
                let yes = ask_yes_no(
                    console,
                    &format!("{} does not exist yet. Create it now{sub}?", path.display()),
                )?;
                if !yes {
                    continue;
                }
                if let Err(e) = std::fs::create_dir_all(&path) {
                    say(&format!(
                        "Could not create it: {e}. Please try another folder."
                    ));
                    continue;
                }
            } else {
                say("I cannot find that folder. Please check the location and try again.");
                continue;
            }
        }
        if spec.create {
            for sub in &spec.subfolders {
                let _ = std::fs::create_dir_all(path.join(sub));
            }
        }
        let resolved = match resolve_dir(&path) {
            Ok(p) => p,
            Err(e) => {
                say(&format!("That does not work: {e}. Please try again."));
                continue;
            }
        };
        if let Some(problem) = overlap_problem(&resolved, files, chosen) {
            say(&format!("That folder cannot be used: {problem}"));
            continue;
        }
        if spec.expect_empty && has_files(&resolved) {
            let yes = ask_yes_no(
                console,
                "This folder already has files in it. An empty folder gives the clearest results. Use it anyway?",
            )?;
            if !yes {
                continue;
            }
        }
        say(&format!("Watching: {}", resolved.display()));
        return Ok(Some(SavedRoot {
            id: Some(spec.id.clone()),
            label: spec.label.clone(),
            path: resolved.to_string_lossy().to_string(),
        }));
    }
}

/// Any file (not folder, not `.DS_Store`) anywhere below `dir`?
fn has_files(dir: &Path) -> bool {
    let mut index = std::collections::HashMap::new();
    crate::recorder::scan(dir, &mut index, 10_000);
    index.iter().any(|(p, s)| {
        !s.dir
            && p.file_name()
                .is_some_and(|n| n != ".DS_Store" && n != "Thumbs.db" && n != "desktop.ini")
    })
}

fn ask_yes_no(console: &mut Console, question: &str) -> Result<bool, ProbeError> {
    loop {
        let input = console.ask(&format!("{question} (yes / no)"))?;
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => say("Please type yes or no."),
        }
    }
}

struct Session<'a> {
    protocol: &'a Protocol,
    roots: &'a [WatchRoot],
    watch_ids: &'a [Option<String>],
    recorder: &'a Recorder,
    console: &'a mut Console,
    state: &'a mut SessionState,
    files: &'a OutFiles,
    system: &'a SystemInfo,
}

impl Session<'_> {
    fn steps(&mut self, resuming: bool) -> Result<RunEnd, ProbeError> {
        let total = self.protocol.steps_total();
        let pending = self
            .protocol
            .steps
            .iter()
            .position(|s| !self.state.steps.contains_key(&s.id));
        match (resuming, pending) {
            (_, None) => {
                say("Every step of this session already has an answer.");
                return Ok(RunEnd::Finished);
            }
            (true, Some(i)) => {
                heading(&self.protocol.title);
                say(&format!(
                    "Welcome back. Continuing at step {} of {total}.",
                    self.protocol.step_label(i)
                ));
            }
            (false, Some(_)) => {
                say("");
                say("Setup is done. The steps start now.");
            }
        }
        for (i, step) in self.protocol.steps.iter().enumerate() {
            if self.state.steps.contains_key(&step.id) {
                continue;
            }
            if !conditions_met(&step.when, self.state, &[], &step.id)
                || any_condition_met(&step.unless, self.state, &step.id)
            {
                self.state.steps.insert(
                    step.id.clone(),
                    StepResult {
                        status: StepStatus::NotApplicable,
                        run: self.run_no(),
                        started: None,
                        finished: None,
                        answers: Vec::new(),
                        actions: Vec::new(),
                        seen: None,
                        skip_reason: None,
                    },
                );
                self.state.save(&self.files.answers)?;
                continue;
            }
            self.step(i, step)?;
        }
        Ok(RunEnd::Finished)
    }

    fn run_no(&self) -> u32 {
        self.state.runs.last().map(|r| r.run).unwrap_or(1)
    }

    fn vars(&self) -> impl Fn(&str) -> Option<String> + '_ {
        move |name: &str| {
            if let Some(v) = protocol::os_var(name) {
                return Some(v);
            }
            if let Some(id) = name.strip_prefix("watch.") {
                return self
                    .state
                    .watch
                    .iter()
                    .find(|w| w.id.as_deref() == Some(id))
                    .map(|w| w.path.clone());
            }
            if let Some(key) = name.strip_prefix("answer.") {
                return Some(
                    self.state
                        .answer(key)
                        .unwrap_or("(not answered)")
                        .to_string(),
                );
            }
            None
        }
    }

    fn step(&mut self, i: usize, step: &Step) -> Result<(), ProbeError> {
        let total = self.protocol.steps_total();
        let optional = if step.optional { " (optional)" } else { "" };
        heading(&format!(
            "Step {} of {total}: {}{optional}",
            self.protocol.step_label(i),
            step.title
        ));
        let vars = self.vars();
        let text = step.instructions(&vars);
        drop(vars);
        if !text.is_empty() {
            say(&text);
        }
        let mut result = StepResult {
            status: StepStatus::Done,
            run: self.run_no(),
            started: None,
            finished: None,
            answers: Vec::new(),
            actions: Vec::new(),
            seen: None,
            skip_reason: None,
        };
        if step.user_action {
            say("");
            let prompt = step.start_prompt.clone().unwrap_or_else(|| {
                let skip = if step.optional {
                    "This step is optional: type skip to leave it out."
                } else {
                    "If you cannot do this step, type skip."
                };
                format!("When you are ready, press Enter to start. ({skip})")
            });
            let input = self.console.ask(&prompt)?;
            if is_skip(&input) {
                return self.skip(step, result);
            }
        }
        result.started = Some(wall_now());
        self.recorder.marker(&step.id, Phase::Start);
        let mut skipped_midway = false;
        for a in &step.before {
            let r = self.action(step, a, "before")?;
            result.actions.push(r);
        }
        if step.user_action {
            let prompt = step.done_prompt.clone().unwrap_or_else(|| {
                "Go ahead now. Press Enter when you have finished (or type skip if it turns out to be impossible).".into()
            });
            say("");
            let input = self.console.ask(&prompt)?;
            skipped_midway = is_skip(&input);
            self.recorder.marker(&step.id, Phase::Done);
            let quiet =
                Duration::from_millis(step.quiet_ms.unwrap_or(self.protocol.settings.quiet_ms));
            let max = Duration::from_millis(self.protocol.settings.max_wait_ms);
            say("Waiting for the files to settle...");
            if !self.recorder.wait_quiet(quiet, max) {
                say(&format!(
                    "Files were still changing after {} seconds; carrying on anyway.",
                    max.as_secs()
                ));
            }
        }
        for a in &step.after {
            let r = self.action(step, a, "after")?;
            result.actions.push(r);
        }
        self.recorder.marker(&step.id, Phase::End);
        self.recorder.flush()?;

        if skipped_midway {
            result.status = StepStatus::Skipped;
            result.skip_reason = self.ask_skip_reason()?;
        } else {
            self.ask_questions(step, &mut result)?;
        }
        result.finished = Some(wall_now());
        let seen = self.observe(&step.id);
        if step.user_action || seen != report::NO_CHANGES {
            say("");
            say(&format!("What the tool saw: {seen}"));
        }
        result.seen = Some(seen);
        self.finish_step(step, result)
    }

    fn skip(&mut self, step: &Step, mut result: StepResult) -> Result<(), ProbeError> {
        result.status = StepStatus::Skipped;
        result.skip_reason = self.ask_skip_reason()?;
        result.finished = Some(wall_now());
        say("Skipped.");
        self.finish_step(step, result)
    }

    fn ask_skip_reason(&mut self) -> Result<Option<String>, ProbeError> {
        let why = self
            .console
            .ask("If you like, type why (for example \"the menu item does not exist\"), or just press Enter:")?;
        Ok(Some(why.trim().to_string()).filter(|w| !w.is_empty()))
    }

    fn finish_step(&mut self, step: &Step, result: StepResult) -> Result<(), ProbeError> {
        self.state.steps.insert(step.id.clone(), result);
        self.state.save(&self.files.answers)?;
        let out_dir = self.files.dir.clone();
        environment::write(
            &self.files.environment,
            self.protocol,
            self.state,
            self.system,
            &out_dir,
        )?;
        report::write_summary(self.files, self.protocol, self.state)
    }

    fn action(
        &mut self,
        step: &Step,
        spec: &crate::protocol::ActionSpec,
        when: &str,
    ) -> Result<ActionResult, ProbeError> {
        let mut ctx = Ctx {
            protocol: self.protocol,
            roots: self.roots,
            watch_ids: self.watch_ids,
            recorder: self.recorder,
            console: self.console,
            state: self.state,
            out_dir: &self.files.dir,
            backups_dir: &self.files.backups,
            step_id: &step.id,
            settle: Duration::from_millis(700),
        };
        let outcome = actions::run(&mut ctx, spec)?;
        let prefix = if outcome.ok {
            "Tool:"
        } else {
            "Tool (did not work):"
        };
        say(&format!("{prefix} {}", outcome.message));
        self.recorder.write(Body::Action {
            step: step.id.clone(),
            action: spec.action.as_str().to_string(),
            ok: outcome.ok,
            message: outcome.message.clone(),
            detail: outcome.detail,
        });
        // Remembered targets and backups must survive a pause.
        self.state.save(&self.files.answers)?;
        Ok(ActionResult {
            action: spec.action.as_str().to_string(),
            when: when.to_string(),
            ok: outcome.ok,
            message: outcome.message,
        })
    }

    fn ask_questions(&mut self, step: &Step, result: &mut StepResult) -> Result<(), ProbeError> {
        let mut asked_any = false;
        for q in &step.questions {
            if !conditions_met(&q.when, self.state, &result.answers, &step.id) {
                continue;
            }
            if !asked_any {
                say("");
                say("A few questions (type skip for any you cannot answer):");
                asked_any = true;
            }
            let vars = self.vars();
            let text = protocol::fill(&q.ask, &vars);
            drop(vars);
            let answer = ask_question(self.console, q, &text)?;
            result.answers.push(answer);
        }
        Ok(())
    }

    fn observe(&self, step_id: &str) -> String {
        let records = match read_trace(&self.files.trace) {
            Ok((r, _)) => r,
            Err(e) => return format!("(could not read the log: {e})"),
        };
        let opts = ClassifyOptions {
            ignore: self.protocol.settings.ignore.clone(),
            ..ClassifyOptions::default()
        };
        match report::observe_step(&records, step_id, &opts) {
            Some(obs) => report::one_line(&obs, &self.labels()),
            None => "(nothing recorded)".into(),
        }
    }

    fn labels(&self) -> Vec<String> {
        self.state.watch.iter().map(|w| w.label.clone()).collect()
    }
}

/// Do the earlier answers match every `key = value` condition? Keys without a
/// dot refer to questions of the current step (`current`).
pub fn conditions_met(
    when: &BTreeMap<String, String>,
    state: &SessionState,
    current: &[Answer],
    step_id: &str,
) -> bool {
    when.iter().all(|(key, want)| {
        let (step, q) = key.split_once('.').unwrap_or((step_id, key.as_str()));
        let got = if step == step_id {
            current
                .iter()
                .find(|a| a.id == q)
                .and_then(|a| a.value.as_deref())
                .or_else(|| state.answer(&format!("{step}.{q}")))
        } else {
            state.answer(&format!("{step}.{q}"))
        };
        got.is_some_and(|g| g.trim().eq_ignore_ascii_case(want.trim()))
    })
}

/// Does any `key = value` pair match an earlier answer? (For `unless`.)
pub fn any_condition_met(
    unless: &BTreeMap<String, String>,
    state: &SessionState,
    step_id: &str,
) -> bool {
    unless.iter().any(|(k, v)| {
        let mut one = BTreeMap::new();
        one.insert(k.clone(), v.clone());
        conditions_met(&one, state, &[], step_id)
    })
}

fn ask_question(console: &mut Console, q: &Question, text: &str) -> Result<Answer, ProbeError> {
    let mut answer = Answer {
        id: q.id.clone(),
        ask: text.to_string(),
        value: None,
        skipped: false,
    };
    say("");
    match q.kind {
        QuestionKind::YesNo => loop {
            let input = console.ask(&format!("{text} (yes / no / skip)"))?;
            match input.trim().to_lowercase().as_str() {
                "y" | "yes" => {
                    answer.value = Some("yes".into());
                    break;
                }
                "n" | "no" => {
                    answer.value = Some("no".into());
                    break;
                }
                "skip" => {
                    answer.skipped = true;
                    break;
                }
                _ => say("Please type yes or no (or skip)."),
            }
        },
        QuestionKind::Choice => {
            say(text);
            for (i, o) in q.options.iter().enumerate() {
                say(&format!("  {}. {o}", i + 1));
            }
            let other_no = q.options.len() + 1;
            if q.other {
                say(&format!("  {other_no}. Something else (you can type it)"));
            }
            let max = if q.other { other_no } else { q.options.len() };
            loop {
                let input = console.ask(&format!("Type a number from 1 to {max} (or skip):"))?;
                let t = input.trim();
                if is_skip(t) {
                    answer.skipped = true;
                    break;
                }
                if let Some(o) = q.options.iter().find(|o| o.eq_ignore_ascii_case(t)) {
                    answer.value = Some(o.clone());
                    break;
                }
                match t.parse::<usize>() {
                    Ok(n) if n >= 1 && n <= q.options.len() => {
                        answer.value = q.options.get(n - 1).cloned();
                        break;
                    }
                    Ok(n) if q.other && n == other_no => {
                        let typed = console.ask("Type it:")?;
                        if typed.trim().is_empty() || is_skip(&typed) {
                            answer.skipped = true;
                        } else {
                            answer.value = Some(format!("other: {}", typed.trim()));
                        }
                        break;
                    }
                    _ if q.other && !t.is_empty() && t.parse::<usize>().is_err() => {
                        answer.value = Some(format!("other: {t}"));
                        break;
                    }
                    _ => say(&format!("Please type a number from 1 to {max}.")),
                }
            }
        }
        QuestionKind::Text => {
            let input = console.ask(&format!(
                "{text}\n(type your answer and press Enter, or type skip)"
            ))?;
            let t = input.trim();
            if t.is_empty() || is_skip(t) {
                answer.skipped = true;
            } else {
                answer.value = Some(t.to_string());
            }
        }
    }
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(step: &str, id: &str, value: &str) -> SessionState {
        let mut s = SessionState::default();
        s.steps.insert(
            step.into(),
            StepResult {
                status: StepStatus::Done,
                run: 1,
                started: None,
                finished: None,
                answers: vec![Answer {
                    id: id.into(),
                    ask: String::new(),
                    value: Some(value.into()),
                    skipped: false,
                }],
                actions: Vec::new(),
                seen: None,
                skip_reason: None,
            },
        );
        s
    }

    #[test]
    fn conditions_use_earlier_and_current_answers() {
        let s = state_with("s00", "trial", "Yes");
        let mut when = BTreeMap::new();
        when.insert("s00.trial".to_string(), "yes".to_string());
        assert!(conditions_met(&when, &s, &[], "s05"));
        when.insert("warned".to_string(), "yes".to_string());
        assert!(!conditions_met(&when, &s, &[], "s05"));
        let current = vec![Answer {
            id: "warned".into(),
            ask: String::new(),
            value: Some("yes".into()),
            skipped: false,
        }];
        assert!(conditions_met(&when, &s, &current, "s05"));
    }

    #[test]
    fn choice_accepts_number_text_and_other() {
        let q = Question {
            id: "q".into(),
            ask: "Pick".into(),
            kind: QuestionKind::Choice,
            options: vec!["Off".into(), "Every minute".into()],
            other: true,
            when: BTreeMap::new(),
            env: None,
        };
        let mut c = Console::from_reader(std::io::Cursor::new(
            "9\n2\nevery minute\n3\nhourly\nweekly\n",
        ));
        assert_eq!(
            ask_question(&mut c, &q, "Pick").unwrap().value.as_deref(),
            Some("Every minute")
        );
        assert_eq!(
            ask_question(&mut c, &q, "Pick").unwrap().value.as_deref(),
            Some("Every minute")
        );
        assert_eq!(
            ask_question(&mut c, &q, "Pick").unwrap().value.as_deref(),
            Some("other: hourly")
        );
        assert_eq!(
            ask_question(&mut c, &q, "Pick").unwrap().value.as_deref(),
            Some("other: weekly")
        );
    }

    #[test]
    fn yes_no_and_text_accept_skip() {
        let yn = Question {
            id: "q".into(),
            ask: "?".into(),
            kind: QuestionKind::YesNo,
            options: vec![],
            other: false,
            when: BTreeMap::new(),
            env: None,
        };
        let text = Question {
            kind: QuestionKind::Text,
            ..yn.clone()
        };
        let mut c = Console::from_reader(std::io::Cursor::new("maybe\nY\nskip\n\nhello\n"));
        assert_eq!(
            ask_question(&mut c, &yn, "?").unwrap().value.as_deref(),
            Some("yes")
        );
        assert!(ask_question(&mut c, &yn, "?").unwrap().skipped);
        assert!(ask_question(&mut c, &text, "?").unwrap().skipped);
        assert_eq!(
            ask_question(&mut c, &text, "?").unwrap().value.as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn backups_folder_is_a_sibling() {
        let f = OutFiles::new(Path::new("/r/validation-results/fl-studio"));
        assert_eq!(
            f.backups,
            PathBuf::from("/r/validation-results/fl-studio-backups")
        );
    }
}
