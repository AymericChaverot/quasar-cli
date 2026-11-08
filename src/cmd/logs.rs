//! `qsr logs`: an application's recent output, or a live stream of it.

use std::io::{self, BufRead, BufReader, Write};

use anyhow::{Context as _, Result};
use serde::Deserialize;

use crate::{apps, output, session};

#[derive(Deserialize)]
struct Line {
    time: Option<String>,
    line: String,
}

pub struct Options {
    pub lines: u32,
    pub follow: bool,
    pub timestamps: bool,
}

pub fn run(app: &str, opts: Options, context: Option<&str>, json: bool) -> Result<()> {
    let s = session::open(context)?;
    let a = apps::find(&s.client, app)?;
    let resp = s.client.stream(
        &format!("/apps/{}/logs", a.id),
        &[
            ("tail", opts.lines.to_string()),
            ("follow", opts.follow.to_string()),
        ],
    )?;

    copy(BufReader::new(resp), &mut io::stdout().lock(), &opts, json)
}

/// Writes the server's lines out as they arrive, until the stream ends or
/// whoever reads the output stops.
fn copy(input: impl BufRead, out: &mut impl Write, opts: &Options, json: bool) -> Result<()> {
    for raw in input.lines() {
        let raw = raw.context("the log stream broke off")?;
        if raw.is_empty() {
            continue;
        }
        // With --json the server's own lines go through as they are, which
        // is already one JSON object per line.
        let written = if json {
            writeln!(out, "{raw}")
        } else {
            let l: Line = serde_json::from_str(&raw)
                .with_context(|| format!("unexpected log line from the server: {raw}"))?;
            writeln!(out, "{}", render(&l, opts.timestamps))
        };
        // A closed pipe (`qsr logs blog | head`) is the reader being done,
        // not an error.
        match written {
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => return Ok(()),
            other => other?,
        }
        if opts.follow {
            out.flush()?;
        }
    }
    Ok(())
}

fn render(l: &Line, timestamps: bool) -> String {
    match (&l.time, timestamps) {
        (Some(t), true) => format!("{} {}", output::dim(t), l.line),
        (None, true) => format!("{} {}", output::dim(&" ".repeat(24)), l.line),
        _ => l.line.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(timestamps: bool) -> Options {
        Options {
            lines: 200,
            follow: false,
            timestamps,
        }
    }

    const STREAM: &str = "{\"time\":\"2026-11-09T10:00:00Z\",\"line\":\"GET / 200\"}\n\
                          \n\
                          {\"time\":null,\"line\":\"boot\"}\n";

    #[test]
    fn copies_each_line_as_text() {
        let mut out = Vec::new();
        copy(STREAM.as_bytes(), &mut out, &opts(false), false).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "GET / 200\nboot\n");
    }

    #[test]
    fn json_passes_the_lines_through() {
        let mut out = Vec::new();
        copy(STREAM.as_bytes(), &mut out, &opts(false), true).unwrap();
        let got = String::from_utf8(out).unwrap();
        assert_eq!(got.lines().count(), 2);
        assert!(got.starts_with("{\"time\":\"2026-11-09T10:00:00Z\""));
    }

    #[test]
    fn a_garbled_line_is_an_error() {
        let mut out = Vec::new();
        assert!(copy("not json\n".as_bytes(), &mut out, &opts(false), false).is_err());
    }

    #[test]
    fn timestamps_only_when_asked() {
        let l = Line {
            time: Some("2026-11-09T10:00:00.123Z".into()),
            line: "GET / 200".into(),
        };
        assert_eq!(render(&l, false), "GET / 200");
        assert_eq!(render(&l, true), "2026-11-09T10:00:00.123Z GET / 200");
        let bare = Line {
            time: None,
            line: "boot".into(),
        };
        assert_eq!(render(&bare, true).trim_start(), "boot");
    }
}
