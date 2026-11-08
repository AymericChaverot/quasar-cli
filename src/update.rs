//! Keeping `qsr` itself up to date: `qsr update`, and the once-a-day line that
//! says a new release is out.
//!
//! Releases are the GitHub releases of this repository. Each carries one
//! archive per target and a SHA256SUMS file; nothing replaces the running
//! binary before its archive has matched its checksum.

use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{VERSION, config};

const REPO: &str = "AymericChaverot/quasar-cli";
/// Turns the daily check off.
pub const NO_CHECK_ENV: &str = "QSR_NO_UPDATE_CHECK";
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// The target this binary was built for, as release archives are named.
/// A Linux build for glibc takes the static musl archive, which is the only
/// Linux one there is and runs everywhere.
fn target() -> String {
    env!("QSR_TARGET").replace("-unknown-linux-gnu", "-unknown-linux-musl")
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn http(timeout: Duration) -> Result<reqwest::blocking::Client> {
    crate::api::install_crypto();
    reqwest::blocking::Client::builder()
        .user_agent(format!("qsr/{VERSION}"))
        .timeout(timeout)
        .build()
        .context("could not set up the HTTP client")
}

fn latest_release(http: &reqwest::blocking::Client) -> Result<Release> {
    let resp = http
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .send()
        .context("could not reach GitHub")?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("{REPO} has no release published yet");
    }
    resp.error_for_status()
        .context("could not ask GitHub for the latest release")?
        .json()
        .context("unexpected answer from GitHub")
}

fn parse_version(tag: &str) -> Result<Version> {
    Version::parse(tag.trim_start_matches('v')).with_context(|| format!("{tag} is not a version"))
}

fn current() -> Result<Version> {
    parse_version(VERSION)
}

/// A build made outside the release workflow: nothing to compare it with.
fn is_dev_build() -> bool {
    current().map(|v| !v.pre.is_empty()).unwrap_or(true)
}

/// `qsr update`: installs the latest release over the running binary, or
/// with `check_only`, says whether there is one.
pub fn run(check_only: bool) -> Result<()> {
    let http = http(Duration::from_secs(120))?;
    let release = latest_release(&http)?;
    let latest = parse_version(&release.tag_name)?;
    remember(&latest);

    if is_dev_build() {
        println!("This is a development build ({VERSION}); the latest release is {latest}.");
        if !check_only {
            println!("Install a release with the install script in the README instead.");
        }
        return Ok(());
    }
    let have = current()?;
    if latest <= have {
        println!("qsr {have} is the latest release.");
        return Ok(());
    }
    if check_only {
        println!("qsr {latest} is out (you have {have}). Run `qsr update` to install it.");
        return Ok(());
    }

    let target = target();
    let archive_ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    let name = format!("qsr-{latest}-{target}.{archive_ext}");
    let asset = |n: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == n)
            .with_context(|| format!("the release {latest} has no {n}"))
    };
    let archive_url = &asset(&name)?.browser_download_url;
    let sums_url = &asset("SHA256SUMS")?.browser_download_url;

    println!("Downloading qsr {latest} for {target}...");
    let archive = download(&http, archive_url)?;
    let sums = String::from_utf8(download(&http, sums_url)?)
        .context("the release's checksums are not text")?;
    verify(&archive, &sums, &name)?;

    let binary = extract(&archive, &format!("qsr-{latest}-{target}"))?;
    install(&binary)?;
    println!("Updated qsr from {have} to {latest}.");
    Ok(())
}

fn download(http: &reqwest::blocking::Client, url: &str) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    http.get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("could not download {url}"))?
        .read_to_end(&mut body)
        .with_context(|| format!("the download of {url} broke off"))?;
    Ok(body)
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Checks an archive against its line in a `sha256sum` listing.
fn verify(archive: &[u8], sums: &str, name: &str) -> Result<()> {
    let expected = sums
        .lines()
        .find_map(|line| {
            let (hash, file) = line.split_once(char::is_whitespace)?;
            (file.trim().trim_start_matches('*') == name).then(|| hash.to_ascii_lowercase())
        })
        .with_context(|| format!("{name} is not in the release's checksums"))?;
    if sha256_hex(archive) != expected {
        bail!("{name} does not match its checksum; nothing was installed");
    }
    Ok(())
}

/// Pulls the binary out of a release archive, whose files sit in a folder
/// named after the archive.
fn extract(archive: &[u8], folder: &str) -> Result<Vec<u8>> {
    let wanted = format!("{folder}/{}", binary_name());
    #[cfg(windows)]
    {
        let mut zip = zip::ZipArchive::new(io::Cursor::new(archive))
            .context("the downloaded archive is not a zip")?;
        let mut file = zip
            .by_name(&wanted)
            .with_context(|| format!("the archive has no {wanted}"))?;
        let mut out = Vec::new();
        file.read_to_end(&mut out)?;
        Ok(out)
    }
    #[cfg(not(windows))]
    {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
        for entry in tar
            .entries()
            .context("the downloaded archive is not a tar.gz")?
        {
            let mut entry = entry?;
            if entry.path()?.to_string_lossy() == wanted {
                let mut out = Vec::new();
                entry.read_to_end(&mut out)?;
                return Ok(out);
            }
        }
        bail!("the archive has no {wanted}")
    }
}

fn binary_name() -> &'static str {
    if cfg!(windows) { "qsr.exe" } else { "qsr" }
}

/// Puts the new binary in place of the running one. It is written beside the
/// current binary first, so the swap is a rename on the same filesystem; on
/// Windows, where a running binary cannot be overwritten, self-replace moves
/// the old one aside.
fn install(binary: &[u8]) -> Result<()> {
    let exe = std::env::current_exe().context("could not find the running qsr")?;
    let dir = exe.parent().context("the running qsr has no directory")?;
    let staged = (|| -> io::Result<tempfile::NamedTempFile> {
        let mut f = tempfile::Builder::new()
            .prefix(".qsr-update")
            .tempfile_in(dir)?;
        io::Write::write_all(&mut f, binary)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.as_file()
                .set_permissions(fs::Permissions::from_mode(0o755))?;
        }
        Ok(f)
    })();
    let staged = match staged {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => bail!(
            "{} is not writable by you; update with the install script instead, or run this again with the rights to write there",
            dir.display()
        ),
        Err(e) => return Err(e).context("could not write the new binary"),
    };
    self_replace::self_replace(staged.path()).context("could not swap in the new binary")?;
    Ok(())
}

#[derive(Serialize, Deserialize, Default)]
struct CheckState {
    /// When GitHub was last asked, in seconds since the epoch.
    checked_at: u64,
    /// The latest release it named.
    latest: Option<String>,
}

fn state_path() -> Option<PathBuf> {
    config::default_path()
        .ok()
        .map(|p| p.with_file_name("update-check.json"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn load_state(path: &Path) -> CheckState {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn remember(latest: &Version) {
    if let Some(path) = state_path() {
        let state = CheckState {
            checked_at: now(),
            latest: Some(latest.to_string()),
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(&path, serde_json::to_string(&state).unwrap_or_default());
    }
}

/// After a command: at most once a day, asks GitHub for the latest release,
/// briefly, and says on stderr when there is a newer one. Never in a
/// pipeline, never for a development build, never when turned off, and never
/// at the cost of the command: every failure is silent.
pub fn notify_if_newer() {
    if is_dev_build()
        || !io::stderr().is_terminal()
        || std::env::var_os(NO_CHECK_ENV).is_some_and(|v| !v.is_empty())
    {
        return;
    }
    let Some(path) = state_path() else { return };
    let mut state = load_state(&path);
    if now().saturating_sub(state.checked_at) >= CHECK_EVERY.as_secs() {
        let fetched = http(Duration::from_secs(2))
            .and_then(|h| latest_release(&h))
            .and_then(|r| parse_version(&r.tag_name));
        match fetched {
            Ok(v) => {
                remember(&v);
                state.latest = Some(v.to_string());
            }
            // Offline, rate-limited: try again tomorrow rather than at every
            // command, without forgetting what was last known.
            Err(_) => {
                let _ = fs::write(
                    &path,
                    serde_json::to_string(&CheckState {
                        checked_at: now(),
                        latest: state.latest.clone(),
                    })
                    .unwrap_or_default(),
                );
            }
        }
    }
    if let (Some(latest), Ok(have)) = (
        state.latest.as_deref().and_then(|v| Version::parse(v).ok()),
        current(),
    ) && latest > have
    {
        eprintln!("\nqsr {latest} is out (you have {have}). Run `qsr update` to install it.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_are_matched_by_file_name() {
        let archive = b"archive bytes";
        let hash = sha256_hex(archive);
        let sums = format!(
            "{}  other.tar.gz\n{hash}  qsr-1.0.0-x.tar.gz\n",
            "0".repeat(64)
        );
        assert!(verify(archive, &sums, "qsr-1.0.0-x.tar.gz").is_ok());
        assert!(verify(b"tampered", &sums, "qsr-1.0.0-x.tar.gz").is_err());
        assert!(verify(archive, &sums, "missing.tar.gz").is_err());
    }

    #[test]
    fn sha256_matches_the_known_value() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn versions_compare_as_releases() {
        assert!(parse_version("v0.2.0").unwrap() > parse_version("0.1.9").unwrap());
        assert!(!parse_version("0.0.0-dev").unwrap().pre.is_empty());
        assert!(parse_version("nope").is_err());
    }

    #[test]
    fn linux_builds_take_the_musl_archive() {
        let t = target();
        assert!(!t.contains("linux-gnu"), "{t}");
    }

    #[cfg(not(windows))]
    #[test]
    fn extracts_the_binary_from_a_release_archive() {
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (path, data) in [
            ("qsr-1.0.0-x/README.md", &b"readme"[..]),
            ("qsr-1.0.0-x/qsr", &b"binary"[..]),
        ] {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o755);
            h.set_cksum();
            tar.append_data(&mut h, path, data).unwrap();
        }
        let gz = tar.into_inner().unwrap().finish().unwrap();
        assert_eq!(extract(&gz, "qsr-1.0.0-x").unwrap(), b"binary");
        assert!(extract(&gz, "qsr-2.0.0-x").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn extracts_the_binary_from_a_release_archive() {
        use std::io::Write;
        let mut buf = io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("qsr-1.0.0-x/README.md", opts).unwrap();
            zip.write_all(b"readme").unwrap();
            zip.start_file("qsr-1.0.0-x/qsr.exe", opts).unwrap();
            zip.write_all(b"binary").unwrap();
            zip.finish().unwrap();
        }
        let data = buf.into_inner();
        assert_eq!(extract(&data, "qsr-1.0.0-x").unwrap(), b"binary");
        assert!(extract(&data, "qsr-2.0.0-x").is_err());
    }
}
