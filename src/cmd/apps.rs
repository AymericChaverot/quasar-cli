//! `qsr apps`: list applications and show one.

use anyhow::Result;
use clap::Subcommand;

use crate::{apps, output, session};

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
}

pub fn run(cmd: Option<AppsCommand>, context: Option<&str>, json: bool) -> Result<()> {
    let s = session::open(context)?;
    match cmd.unwrap_or(AppsCommand::List) {
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
            output::fields(&[
                ("id", a.id.clone()),
                ("address", format!("https://{}", a.host)),
                ("type", kind),
                ("port", a.port.to_string()),
                ("state", state),
            ])
        }
    }
}
