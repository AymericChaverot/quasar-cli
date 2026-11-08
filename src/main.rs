//! `qsr`, the command-line client for Quasar.

mod cmd;
mod config;
mod output;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// The version a release was tagged with, which the release workflow passes
/// in as QSR_VERSION. Anything built without it is a development build.
const VERSION: &str = match option_env!("QSR_VERSION") {
    Some(v) => v,
    None => concat!(env!("CARGO_PKG_VERSION"), "-dev"),
};

/// Drive Quasar servers from a terminal or a pipeline.
#[derive(Parser)]
#[command(name = "qsr", version = VERSION, about, propagate_version = true)]
struct Cli {
    /// Run against this context instead of the current one
    #[arg(long, global = true, env = "QSR_CONTEXT")]
    context: Option<String>,

    /// Print the result as JSON, for scripts
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage the servers qsr knows
    #[command(subcommand)]
    Context(cmd::context::ContextCommand),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Context(c) => cmd::context::run(c, cli.json),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
