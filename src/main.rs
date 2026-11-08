//! `qsr`, the command-line client for Quasar.

mod api;
mod apps;
mod cmd;
mod config;
mod output;
mod session;

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
    /// Sign in to a server with a personal token, as a named context
    Login {
        /// A name for this server, such as prod
        name: String,
        /// The dashboard's address, such as https://admin.example.com
        url: String,
        /// Read the token from stdin instead of prompting for it
        #[arg(long)]
        token_stdin: bool,
    },
    /// Forget a context and its token (the current one by default)
    Logout {
        /// The context to forget
        name: Option<String>,
    },
    /// Show who you are signed in as, and where
    Whoami,
    /// List applications, or show one
    Apps {
        #[command(subcommand)]
        command: Option<cmd::apps::AppsCommand>,
    },
    /// Manage the servers qsr knows
    #[command(subcommand)]
    Context(cmd::context::ContextCommand),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let ctx = cli.context.as_deref();
    let signing_in = matches!(cli.command, Command::Login { .. });
    let result = match cli.command {
        Command::Login {
            name,
            url,
            token_stdin,
        } => cmd::auth::login(&name, &url, token_stdin),
        Command::Logout { name } => cmd::auth::logout(name.as_deref(), ctx),
        Command::Whoami => cmd::auth::whoami(ctx, cli.json),
        Command::Apps { command } => cmd::apps::run(command, ctx, cli.json),
        Command::Context(c) => cmd::context::run(c, cli.json),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            // A stored token that stopped working has one remedy, which is
            // not obvious from "invalid token" alone.
            let refused = e
                .downcast_ref::<api::ApiError>()
                .is_some_and(|a| a.status == reqwest::StatusCode::UNAUTHORIZED);
            if refused && !signing_in {
                eprintln!("hint: create a new token and sign in again with `qsr login`");
            }
            ExitCode::FAILURE
        }
    }
}
