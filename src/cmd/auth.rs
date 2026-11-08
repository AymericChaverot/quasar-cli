//! `qsr login`, `logout` and `whoami`.

use std::io::{self, BufRead, IsTerminal};

use anyhow::{Context as _, Result, bail};
use serde::Serialize;

use crate::api::{self, Client, Me};
use crate::config::{self, Config, Context};
use crate::{output, session};

/// Signs in to a server with a personal token, and saves it as a context
/// that becomes the current one.
pub fn login(name: &str, url: &str, token_stdin: bool) -> Result<()> {
    let url = api::normalize_url(url)?;
    if api::is_plaintext_remote(&url) {
        eprintln!("warning: {url} is not https; the token will cross the network in clear");
    }
    let token = read_token(&url, token_stdin)?;
    if !token.starts_with("qsr_") {
        bail!("that is not a Quasar token; they start with qsr_");
    }

    let me = Client::new(&url, &token)?
        .me()
        .with_context(|| format!("could not sign in to {url}"))?;

    let path = config::default_path()?;
    let mut cfg = Config::load(&path)?;
    cfg.upsert(
        name,
        Context {
            url: url.clone(),
            token,
        },
    );
    cfg.use_context(name)?;
    cfg.save(&path)?;

    println!("Signed in to {name} as {}.", me.describe());
    if me.kind == "service" {
        println!(
            "This is a service token: it works, but it never reads environments. \
             A personal token (Settings > Personal tokens) acts as your account."
        );
    }
    if let Some(expires) = &me.expires_at {
        println!("The token expires on {}.", day(expires));
    }
    Ok(())
}

/// Takes the token from stdin when asked, or else from a hidden prompt.
fn read_token(url: &str, from_stdin: bool) -> Result<String> {
    let token = if from_stdin {
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        line
    } else {
        if !io::stdin().is_terminal() {
            bail!("no terminal to ask for the token; pass it on stdin with --token-stdin");
        }
        eprintln!("Create a personal token in Settings > Personal tokens on {url}.");
        rpassword::prompt_password("Token: ").context("could not read the token")?
    };
    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("no token given");
    }
    Ok(token)
}

/// Forgets a context and its token: the named one, or else the current one.
pub fn logout(name: Option<&str>, context: Option<&str>) -> Result<()> {
    let path = config::default_path()?;
    let mut cfg = Config::load(&path)?;
    let name = cfg.resolve(name.or(context))?.0.to_string();
    cfg.remove(&name)?;
    cfg.save(&path)?;
    println!("Signed out of {name}.");
    println!(
        "The token itself works until it expires or is revoked in Settings > Personal tokens."
    );
    Ok(())
}

#[derive(Serialize)]
struct WhoAmI<'a> {
    context: Option<&'a str>,
    url: &'a str,
    #[serde(flatten)]
    me: &'a Me,
}

pub fn whoami(context: Option<&str>, json: bool) -> Result<()> {
    let s = session::open(context)?;
    let me = s.client.me()?;
    if json {
        return output::json(&WhoAmI {
            context: s.context.as_deref(),
            url: s.client.base(),
            me: &me,
        });
    }
    match &s.context {
        Some(c) => println!("{} on {c} ({})", me.describe(), s.client.base()),
        None => println!("{} on {}", me.describe(), s.client.base()),
    }
    let expiry = match &me.expires_at {
        Some(e) => format!("expires on {}", day(e)),
        None => "never expires".to_string(),
    };
    println!("{} token {}, {expiry}", me.kind, me.token);
    println!("Quasar {}", me.server_version);
    Ok(())
}

/// The date part of an RFC 3339 timestamp.
fn day(ts: &str) -> &str {
    ts.get(..10).unwrap_or(ts)
}
