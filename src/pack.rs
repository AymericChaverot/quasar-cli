//! Packing a folder for `qsr deploy <app> <folder>`: what Docker and git
//! would leave out stays out, and the rest goes into a gzipped tar.

use std::path::{Component, Path};

use anyhow::{Context as _, Result, bail};
use flate2::Compression;
use flate2::write::GzEncoder;
use ignore::WalkBuilder;

/// The names docker compose looks for at the root of a project.
const COMPOSE_FILES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// A folder, packed.
pub struct Packed {
    pub archive: Vec<u8>,
    pub files: usize,
    /// The files' size before compression.
    pub bytes: u64,
}

/// Refuses a folder the server could not build: it needs a Dockerfile or a
/// compose file at its root, as a repository does.
pub fn check_buildable(dir: &Path) -> Result<()> {
    if !dir.is_dir() {
        bail!("{} is not a folder", dir.display());
    }
    let has = |name: &str| dir.join(name).is_file();
    if has("Dockerfile") || COMPOSE_FILES.iter().any(|f| has(f)) {
        return Ok(());
    }
    bail!(
        "{} has neither a Dockerfile nor a compose file at its root, so there is nothing to build",
        dir.display()
    )
}

/// Packs a folder into a gzipped tar, leaving out `.git` and whatever its
/// `.gitignore` and `.dockerignore` files exclude — read the way git reads a
/// `.gitignore`, at every level of the folder, whether or not it is a
/// repository. Symbolic links are kept as links, not followed.
pub fn pack(dir: &Path) -> Result<Packed> {
    let mut walk = WalkBuilder::new(dir);
    walk.hidden(false)
        .parents(false)
        .ignore(false)
        .git_global(false)
        .git_exclude(false)
        .require_git(false)
        .git_ignore(true)
        .follow_links(false)
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(|e| e.file_name() != ".git");
    walk.add_custom_ignore_filename(".dockerignore");

    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    tar.follow_symlinks(false);
    let (mut files, mut bytes) = (0, 0);
    for entry in walk.build() {
        let entry = entry.with_context(|| format!("could not read {}", dir.display()))?;
        if entry.depth() == 0 {
            continue;
        }
        let rel = entry.path().strip_prefix(dir).unwrap_or(entry.path());
        let name = archive_name(rel)?;
        let Some(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_file() {
            files += 1;
            bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
        tar.append_path_with_name(entry.path(), &name)
            .with_context(|| format!("could not pack {}", rel.display()))?;
    }
    let archive = tar
        .into_inner()
        .and_then(|gz| gz.finish())
        .context("could not finish the archive")?;
    Ok(Packed {
        archive,
        files,
        bytes,
    })
}

/// A path inside the folder as the archive names it: relative, with forward
/// slashes whatever the platform.
fn archive_name(rel: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for c in rel.components() {
        match c {
            Component::Normal(p) => parts.push(
                p.to_str()
                    .with_context(|| format!("{} is not a valid UTF-8 name", rel.display()))?,
            ),
            _ => bail!("unexpected path {} in the folder", rel.display()),
        }
    }
    Ok(parts.join("/"))
}

/// A size as people read it: 812 bytes, 14.2 KB, 3.1 MB.
pub fn human_size(n: u64) -> String {
    const KB: f64 = 1024.0;
    let f = n as f64;
    if f >= KB * KB {
        format!("{:.1} MB", f / KB / KB)
    } else if f >= KB {
        format!("{:.1} KB", f / KB)
    } else {
        format!("{n} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::fs;

    fn names(p: &Packed) -> Vec<String> {
        let mut ar = tar::Archive::new(GzDecoder::new(&p.archive[..]));
        let mut out: Vec<String> = ar
            .entries()
            .unwrap()
            .map(|e| {
                e.unwrap()
                    .path()
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn packs_what_git_and_docker_would_keep() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let write = |p: &str, s: &str| {
            let path = d.join(p);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, s).unwrap();
        };
        write("Dockerfile", "FROM nginx\n");
        write("site/index.html", "<h1>hi</h1>");
        write(".gitignore", "node_modules/\n*.log\n");
        write(".dockerignore", "secrets.txt\n");
        write("node_modules/x/index.js", "x");
        write("debug.log", "noise");
        write("secrets.txt", "hunter2");
        write("site/.env.example", "A=1");
        write(".git/HEAD", "ref: refs/heads/main");

        let p = pack(d).unwrap();
        assert_eq!(
            names(&p),
            [
                ".dockerignore",
                ".gitignore",
                "Dockerfile",
                "site",
                "site/.env.example",
                "site/index.html",
            ]
        );
        assert_eq!(p.files, 5);
        check_buildable(d).unwrap();
    }

    #[test]
    fn a_folder_needs_something_to_build() {
        let dir = tempfile::tempdir().unwrap();
        assert!(check_buildable(dir.path()).is_err());
        fs::write(dir.path().join("compose.yaml"), "services: {}").unwrap();
        check_buildable(dir.path()).unwrap();
        assert!(check_buildable(&dir.path().join("missing")).is_err());
    }

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(human_size(812), "812 bytes");
        assert_eq!(human_size(14_540), "14.2 KB");
        assert_eq!(human_size(3_250_586), "3.1 MB");
    }
}
