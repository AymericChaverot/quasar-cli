//! The configuration file: the servers `qsr` knows, each as a named context
//! holding its address and token, and which one commands use by default.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

/// Overrides where the configuration file lives, for tests and for anyone
/// who keeps several sets of contexts.
pub const CONFIG_ENV: &str = "QSR_CONFIG";

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// The context commands use when none is named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    #[serde(default)]
    pub contexts: BTreeMap<String, Context>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Context {
    /// The dashboard's address, without a trailing slash.
    pub url: String,
    pub token: String,
}

/// Where the configuration file is read from and written to.
pub fn default_path() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os(CONFIG_ENV) {
        return Ok(PathBuf::from(p));
    }
    let dirs = directories::BaseDirs::new().context("could not find your home directory")?;
    Ok(dirs.config_dir().join("quasar").join("config.toml"))
}

impl Config {
    /// Reads the file, or starts empty when there is none yet.
    pub fn load(path: &Path) -> Result<Self> {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).with_context(|| format!("could not read {}", path.display())),
        };
        toml::from_str(&text).with_context(|| format!("{} is not valid", path.display()))
    }

    /// Writes the file readable by its owner only, through a temporary file
    /// renamed into place, so an interrupted write never leaves half a file
    /// and the tokens are never readable by anyone else, even for a moment.
    pub fn save(&self, path: &Path) -> Result<()> {
        let dir = path
            .parent()
            .context("the configuration path has no directory")?;
        fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
        let text = toml::to_string_pretty(self)?;
        let tmp = path.with_extension("toml.tmp");
        {
            let mut f =
                private_file(&tmp).with_context(|| format!("could not write {}", tmp.display()))?;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        fs::rename(&tmp, path).with_context(|| format!("could not write {}", path.display()))
    }

    /// Adds or replaces a context, and makes it the current one when there is
    /// no current one yet.
    #[allow(dead_code, reason = "login uses it")]
    pub fn upsert(&mut self, name: &str, ctx: Context) {
        self.contexts.insert(name.to_string(), ctx);
        if self.current.is_none() {
            self.current = Some(name.to_string());
        }
    }

    pub fn use_context(&mut self, name: &str) -> Result<()> {
        if !self.contexts.contains_key(name) {
            bail!("there is no context named {name}{}", self.known());
        }
        self.current = Some(name.to_string());
        Ok(())
    }

    /// Removes a context. Removing the current one leaves none current rather
    /// than quietly pointing commands at another server.
    pub fn remove(&mut self, name: &str) -> Result<Context> {
        let ctx = self
            .contexts
            .remove(name)
            .with_context(|| format!("there is no context named {name}{}", self.known()))?;
        if self.current.as_deref() == Some(name) {
            self.current = None;
        }
        Ok(ctx)
    }

    /// The context a command runs against: the one named, or else the
    /// current one.
    #[allow(dead_code, reason = "commands that reach a server use it")]
    pub fn resolve(&self, name: Option<&str>) -> Result<(&str, &Context)> {
        let name = match name.or(self.current.as_deref()) {
            Some(n) => n,
            None if self.contexts.is_empty() => {
                bail!("not signed in to any server; run `qsr login <name> <url>` first")
            }
            None => bail!("no current context; pick one with `qsr context use <name>`"),
        };
        self.contexts
            .get_key_value(name)
            .map(|(k, v)| (k.as_str(), v))
            .with_context(|| format!("there is no context named {name}{}", self.known()))
    }

    fn known(&self) -> String {
        if self.contexts.is_empty() {
            String::new()
        } else {
            let names: Vec<&str> = self.contexts.keys().map(String::as_str).collect();
            format!(" (known: {})", names.join(", "))
        }
    }
}

#[cfg(unix)]
fn private_file(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

// On Windows the file sits in the user's own profile, which other accounts
// cannot read by default.
#[cfg(not(unix))]
fn private_file(path: &Path) -> std::io::Result<fs::File> {
    fs::File::create(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(url: &str) -> Context {
        Context {
            url: url.into(),
            token: "qsr_secret".into(),
        }
    }

    #[test]
    fn missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load(&dir.path().join("config.toml")).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let mut cfg = Config::default();
        cfg.upsert("prod", ctx("https://admin.a.com"));
        cfg.upsert("staging", ctx("https://admin.b.com"));
        cfg.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), cfg);
        assert!(!path.with_extension("toml.tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn saved_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        Config::default().save(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn first_context_becomes_current() {
        let mut cfg = Config::default();
        cfg.upsert("prod", ctx("https://a"));
        cfg.upsert("staging", ctx("https://b"));
        assert_eq!(cfg.current.as_deref(), Some("prod"));
    }

    #[test]
    fn resolve_prefers_the_named_context() {
        let mut cfg = Config::default();
        cfg.upsert("prod", ctx("https://a"));
        cfg.upsert("staging", ctx("https://b"));
        assert_eq!(cfg.resolve(None).unwrap().0, "prod");
        assert_eq!(cfg.resolve(Some("staging")).unwrap().1.url, "https://b");
        let err = cfg.resolve(Some("nope")).unwrap_err().to_string();
        assert!(err.contains("known: prod, staging"), "{err}");
    }

    #[test]
    fn removing_the_current_context_leaves_none_current() {
        let mut cfg = Config::default();
        cfg.upsert("prod", ctx("https://a"));
        cfg.upsert("staging", ctx("https://b"));
        cfg.remove("prod").unwrap();
        assert_eq!(cfg.current, None);
        let err = cfg.resolve(None).unwrap_err().to_string();
        assert!(err.contains("qsr context use"), "{err}");
    }

    #[test]
    fn nothing_configured_points_at_login() {
        let err = Config::default().resolve(None).unwrap_err().to_string();
        assert!(err.contains("qsr login"), "{err}");
    }

    #[test]
    fn use_unknown_context_fails() {
        let mut cfg = Config::default();
        assert!(cfg.use_context("prod").is_err());
    }
}
