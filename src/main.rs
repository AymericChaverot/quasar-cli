//! `qsr`, the command-line client for Quasar.

use clap::Parser;

/// The version a release was tagged with, which the release workflow passes
/// in as QSR_VERSION. Anything built without it is a development build.
const VERSION: &str = match option_env!("QSR_VERSION") {
    Some(v) => v,
    None => concat!(env!("CARGO_PKG_VERSION"), "-dev"),
};

/// Drive Quasar servers from a terminal or a pipeline.
#[derive(Parser)]
#[command(name = "qsr", version = VERSION, about, propagate_version = true)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
