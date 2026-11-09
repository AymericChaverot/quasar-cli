//! `qsr deploy`, `restart`, `start` and `stop`.

use std::io::{self, IsTerminal, Write};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use serde::Deserialize;

use crate::api::Client;
use crate::apps::{self, App};
use crate::{output, session};

/// How often a running deploy is asked about.
const POLL: Duration = Duration::from_secs(2);

#[derive(Deserialize)]
struct Accepted {}

/// Starts a deploy and, unless detached, waits for it to finish and reports
/// how it went, failing when it failed.
pub fn deploy(app: &str, detach: bool, context: Option<&str>, json: bool) -> Result<()> {
    let s = session::open(context)?;
    let a = apps::find(&s.client, app)?;
    let _: Accepted = s.client.post(&format!("/apps/{}/deploy", a.id))?;
    if detach {
        if json {
            return output::json(&a);
        }
        println!(
            "Deploying {}. `qsr apps show {}` tells how it goes.",
            a.name, a.subdomain
        );
        return Ok(());
    }

    wait(&s.client, &a, "Deploying", "Deployed", json)
}

/// Waits for the deploy just started on an application to end, reporting it
/// as `verb` while it runs and `done` once it worked, and fails when it
/// failed. With --json, prints the application as it ends up.
pub fn wait(client: &Client, a: &App, verb: &str, done: &str, json: bool) -> Result<()> {
    let started = Instant::now();
    let progress = Progress::new(json);
    let last = loop {
        thread::sleep(POLL);
        let now: App = client.get(&format!("/apps/{}", a.id))?;
        match status(&now) {
            Status::Running => progress.tick(verb, &a.name, started.elapsed()),
            Status::Unknown => {
                progress.clear();
                bail!(
                    "the deploy of {} started, but this server does not report how it goes; \
                     update Quasar to follow deploys from qsr",
                    a.name
                );
            }
            Status::Done => break now,
        }
    };
    progress.clear();

    let took = human(started.elapsed());
    if json {
        output::json(&last)?;
    }
    match last.deploy.as_ref().and_then(|d| d.error.as_deref()) {
        Some(err) => bail!("{} failed to deploy after {took}: {err}", a.name),
        None => {
            if !json {
                println!("{done} {} in {took}.", a.name);
            }
            Ok(())
        }
    }
}

#[derive(Debug, PartialEq)]
enum Status {
    Running,
    Done,
    /// The server does not report deploys at all.
    Unknown,
}

fn status(app: &App) -> Status {
    match &app.deploy {
        None => Status::Unknown,
        Some(d) if d.running => Status::Running,
        Some(_) => Status::Done,
    }
}

#[derive(Deserialize)]
struct Done {
    status: String,
}

/// Starts, stops or restarts an application, which the server does before it
/// answers.
pub fn lifecycle(action: &str, app: &str, context: Option<&str>, json: bool) -> Result<()> {
    let s = session::open(context)?;
    let a = apps::find(&s.client, app)?;
    let done: Done = s.client.post(&format!("/apps/{}/{action}", a.id))?;
    if json {
        let now: App = s.client.get(&format!("/apps/{}", a.id))?;
        return output::json(&now);
    }
    println!("{} {}.", capitalise(&done.status), a.name);
    Ok(())
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// A duration as people say it: 42s, 3m 05s.
fn human(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else {
        format!("{}m {:02}s", s / 60, s % 60)
    }
}

/// The line that says a deploy is still going, redrawn in place on stderr.
/// Only on a terminal: in a pipeline or with --json, the wait is silent.
struct Progress {
    on: bool,
}

impl Progress {
    fn new(json: bool) -> Self {
        Self {
            on: !json && io::stderr().is_terminal(),
        }
    }

    fn tick(&self, verb: &str, name: &str, elapsed: Duration) {
        if self.on {
            let mut err = io::stderr().lock();
            let _ = write!(err, "\r\x1b[2K{verb} {name}... {}", human(elapsed));
            let _ = err.flush();
        }
    }

    fn clear(&self) {
        if self.on {
            let mut err = io::stderr().lock();
            let _ = write!(err, "\r\x1b[2K");
            let _ = err.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::DeployState;

    fn app(deploy: Option<DeployState>) -> App {
        serde_json::from_value::<App>(serde_json::json!({
            "id": "a1", "name": "blog", "subdomain": "blog", "host": "blog.x",
            "deploy_type": "image", "port": 80, "state": "running"
        }))
        .map(|mut a| {
            a.deploy = deploy;
            a
        })
        .unwrap()
    }

    #[test]
    fn status_follows_the_deploy_field() {
        assert_eq!(status(&app(None)), Status::Unknown);
        let running = DeployState {
            running: true,
            error: None,
        };
        assert_eq!(status(&app(Some(running))), Status::Running);
        let failed = DeployState {
            running: false,
            error: Some("pull failed".into()),
        };
        assert_eq!(status(&app(Some(failed))), Status::Done);
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(human(Duration::from_secs(42)), "42s");
        assert_eq!(human(Duration::from_secs(185)), "3m 05s");
    }

    #[test]
    fn statuses_become_sentences() {
        assert_eq!(capitalise("restarted"), "Restarted");
        assert_eq!(capitalise(""), "");
    }
}
