//! The Quasar JSON API, as `qsr` speaks it.

use std::fmt;
use std::time::Duration;

use anyhow::{Context as _, Result};
use reqwest::StatusCode;
use reqwest::blocking::{Client as Http, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::VERSION;

/// An answer the server gave on purpose: a status and its message.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status {
            StatusCode::UNAUTHORIZED => {
                write!(f, "the server refused the token: {}", self.message)
            }
            StatusCode::NOT_FOUND if self.message.is_empty() => write!(
                f,
                "the server does not know this request; it may run a Quasar too old for this command"
            ),
            _ if self.message.is_empty() => write!(f, "the server answered {}", self.status),
            _ => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ApiError {}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

/// Who a token acts as, from `GET /api/v1/me`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Me {
    /// "personal" or "service".
    pub kind: String,
    pub token: String,
    pub user: Option<String>,
    pub role: String,
    pub expires_at: Option<String>,
    pub server_version: String,
}

impl Me {
    /// Who this is, as a sentence fragment: "alice (admin)", or for a service
    /// token, "the service token ci (admin)".
    pub fn describe(&self) -> String {
        match &self.user {
            Some(user) => format!("{user} ({})", self.role),
            None => format!("the service token {} ({})", self.token, self.role),
        }
    }
}

/// A connection to one Quasar server.
pub struct Client {
    base: String,
    token: String,
    http: Http,
}

/// How long an ordinary request may take. Following logs opts out.
const TIMEOUT: Duration = Duration::from_secs(30);

impl Client {
    pub fn new(url: &str, token: &str) -> Result<Self> {
        install_crypto();
        let http = Http::builder()
            .user_agent(format!("qsr/{VERSION}"))
            .timeout(TIMEOUT)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .context("could not set up the HTTP client")?;
        Ok(Self {
            base: url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            http,
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn request(&self, method: reqwest::Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, format!("{}/api/v1{path}", self.base))
            .bearer_auth(&self.token)
    }

    /// Sends a request and turns anything but a success into an [`ApiError`],
    /// or into an error naming the server when it could not be reached.
    fn send(&self, req: RequestBuilder) -> Result<Response> {
        let resp = req
            .send()
            .with_context(|| format!("could not reach {}", self.base))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let message = resp
            .json::<ErrorBody>()
            .map(|b| b.error)
            .unwrap_or_default();
        Err(ApiError { status, message }.into())
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self.send(self.request(reqwest::Method::GET, path))?;
        resp.json()
            .with_context(|| format!("unexpected answer from {}", self.base))
    }

    pub fn post<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self.send(self.request(reqwest::Method::POST, path))?;
        resp.json()
            .with_context(|| format!("unexpected answer from {}", self.base))
    }

    pub fn post_json<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let resp = self.send(self.request(reqwest::Method::POST, path).json(body))?;
        resp.json()
            .with_context(|| format!("unexpected answer from {}", self.base))
    }

    pub fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self.send(self.request(reqwest::Method::DELETE, path))?;
        resp.json()
            .with_context(|| format!("unexpected answer from {}", self.base))
    }

    pub fn put<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let resp = self.send(self.request(reqwest::Method::PUT, path).json(body))?;
        resp.json()
            .with_context(|| format!("unexpected answer from {}", self.base))
    }

    /// Opens a response to read as it arrives, with no overall time limit:
    /// a followed log stays open until the user stops it.
    pub fn stream(&self, path: &str, query: &[(&str, String)]) -> Result<Response> {
        let req = self
            .request(reqwest::Method::GET, path)
            .query(query)
            .timeout(Duration::from_secs(60 * 60 * 24 * 365));
        self.send(req)
    }

    pub fn me(&self) -> Result<Me> {
        self.get("/me")
    }
}

/// Picks the TLS provider once per process. reqwest is built without one, so
/// that the release builds need nothing but a C compiler for `ring`.
pub fn install_crypto() {
    // Fails only when a provider is already installed, which is the goal.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Reads an address as typed into its canonical form: a scheme (https
/// unless one is given) and no trailing slash.
pub fn normalize_url(input: &str) -> Result<String> {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() {
        anyhow::bail!("the server address is empty");
    }
    let url = if s.contains("://") {
        s.to_string()
    } else {
        format!("https://{s}")
    };
    let parsed = reqwest::Url::parse(&url).with_context(|| format!("{input} is not an address"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        anyhow::bail!("{input} is not an http or https address");
    }
    Ok(url)
}

/// Whether a token sent to this address would cross the network in clear.
pub fn is_plaintext_remote(url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url) else {
        return false;
    };
    u.scheme() == "http"
        && !matches!(
            u.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_get_https_and_lose_trailing_slashes() {
        assert_eq!(
            normalize_url("admin.example.com/").unwrap(),
            "https://admin.example.com"
        );
        assert_eq!(
            normalize_url(" http://127.0.0.1:18080 ").unwrap(),
            "http://127.0.0.1:18080"
        );
        assert!(normalize_url("").is_err());
        assert!(normalize_url("ftp://x").is_err());
    }

    #[test]
    fn plaintext_is_only_flagged_off_this_machine() {
        assert!(is_plaintext_remote("http://admin.example.com"));
        assert!(!is_plaintext_remote("https://admin.example.com"));
        assert!(!is_plaintext_remote("http://localhost:8080"));
        assert!(!is_plaintext_remote("http://127.0.0.1:18080"));
    }

    #[test]
    fn me_describes_both_kinds() {
        let mut me = Me {
            kind: "personal".into(),
            token: "laptop".into(),
            user: Some("alice".into()),
            role: "admin".into(),
            expires_at: None,
            server_version: "v0.5.2".into(),
        };
        assert_eq!(me.describe(), "alice (admin)");
        me.user = None;
        me.kind = "service".into();
        assert_eq!(me.describe(), "the service token laptop (admin)");
    }
}
