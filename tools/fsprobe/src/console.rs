//! Terminal input and output for the guided session.
//!
//! Input is read on its own thread so the session can wait for Enter with a
//! timeout (countdowns) and so a script can drive the tool through stdin.
//! Output errors (a closed terminal) are ignored rather than panicking.

use std::io::{BufRead, Write};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use crate::error::ProbeError;

/// Words that pause the session at any prompt.
const PAUSE_WORDS: [&str; 3] = ["pause", "quit", "exit"];

/// Line-based console.
pub struct Console {
    rx: Receiver<Option<String>>,
    ended: bool,
}

impl std::fmt::Debug for Console {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Console")
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}

impl Console {
    /// Read from the process's stdin.
    pub fn stdin() -> Self {
        Self::from_reader(std::io::stdin())
    }

    /// Read lines from any reader (tests).
    pub fn from_reader<R: std::io::Read + Send + 'static>(reader: R) -> Self {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("fsprobe-input".into())
            .spawn(move || {
                let mut lines = std::io::BufReader::new(reader).lines();
                loop {
                    match lines.next() {
                        Some(Ok(line)) => {
                            if tx.send(Some(line)).is_err() {
                                return;
                            }
                        }
                        _ => {
                            let _ = tx.send(None);
                            return;
                        }
                    }
                }
            });
        let ended = spawned.is_err();
        Self { rx, ended }
    }

    /// Next line. "pause" / "quit" / "exit" become [`ProbeError::Paused`].
    pub fn line(&mut self) -> Result<String, ProbeError> {
        if self.ended {
            return Err(ProbeError::InputEnded);
        }
        match self.rx.recv() {
            Ok(Some(line)) => check_pause(line),
            _ => {
                self.ended = true;
                Err(ProbeError::InputEnded)
            }
        }
    }

    /// Next line, or `None` after `timeout`.
    pub fn line_timeout(&mut self, timeout: Duration) -> Result<Option<String>, ProbeError> {
        if self.ended {
            return Err(ProbeError::InputEnded);
        }
        match self.rx.recv_timeout(timeout) {
            Ok(Some(line)) => check_pause(line).map(Some),
            Ok(None) | Err(RecvTimeoutError::Disconnected) => {
                self.ended = true;
                Err(ProbeError::InputEnded)
            }
            Err(RecvTimeoutError::Timeout) => Ok(None),
        }
    }

    /// Print `text`, then a `> ` prompt, and read a line.
    pub fn ask(&mut self, text: &str) -> Result<String, ProbeError> {
        say(text);
        prompt_marker();
        self.line()
    }
}

fn check_pause(line: String) -> Result<String, ProbeError> {
    let word = line.trim().to_lowercase();
    if PAUSE_WORDS.contains(&word.as_str()) {
        Err(ProbeError::Paused)
    } else {
        Ok(line)
    }
}

/// Print a line (errors ignored).
pub fn say(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{text}");
    let _ = out.flush();
}

/// Print without a newline (errors ignored).
pub fn say_inline(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "{text}");
    let _ = out.flush();
}

fn prompt_marker() {
    say_inline("> ");
}

/// A visible separator line with a heading.
pub fn heading(text: &str) {
    say("");
    say(&format!("=== {text} ==="));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lines_then_reports_end() {
        let mut c = Console::from_reader(std::io::Cursor::new("one\ntwo\n"));
        assert_eq!(c.line().unwrap(), "one");
        assert_eq!(c.line().unwrap(), "two");
        assert!(matches!(c.line(), Err(ProbeError::InputEnded)));
        assert!(matches!(c.line(), Err(ProbeError::InputEnded)));
    }

    #[test]
    fn pause_word_pauses() {
        let mut c = Console::from_reader(std::io::Cursor::new("  Pause \n"));
        assert!(matches!(c.line(), Err(ProbeError::Paused)));
    }
}
