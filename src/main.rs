//! `qsr`, the command-line client for Quasar.

mod api;
mod apps;
mod cmd;
mod config;
mod dotenv;
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
    /// Fetch an application's latest version and deploy it, then wait for the outcome
    Deploy {
        /// Its name, subdomain or id
        app: String,
        /// Return once the deploy has started, without waiting for it
        #[arg(long, short)]
        detach: bool,
    },
    /// Print an application's recent logs, or follow them
    Logs {
        /// Its name, subdomain or id
        app: String,
        /// How many recent lines to start with (at most 5000)
        #[arg(long, short = 'n', default_value_t = 200, value_parser = clap::value_parser!(u32).range(1..=5000))]
        lines: u32,
        /// Keep printing new lines as they are written
        #[arg(long, short)]
        follow: bool,
        /// Prefix each line with the time Docker received it
        #[arg(long, short)]
        timestamps: bool,
    },
    /// Show an application's environment, or change it
    Env(cmd::env::EnvArgs),
    /// Restart an application's containers
    Restart {
        /// Its name, subdomain or id
        app: String,
    },
    /// Start an application's containers
    Start {
        /// Its name, subdomain or id
        app: String,
    },
    /// Stop an application's containers
    Stop {
        /// Its name, subdomain or id
        app: String,
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
        Command::Deploy { app, detach } => cmd::lifecycle::deploy(&app, detach, ctx, cli.json),
        Command::Logs {
            app,
            lines,
            follow,
            timestamps,
        } => cmd::logs::run(
            &app,
            cmd::logs::Options {
                lines,
                follow,
                timestamps,
            },
            ctx,
            cli.json,
        ),
        Command::Env(args) => cmd::env::run(args, ctx, cli.json),
        Command::Restart { app } => cmd::lifecycle::lifecycle("restart", &app, ctx, cli.json),
        Command::Start { app } => cmd::lifecycle::lifecycle("start", &app, ctx, cli.json),
        Command::Stop { app } => cmd::lifecycle::lifecycle("stop", &app, ctx, cli.json),
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
