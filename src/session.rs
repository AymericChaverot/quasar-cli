//! Which server a command talks to, and with which token.

use anyhow::{Result, bail};

use crate::api::Client;
use crate::config::{self, Config};

/// Overrides the context's address, for pipelines that skip `qsr login`.
pub const URL_ENV: &str = "QSR_URL";
/// Overrides the context's token.
pub const TOKEN_ENV: &str = "QSR_TOKEN";

/// The server a command runs against.
pub struct Session {
    /// The context it came from, when it came from one.
    pub context: Option<String>,
    pub client: Client,
}

/// Resolves the server from the environment first, then from the named or
/// current context.
///
/// QSR_URL and QSR_TOKEN together need no configuration at all. QSR_TOKEN
/// alone swaps the token of the context; QSR_URL alone is refused, since
/// sending a stored token to an address it was not stored for would hand it
/// to whatever answers there.
pub fn open(context: Option<&str>) -> Result<Session> {
    let url = env(URL_ENV);
    let token = env(TOKEN_ENV);
    match (url, token) {
        (Some(url), Some(token)) => {
            let url = crate::api::normalize_url(&url)?;
            Ok(Session {
                context: None,
                client: Client::new(&url, &token)?,
            })
        }
        (Some(_), None) => bail!("{URL_ENV} is set without {TOKEN_ENV}; set both, or neither"),
        (None, token) => {
            let cfg = Config::load(&config::default_path()?)?;
            let (name, ctx) = cfg.resolve(context)?;
            let token = token.unwrap_or_else(|| ctx.token.clone());
            Ok(Session {
                context: Some(name.to_string()),
                client: Client::new(&ctx.url, &token)?,
            })
        }
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}
