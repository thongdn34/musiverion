//! `fsprobe`: records how a DAW saves files, step by step.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use fsprobe::console::{Console, say};
use fsprobe::protocol::Protocol;
use fsprobe::recorder::RecorderOptions;
use fsprobe::session::{self, OutFiles, RunArgs, SessionState};
use fsprobe::{ProbeError, report};

#[derive(Debug, Parser)]
#[command(
    name = "fsprobe",
    version,
    about = "Records how a DAW saves its files, guided step by step"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start a guided session, or continue one (same --out folder).
    Run {
        /// Protocol file, e.g. tools/fsprobe/protocols/fl-studio.toml.
        #[arg(long)]
        protocol: PathBuf,
        /// Folder to watch (repeatable). Leave out to be asked.
        #[arg(long)]
        watch: Vec<PathBuf>,
        /// Results folder. Running again with the same folder continues.
        #[arg(long)]
        out: PathBuf,
    },
    /// Rebuild summary.md from a results folder (after classifier changes).
    Report {
        /// Results folder.
        #[arg(long)]
        out: PathBuf,
        /// Protocol file (default: the copy inside the results folder).
        #[arg(long)]
        protocol: Option<PathBuf>,
    },
    /// Check a protocol file for mistakes.
    Check {
        /// Protocol file.
        #[arg(long)]
        protocol: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match execute(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            say("");
            say(&format!("Problem: {err}"));
            say(
                "Nothing you did so far is lost. If you are stuck, send this message along with the results folder.",
            );
            ExitCode::FAILURE
        }
    }
}

fn execute(command: Command) -> Result<(), ProbeError> {
    match command {
        Command::Run {
            protocol,
            watch,
            out,
        } => {
            let args = RunArgs {
                protocol,
                watch,
                out,
                recorder: RecorderOptions::default(),
            };
            let mut console = Console::stdin();
            session::run(&args, &mut console).map(|_| ())
        }
        Command::Report { out, protocol } => {
            let files = OutFiles::new(&out);
            let protocol = Protocol::load(&protocol.unwrap_or_else(|| files.protocol.clone()))?;
            let state = SessionState::load(&files.answers)?.ok_or_else(|| {
                ProbeError::Usage(format!("{} has no answers.json", out.display()))
            })?;
            report::write_summary(&files, &protocol, &state)?;
            say(&format!("Wrote {}", files.summary.display()));
            Ok(())
        }
        Command::Check { protocol } => {
            let p = Protocol::load(&protocol)?;
            say(&format!(
                "{}: OK ({} steps, {} folders to watch, {} named files)",
                protocol.display(),
                p.steps.len(),
                p.watch.len(),
                p.targets.len()
            ));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
