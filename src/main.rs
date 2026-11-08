//! `qsr`, the command-line client for Quasar.

use clap::Parser;

/// Drive Quasar servers from a terminal or a pipeline.
#[derive(Parser)]
#[command(name = "qsr", version, about, propagate_version = true)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
