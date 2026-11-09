//! `qsr apps`: list, show, create and delete applications.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::apps::{self, App};
use crate::cmd::lifecycle;
use crate::{output, session};

#[derive(Subcommand)]
pub enum AppsCommand {
    /// List applications and their state (the default)
    #[command(visible_alias = "ls")]
    List,
    /// Show one application
    Show {
        /// Its name, subdomain or id
        app: String,
    },
    /// Create an application, and deploy it unless it waits for a folder
    Create(Box<CreateArgs>),
    /// Delete an application, with its containers, files and data
    #[command(visible_alias = "rm")]
    Delete {
        /// Its name, subdomain or id
        app: String,
        /// Do not ask for confirmation
        #[arg(long, short)]
        yes: bool,
    },
}

#[derive(Args)]
#[command(group(clap::ArgGroup::new("source").required(true)
    .args(["image", "git", "compose", "local"])))]
pub struct CreateArgs {
    /// Its name, as the dashboard shows it
    name: String,
    /// The subdomain it is served on (by default, made from its name; @ for
    /// the root domain)
    #[arg(long, short)]
    subdomain: Option<String>,
    /// Run this Docker image, such as nginx:latest
    #[arg(long, value_name = "REF")]
    image: Option<String>,
    /// Build this git repository
    #[arg(long, value_name = "URL")]
    git: Option<String>,
    /// The branch to build, with --git (main by default)
    #[arg(long)]
    branch: Option<String>,
    /// Run the stack in this compose file
    #[arg(long, value_name = "FILE")]
    compose: Option<PathBuf>,
    /// Deploy it from a folder on this machine, sent with `qsr deploy <app> <folder>`
    #[arg(long)]
    local: bool,
    /// The port it listens on inside its container (80 by default)
    #[arg(long, short, value_parser = clap::value_parser!(u16).range(1..))]
    port: Option<u16>,
    /// Its environment, from this dotenv file
    #[arg(long, value_name = "FILE")]
    env_file: Option<PathBuf>,
    /// A container path kept across deploys, such as /data
    #[arg(long, value_name = "PATH")]
    data_mount: Option<String>,
    /// Another domain routed to it (repeat for several)
    #[arg(long = "domain", value_name = "DOMAIN")]
    domains: Vec<String>,
    /// The most CPUs it may use, such as 0.5
    #[arg(long)]
    cpus: Option<f64>,
    /// The most memory it may use, in megabytes
    #[arg(long, value_name = "MB")]
    memory: Option<u64>,
    /// Return once its first deploy has started, without waiting for it
    #[arg(long, short)]
    detach: bool,
}

/// The body of `POST /api/v1/apps`.
#[derive(Serialize, Default)]
struct NewApp {
    name: String,
    subdomain: String,
    deploy_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    git_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    git_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compose_yaml: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_mount: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    custom_domains: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cpu_limit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mem_limit_mb: Option<u64>,
}

impl NewApp {
    fn from_args(args: &CreateArgs) -> Result<Self> {
        if args.branch.is_some() && args.git.is_none() {
            bail!("--branch only applies with --git");
        }
        let read = |path: &PathBuf| {
            std::fs::read_to_string(path)
                .with_context(|| format!("could not read {}", path.display()))
        };
        let mut new = NewApp {
            name: args.name.trim().to_string(),
            subdomain: match &args.subdomain {
                Some(s) => s.trim().to_lowercase(),
                None => subdomain_from(&args.name)?,
            },
            port: args.port,
            env_content: args.env_file.as_ref().map(read).transpose()?,
            data_mount: args.data_mount.clone(),
            custom_domains: args.domains.join(","),
            cpu_limit: args.cpus,
            mem_limit_mb: args.memory,
            ..Default::default()
        };
        if let Some(image) = &args.image {
            new.deploy_type = "image";
            new.image_ref = Some(image.clone());
        } else if let Some(url) = &args.git {
            new.deploy_type = "git";
            new.git_url = Some(url.clone());
            new.git_branch = args.branch.clone();
        } else if let Some(file) = &args.compose {
            new.deploy_type = "compose";
            new.compose_yaml = Some(read(file)?);
        } else {
            new.deploy_type = "local";
        }
        Ok(new)
    }
}

/// A subdomain made from an application's name: lowercase letters, digits
/// and single hyphens, as the server accepts.
fn subdomain_from(name: &str) -> Result<String> {
    let mut out = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        bail!("no subdomain can be made from {name:?}; give one with --subdomain");
    }
    Ok(out)
}

#[derive(Deserialize)]
struct Deleted {}

pub fn run(cmd: Option<AppsCommand>, context: Option<&str>, json: bool) -> Result<()> {
    let s = session::open(context)?;
    match cmd.unwrap_or(AppsCommand::List) {
        AppsCommand::Create(args) => {
            let new = NewApp::from_args(&args)?;
            let a: App = s.client.post_json("/apps", &new)?;
            if new.deploy_type == "local" {
                if json {
                    return output::json(&a);
                }
                println!("Created {} at https://{}.", a.name, a.host);
                println!(
                    "Deploy a folder to it with `qsr deploy {} <folder>`.",
                    a.subdomain
                );
                return Ok(());
            }
            if args.detach {
                if json {
                    return output::json(&a);
                }
                println!(
                    "Created {}; its first deploy has started. `qsr apps show {}` tells how it goes.",
                    a.name, a.subdomain
                );
                return Ok(());
            }
            if !json {
                println!("Created {} at https://{}.", a.name, a.host);
            }
            lifecycle::wait(&s.client, &a, "Deploying", "Deployed", json)
        }
        AppsCommand::Delete { app, yes } => {
            let a = apps::find(&s.client, &app)?;
            if !yes {
                confirm_delete(&a)?;
            }
            let _: Deleted = s.client.delete(&format!("/apps/{}", a.id))?;
            if json {
                return output::json(&a);
            }
            println!("Deleted {}.", a.name);
            Ok(())
        }
        AppsCommand::List => {
            let list = apps::list(&s.client)?;
            if json {
                return output::json(&list);
            }
            if list.is_empty() {
                println!("No applications yet.");
                return Ok(());
            }
            let rows: Vec<Vec<String>> = list
                .iter()
                .map(|a| {
                    vec![
                        a.name.clone(),
                        a.host.clone(),
                        a.deploy_type.clone(),
                        a.state.clone(),
                    ]
                })
                .collect();
            output::table_styled(&["NAME", "HOST", "TYPE", "STATE"], &rows, |col, cell| {
                if col == 3 {
                    output::state(cell)
                } else {
                    cell.to_string()
                }
            })
        }
        AppsCommand::Show { app } => {
            let a = apps::find(&s.client, &app)?;
            if json {
                return output::json(&a);
            }
            let state = match &a.uptime {
                Some(up) => format!("{}, up {up}", output::state(&a.state)),
                None => output::state(&a.state),
            };
            let source = a.source();
            let kind = if source.is_empty() {
                a.deploy_type.clone()
            } else {
                format!("{} ({source})", a.deploy_type)
            };
            println!("{}", output::bold(&a.name));
            let mut fields = vec![
                ("id", a.id.clone()),
                ("address", format!("https://{}", a.host)),
                ("type", kind),
                ("port", a.port.to_string()),
                ("state", state),
            ];
            // Only once there is something to say: a deploy under way, or the
            // reason the latest one failed.
            match &a.deploy {
                Some(d) if d.running => fields.push(("deploy", output::state("deploying"))),
                Some(d) => {
                    if let Some(err) = &d.error {
                        fields.push(("deploy", format!("{} {err}", output::state("error"))));
                    }
                }
                None => {}
            }
            output::fields(&fields)
        }
    }
}

/// Asks for the application's name to be typed back before deleting it, as
/// the dashboard does. Without a terminal to ask on, a delete needs --yes.
fn confirm_delete(a: &App) -> Result<()> {
    if !io::stdin().is_terminal() {
        bail!(
            "deleting {} needs confirmation; pass --yes to delete it without being asked",
            a.name
        );
    }
    eprintln!(
        "This deletes {} (https://{}) with its containers, its files and its data. It cannot be undone.",
        a.name, a.host
    );
    eprint!("Type its name to confirm: ");
    io::stderr().flush()?;
    let mut typed = String::new();
    io::stdin().lock().read_line(&mut typed)?;
    if typed.trim() != a.name {
        bail!("not deleted: the name did not match");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdomains_are_made_from_names() {
        assert_eq!(subdomain_from("My Blog").unwrap(), "my-blog");
        assert_eq!(subdomain_from("  API v2 (beta) ").unwrap(), "api-v2-beta");
        assert_eq!(subdomain_from("grafana").unwrap(), "grafana");
        assert!(subdomain_from("éé").is_err());
    }

    fn args(extra: &[&str]) -> CreateArgs {
        use clap::Parser;
        #[derive(Parser)]
        struct T {
            #[command(flatten)]
            a: CreateArgs,
        }
        let mut argv = vec!["t", "My Site"];
        argv.extend_from_slice(extra);
        T::try_parse_from(argv).unwrap().a
    }

    #[test]
    fn create_needs_exactly_one_source() {
        use clap::Parser;
        #[derive(Parser)]
        struct T {
            #[command(flatten)]
            _a: CreateArgs,
        }
        assert!(T::try_parse_from(["t", "x"]).is_err());
        assert!(T::try_parse_from(["t", "x", "--local", "--image", "nginx"]).is_err());
        assert!(NewApp::from_args(&args(&["--branch", "dev", "--local"])).is_err());
    }

    #[test]
    fn create_builds_the_request() {
        let new = NewApp::from_args(&args(&[
            "--git",
            "https://g/x.git",
            "--branch",
            "dev",
            "--domain",
            "a.com",
            "--domain",
            "b.com",
        ]))
        .unwrap();
        let v = serde_json::to_value(&new).unwrap();
        assert_eq!(v["deploy_type"], "git");
        assert_eq!(v["subdomain"], "my-site");
        assert_eq!(v["git_branch"], "dev");
        assert_eq!(v["custom_domains"], "a.com,b.com");
        assert!(
            v.get("port").is_none(),
            "unset fields are left to the server"
        );

        let local = NewApp::from_args(&args(&["--local", "-s", "Site", "-p", "8080"])).unwrap();
        let v = serde_json::to_value(&local).unwrap();
        assert_eq!(v["deploy_type"], "local");
        assert_eq!(v["subdomain"], "site");
        assert_eq!(v["port"], 8080);
        assert!(v.get("custom_domains").is_none());
    }
}
