//! Scripted end-to-end run of `fsprobe run`: a tiny protocol, answers fed
//! through stdin, real file operations performed between the prompts, a
//! pause, and a resume that continues at the first unanswered step.

// Tests may unwrap (clippy.toml allows it in #[test] fns; this covers helpers).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PROTOCOL: &str = r#"
schema = 1
id = "tiny"
title = "Tiny test protocol"
daw = "TestDAW"
intro = "This is the intro."
outro = "This is the outro."

[settings]
quiet_ms = 400
max_wait_ms = 10000
ignore = [".DS_Store"]

[[watch]]
id = "test"
label = "Test folder"
explain = "The folder the test works in."

[targets.project]
label = "the project file"
pattern = "*.prj"
root = "test"
prefer = "Song.prj"

[[steps]]
id = "env"
title = "About your setup"
user_action = false

[[steps.questions]]
id = "version"
ask = "Which version?"
type = "text"
env = "daw_version"

[[steps]]
id = "save"
title = "Save the project"
text = "Press {mod}+S."
after = [{ action = "stat_file", target = "project" }]

[[steps.questions]]
id = "ok"
ask = "Did it work?"
type = "yes_no"

[[steps]]
id = "readonly"
title = "Save a read-only project"
before = [
  { action = "set_readonly", target = "project" },
  { action = "backup_file", target = "project", slot = "ro" },
]
after = [
  { action = "compare_with_backup", slot = "ro" },
  { action = "clear_readonly", target = "project" },
  { action = "snapshot_tree" },
  { action = "list_open_handles" },
]

[[steps.questions]]
id = "error"
ask = "Did you see an error?"
type = "choice"
options = ["No error", "An error"]
other = true

[[steps.questions]]
id = "error_text"
ask = "What did it say?"
type = "text"
when = { error = "An error" }

[[steps]]
id = "trial_only"
title = "Only for trial users"
when = { "env.version" = "trial" }

[[steps]]
id = "last"
title = "Wrap up"
user_action = false

[[steps.questions]]
id = "notes"
ask = "Anything else?"
type = "text"
"#;

/// Drives the binary through its prompts.
struct Driver {
    child: Child,
    stdin: Option<ChildStdin>,
    out: Arc<Mutex<String>>,
    cursor: usize,
    transcript: Arc<Mutex<String>>,
}

impl Driver {
    fn spawn(args: &[&str], transcript: Arc<Mutex<String>>) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fsprobe"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let mut stdout = child.stdout.take().unwrap();
        let out = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&out);
        let log = Arc::clone(&transcript);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = stdout.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                sink.lock().unwrap().push_str(&text);
                log.lock().unwrap().push_str(&text);
            }
        });
        Self {
            child,
            stdin,
            out,
            cursor: 0,
            transcript,
        }
    }

    /// Wait until `needle` appears after everything matched so far.
    fn expect(&mut self, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            {
                let out = self.out.lock().unwrap();
                if let Some(pos) = out[self.cursor..].find(needle) {
                    self.cursor += pos + needle.len();
                    return;
                }
            }
            if Instant::now() > deadline {
                let out = self.out.lock().unwrap();
                panic!(
                    "timed out waiting for {needle:?}; output after cursor:\n{}",
                    &out[self.cursor..]
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn send(&mut self, line: &str) {
        self.transcript
            .lock()
            .unwrap()
            .push_str(&format!("{line}    <-- typed\n"));
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    fn wait(mut self) -> (bool, String) {
        drop(self.stdin.take());
        let status = self.child.wait().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let out = self.out.lock().unwrap().clone();
        (status.success(), out)
    }
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
#[cfg_attr(windows, ignore = "run by hand on Windows")]
fn guided_run_pause_and_resume() {
    let work = tempfile::tempdir().unwrap();
    let watched = work.path().join("Song A");
    std::fs::create_dir_all(&watched).unwrap();
    std::fs::write(watched.join("Song.prj"), vec![b'a'; 1000]).unwrap();
    let protocol = work.path().join("tiny.toml");
    std::fs::write(&protocol, PROTOCOL).unwrap();
    let out = work.path().join("results/tiny-1");
    let protocol_s = protocol.to_string_lossy().to_string();
    let out_s = out.to_string_lossy().to_string();
    let args = ["run", "--protocol", &protocol_s, "--out", &out_s];
    let transcript = Arc::new(Mutex::new(String::new()));

    // ---- First run: choose the folder, do two steps, then pause.
    let mut d = Driver::spawn(&args, Arc::clone(&transcript));
    d.expect("This is the intro.");
    d.expect("--- Test folder ---");
    d.expect("drag the folder into this window");
    d.send(&watched.to_string_lossy());
    d.expect("Watching:");
    d.expect("Step 1 of 5: About your setup");
    d.expect("Which version?");
    d.send("1.2.3");

    d.expect("Step 2 of 5: Save the project");
    d.expect("press Enter to start");
    d.send("");
    d.expect("Press Enter when you have finished");
    // The "DAW" saves through a temporary file.
    let tmp = watched.join("Song.prj.tmp");
    std::fs::write(&tmp, vec![b'b'; 2000]).unwrap();
    std::fs::rename(&tmp, watched.join("Song.prj")).unwrap();
    d.send("");
    d.expect("Using Song.prj.");
    d.expect("Tool: Song.prj: 2.0 KB, read-only: no");
    d.expect("Did it work? (yes / no / skip)");
    d.send("yes");
    d.expect("What the tool saw: `Song.prj` was replaced via a temporary file: `Song.prj.tmp` -> `Song.prj`");

    d.expect("Step 3 of 5: Save a read-only project");
    d.expect("press Enter to start");
    d.send("pause");
    d.expect("Paused.");
    let (ok, _) = d.wait();
    assert!(ok, "first run should exit cleanly");

    let answers = read_json(&out.join("answers.json"));
    assert_eq!(answers["steps"]["env"]["answers"][0]["value"], "1.2.3");
    assert_eq!(answers["steps"]["save"]["answers"][0]["value"], "yes");
    assert!(answers["steps"].get("readonly").is_none());
    assert_eq!(answers["runs"][0]["outcome"], "paused");
    let env = read_json(&out.join("environment.json"));
    assert_eq!(env["daw"]["name"], "TestDAW");
    assert_eq!(env["daw"]["daw_version"], "1.2.3");
    assert!(env["os"]["family"].is_string());
    let summary = std::fs::read_to_string(out.join("summary.md")).unwrap();
    assert!(summary.contains("temp_then_rename"), "{summary}");
    assert!(summary.contains("not done yet"), "{summary}");

    // ---- Second run: same command, continues at step 3.
    let mut d = Driver::spawn(&args, Arc::clone(&transcript));
    d.expect("Welcome back. Continuing at step 3 of 5.");
    d.expect("Step 3 of 5: Save a read-only project");
    d.expect("press Enter to start");
    d.send("");
    d.expect("is now read-only");
    d.expect("Took a safety copy of Song.prj");
    d.expect("Press Enter when you have finished");
    // The "DAW" deletes the read-only file and writes a new one.
    std::fs::remove_file(watched.join("Song.prj")).unwrap();
    std::fs::write(watched.join("Song.prj"), vec![b'c'; 1500]).unwrap();
    d.send("");
    d.expect("Song.prj has changed since the safety copy was taken");
    d.expect("can be written to again");
    d.expect("Saved a list of the");
    d.expect("Did you see an error?");
    d.expect("Type a number from 1 to 3");
    d.send("2");
    d.expect("What did it say?");
    d.send("Access denied");
    d.expect("What the tool saw: `Song.prj` was deleted and then written again from scratch");

    // Step 4 is skipped by its condition; step 5 asks one question.
    d.expect("Step 5 of 5: Wrap up");
    d.expect("Anything else?");
    d.send("Nothing else");
    d.expect("Please send back this whole folder");
    d.expect("It does not contain any audio");
    let (ok, _) = d.wait();
    assert!(ok, "second run should finish cleanly");

    let answers = read_json(&out.join("answers.json"));
    assert_eq!(
        answers["steps"]["readonly"]["answers"][0]["value"],
        "An error"
    );
    assert_eq!(
        answers["steps"]["readonly"]["answers"][1]["value"],
        "Access denied"
    );
    assert_eq!(answers["steps"]["trial_only"]["status"], "not_applicable");
    assert_eq!(
        answers["steps"]["last"]["answers"][0]["value"],
        "Nothing else"
    );
    assert_eq!(answers["runs"].as_array().unwrap().len(), 2);
    assert_eq!(answers["runs"][1]["outcome"], "finished");
    assert!(
        answers["targets"]["project"]
            .as_str()
            .unwrap()
            .ends_with("Song.prj")
    );

    let summary = std::fs::read_to_string(out.join("summary.md")).unwrap();
    assert!(summary.contains("delete_then_create"), "{summary}");
    assert!(summary.contains("`set_readonly` (before)"), "{summary}");
    assert!(summary.contains("not applicable"), "{summary}");
    assert!(!summary.contains("not done yet"), "{summary}");

    let trace = std::fs::read_to_string(out.join("trace.jsonl")).unwrap();
    assert!(trace.contains("\"run\":2"));
    assert!(trace.contains("\"tool\":\"set_readonly\""));
    assert!(out.join("protocol.toml").is_file());
    assert!(out.join("snapshots/readonly-1.json").is_file());
    // Safety copies live beside the results folder, not inside it.
    assert!(
        work.path()
            .join("results/tiny-1-backups/ro/Song.prj")
            .is_file()
    );

    let t = transcript.lock().unwrap().clone();
    if std::env::var_os("FSPROBE_SHOW_TRANSCRIPT").is_some() {
        println!("{t}");
    }
}

#[test]
#[cfg_attr(windows, ignore = "run by hand on Windows")]
fn resume_refuses_different_watch_folders() {
    let work = tempfile::tempdir().unwrap();
    let a = work.path().join("a");
    let b = work.path().join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    let protocol = work.path().join("tiny.toml");
    std::fs::write(&protocol, PROTOCOL).unwrap();
    let out = work.path().join("out");
    let run = |watch: &Path, input: &str| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fsprobe"))
            .arg("run")
            .arg("--protocol")
            .arg(&protocol)
            .arg("--watch")
            .arg(watch)
            .arg("--out")
            .arg(&out)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let first = run(&a, "pause\n");
    assert!(first.status.success());
    let second = run(&b, "");
    assert!(!second.status.success());
    let text = String::from_utf8_lossy(&second.stdout);
    assert!(text.contains("different folders to watch"), "{text}");
}

#[test]
fn check_command_accepts_the_shipped_fl_studio_protocol() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("protocols/fl-studio.toml");
    let out = Command::new(env!("CARGO_BIN_EXE_fsprobe"))
        .arg("check")
        .arg("--protocol")
        .arg(&path)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("OK"), "{text}");
}
