//! `mv-cli`: headless command-line tool over `mv-app`.
//!
//! Later it also serves as the git textconv/diff driver. The binary is named
//! `mv-cli` rather than `mv` so it never shadows the system `mv` (move) command.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "mv-cli", version, about = "Musiverion command-line tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check that the application service responds.
    Ping {
        /// Print the reply as JSON.
        #[arg(long)]
        json: bool,
    },
}

fn render_ping(json: bool) -> Result<String, serde_json::Error> {
    let reply = mv_app::ping();
    if json {
        serde_json::to_string(&reply)
    } else {
        Ok(format!("{} (core {})", reply.message, reply.core_version))
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Ping { json } => match render_ping(json) {
            Ok(out) => {
                println!("{out}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("error: {err}");
                ExitCode::FAILURE
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_json_is_valid() {
        let out = render_ping(true).unwrap();
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(value["coreVersion"].is_string());
    }

    #[test]
    fn cli_definition_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
