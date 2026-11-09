//! `qsr deploy`, `restart`, `start` and `stop`.

use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use serde::Deserialize;

use crate::api::Client;
use crate::apps::{self, App};
use crate::{output, pack, session};

/// How often a running deploy is asked about.
const POLL: Duration = Duration::from_secs(1);
/// How often the progress line is redrawn, which is how fast its dots move.
const FRAME: Duration = Duration::from_millis(250);
/// The dots after the name, one frame each, padded so that the time after
/// them stays put.
const DOTS: [&str; 4] = [".  ", ".. ", "...", "   "];

#[derive(Deserialize)]
struct Accepted {}

/// Starts a deploy and, unless detached, waits for it to finish and reports
/// how it went, failing when it failed.
///
/// An application deployed from a local folder is sent the folder first —
/// the one given, or the current one.
pub fn deploy(
    app: &str,
    folder: Option<&Path>,
    detach: bool,
    context: Option<&str>,
    json: bool,
) -> Result<()> {
    let s = session::open(context)?;
    let a = apps::find(&s.client, app)?;
    if a.deploy_type == "local" {
        upload(&s.client, &a, folder.unwrap_or(Path::new(".")), json)?;
    } else if let Some(folder) = folder {
        bail!(
            "{} is deployed from {}, not from a folder; only an application created with \
             `qsr apps create --local` takes one, so {} was not sent",
            a.name,
            match a.deploy_type.as_str() {
                "git" => "its repository",
                "compose" => "its compose file",
                _ => "its image",
            },
            folder.display()
        );
    } else {
        let _: Accepted = s.client.post(&format!("/apps/{}/deploy", a.id))?;
    }
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

/// Redeploys an application from what the server already has — for one
/// deployed from a folder, the folder uploaded last — and waits for the
/// outcome. It is what applies a changed environment.
pub fn redeploy(client: &Client, a: &App, json: bool) -> Result<()> {
    let _: Accepted = client.post(&format!("/apps/{}/deploy", a.id))?;
    wait(client, a, "Deploying", "Deployed", json)
}

/// Packs a folder and sends it to an application deployed from one, which
/// starts its deploy. Refuses early what the server would refuse: a folder
/// with nothing to build, or one larger than the server accepts.
fn upload(client: &Client, a: &App, folder: &Path, json: bool) -> Result<()> {
    pack::check_buildable(folder)?;
    let limit = client.me()?.upload_max_bytes;
    let packed = pack::pack(folder)?;
    let size = packed.archive.len() as u64;
    if let Some(limit) = limit.filter(|&l| size > l) {
        bail!(
            "{} is {} once compressed, and the server accepts at most {}; \
             leave more out with .dockerignore, or ask an admin to raise the limit in Settings",
            folder.display(),
            pack::human_size(size),
            pack::human_size(limit)
        );
    }
    if !json {
        eprintln!(
            "Uploading {} ({} {}, {}, {} compressed)...",
            folder.display(),
            packed.files,
            if packed.files == 1 { "file" } else { "files" },
            pack::human_size(packed.bytes),
            pack::human_size(size)
        );
    }
    let _: Accepted = client.put_archive(&format!("/apps/{}/source", a.id), packed.archive)?;
    Ok(())
}

/// Waits for the deploy just started on an application to end, reporting it
/// as `verb` while it runs and `done` once it worked, and fails when it
/// failed. With --json, prints the application as it ends up.
pub fn wait(client: &Client, a: &App, verb: &str, done: &str, json: bool) -> Result<()> {
    let started = Instant::now();
    let progress = Progress::new(json);
    let mut frame = 0;
    let mut next_poll = started + POLL;
    let last = loop {
        // The line is redrawn several times between two questions to the
        // server, so the dots move; without a terminal there is no line, and
        // the wait is one sleep per question.
        if progress.on {
            progress.tick(verb, &a.name, started.elapsed(), frame);
            frame += 1;
            thread::sleep(FRAME);
            if Instant::now() < next_poll {
                continue;
            }
        } else {
            thread::sleep(POLL);
        }
        next_poll = Instant::now() + POLL;
        let now: App = client.get(&format!("/apps/{}", a.id))?;
        match status(&now) {
            Status::Running => {}
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

    fn tick(&self, verb: &str, name: &str, elapsed: Duration, frame: usize) {
        if self.on {
            let mut err = io::stderr().lock();
            let dots = DOTS[frame % DOTS.len()];
            let _ = write!(err, "\r\x1b[2K{verb} {name}{dots} {}", human(elapsed));
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
