//! `qsr env`: read and change an application's environment.

use std::collections::BTreeMap;

use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::api::Client;
use crate::apps::{self, App};
use crate::{dotenv, output, session};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct EnvArgs {
    /// Show this application's environment (its name, subdomain or id)
    app: Option<String>,

    #[command(subcommand)]
    command: Option<EnvCommand>,
}

#[derive(Subcommand)]
pub enum EnvCommand {
    /// Set variables, adding those not there yet
    Set {
        /// Its name, subdomain or id
        app: String,
        /// The variables, as KEY=value
        #[arg(required = true, value_name = "KEY=VALUE")]
        pairs: Vec<String>,
        /// Redeploy afterwards, so the application runs with them
        #[arg(long)]
        deploy: bool,
    },
    /// Remove variables
    Unset {
        /// Its name, subdomain or id
        app: String,
        /// The names of the variables
        #[arg(required = true, value_name = "KEY")]
        keys: Vec<String>,
        /// Redeploy afterwards, so the application runs without them
        #[arg(long)]
        deploy: bool,
    },
}

#[derive(Serialize, Deserialize)]
struct Env {
    content: String,
}

#[derive(Deserialize)]
struct Saved {}

pub fn run(args: EnvArgs, context: Option<&str>, json: bool) -> Result<()> {
    match (args.command, args.app) {
        (Some(EnvCommand::Set { app, pairs, deploy }), _) => {
            let pairs = pairs
                .iter()
                .map(|p| dotenv::parse_pair(p))
                .collect::<Result<Vec<_>>>()?;
            edit(&app, deploy, context, json, |mut content| {
                for (k, v) in &pairs {
                    content = dotenv::set(&content, k, v);
                }
                let names: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
                Ok((content, format!("Set {}", names.join(", "))))
            })
        }
        (Some(EnvCommand::Unset { app, keys, deploy }), _) => {
            for k in &keys {
                dotenv::check_key(k)?;
            }
            edit(&app, deploy, context, json, |mut content| {
                let mut missing = Vec::new();
                for k in &keys {
                    let (next, found) = dotenv::unset(&content, k);
                    content = next;
                    if !found {
                        missing.push(k.as_str());
                    }
                }
                if missing.len() == keys.len() {
                    bail!("none of {} is set", keys.join(", "));
                }
                let mut msg = format!("Removed {}", keys.join(", "));
                if !missing.is_empty() {
                    msg = format!("{msg} ({} was not set)", missing.join(", "));
                }
                Ok((content, msg))
            })
        }
        (None, Some(app)) => show(&app, context, json),
        (None, None) => bail!("name an application: `qsr env <app>`, or see `qsr env --help`"),
    }
}

#[derive(Serialize)]
struct Shown<'a> {
    app: &'a str,
    variables: BTreeMap<String, String>,
    content: &'a str,
}

fn show(app: &str, context: Option<&str>, json: bool) -> Result<()> {
    let s = session::open(context)?;
    let a = apps::find(&s.client, app)?;
    let env = read(&s.client, &a)?;
    if json {
        return output::json(&Shown {
            app: &a.id,
            variables: dotenv::parse(&env.content),
            content: &env.content,
        });
    }
    if env.content.trim().is_empty() {
        eprintln!("{} has no environment variables.", a.name);
        return Ok(());
    }
    print!("{}", env.content);
    if !env.content.ends_with('\n') {
        println!();
    }
    Ok(())
}

/// Reads the environment. The server says plainly when the token may not,
/// so its refusal goes through as it is.
fn read(client: &Client, a: &App) -> Result<Env> {
    client.get(&format!("/apps/{}/env", a.id))
}

/// Reads the environment, lets `change` rewrite it, writes it back, and
/// redeploys when asked.
fn edit(
    app: &str,
    deploy: bool,
    context: Option<&str>,
    json: bool,
    change: impl FnOnce(String) -> Result<(String, String)>,
) -> Result<()> {
    let s = session::open(context)?;
    let a = apps::find(&s.client, app)?;
    let before = read(&s.client, &a)?.content;
    let (after, what) = change(before.clone())?;
    if after == before {
        if !json {
            println!(
                "{} already has that environment; nothing to change.",
                a.name
            );
        }
        return Ok(());
    }
    let _: Saved = s
        .client
        .put(
            &format!("/apps/{}/env", a.id),
            &Env {
                content: after.clone(),
            },
        )
        .context("could not save the environment")?;
    if !json {
        println!("{what} on {}.", a.name);
    }
    if deploy {
        return crate::cmd::lifecycle::redeploy(&s.client, &a, json);
    }
    if json {
        return output::json(&Shown {
            app: &a.id,
            variables: dotenv::parse(&after),
            content: &after,
        });
    }
    println!(
        "It applies from the next deploy: `qsr deploy {}`, or pass --deploy.",
        a.subdomain
    );
    Ok(())
}
