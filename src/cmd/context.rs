//! `qsr context`: list, switch and forget the servers `qsr` knows.

use anyhow::Result;
use clap::Subcommand;
use serde::Serialize;

use crate::config::{self, Config};
use crate::output;

#[derive(Subcommand)]
pub enum ContextCommand {
    /// List the contexts, marking the current one
    #[command(visible_alias = "ls")]
    List,
    /// Make a context the one commands use by default
    Use {
        /// The context's name
        name: String,
    },
    /// Forget a context and its token
    #[command(visible_alias = "rm")]
    Remove {
        /// The context's name
        name: String,
    },
}

#[derive(Serialize)]
struct Row<'a> {
    name: &'a str,
    url: &'a str,
    current: bool,
}

pub fn run(cmd: ContextCommand, json: bool) -> Result<()> {
    let path = config::default_path()?;
    let mut cfg = Config::load(&path)?;
    match cmd {
        ContextCommand::List => {
            let rows: Vec<Row> = cfg
                .contexts
                .iter()
                .map(|(name, ctx)| Row {
                    name,
                    url: &ctx.url,
                    current: cfg.current.as_deref() == Some(name),
                })
                .collect();
            if json {
                return output::json(&rows);
            }
            if rows.is_empty() {
                println!("No contexts yet; add one with `qsr login <name> <url>`.");
                return Ok(());
            }
            let cells: Vec<Vec<String>> = rows
                .iter()
                .map(|r| {
                    vec![
                        if r.current { "*" } else { "" }.to_string(),
                        r.name.to_string(),
                        r.url.to_string(),
                    ]
                })
                .collect();
            output::table(&["", "NAME", "URL"], &cells)
        }
        ContextCommand::Use { name } => {
            cfg.use_context(&name)?;
            cfg.save(&path)?;
            println!("Now using {name} ({}).", cfg.contexts[&name].url);
            Ok(())
        }
        ContextCommand::Remove { name } => {
            cfg.remove(&name)?;
            cfg.save(&path)?;
            println!("Removed {name}.");
            Ok(())
        }
    }
}
