//! Applications as the API reports them, and how a name typed on the command
//! line finds one.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::api::Client;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct App {
    pub id: String,
    pub name: String,
    pub subdomain: String,
    pub host: String,
    pub deploy_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    pub port: u16,
    /// "running", "stopped", "error", "deploying" or "not deployed".
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<String>,
}

impl App {
    /// Where the application is deployed from, in a few words.
    pub fn source(&self) -> String {
        match self.deploy_type.as_str() {
            "image" => self.image_ref.clone().unwrap_or_default(),
            "git" => match (&self.git_url, &self.git_branch) {
                (Some(url), Some(branch)) => format!("{url} @ {branch}"),
                (Some(url), None) => url.clone(),
                _ => String::new(),
            },
            _ => String::new(),
        }
    }
}

#[derive(Deserialize)]
struct AppList {
    apps: Vec<App>,
}

pub fn list(client: &Client) -> Result<Vec<App>> {
    Ok(client.get::<AppList>("/apps")?.apps)
}

/// Finds the application a command names, trying what is unique first: its
/// id, then its subdomain (one per application on a server), then its name,
/// exactly and then ignoring case. A name two applications share is refused
/// with both listed, rather than one of them picked behind the user's back.
pub fn find(client: &Client, wanted: &str) -> Result<App> {
    pick(list(client)?, wanted)
}

fn pick(apps: Vec<App>, wanted: &str) -> Result<App> {
    let unique = |pred: &dyn Fn(&App) -> bool| -> Option<App> {
        let mut found = apps.iter().filter(|a| pred(a));
        match (found.next(), found.next()) {
            (Some(a), None) => Some(a.clone()),
            _ => None,
        }
    };
    let found = unique(&|a| a.id == wanted)
        .or_else(|| unique(&|a| a.subdomain.eq_ignore_ascii_case(wanted)))
        .or_else(|| unique(&|a| a.name == wanted));
    if let Some(a) = found {
        return Ok(a);
    }
    let matches: Vec<&App> = apps
        .iter()
        .filter(|a| a.name.eq_ignore_ascii_case(wanted))
        .collect();
    match matches.as_slice() {
        [one] => Ok((*one).clone()),
        [] => bail!("no application is called {wanted}; `qsr apps` lists them"),
        many => {
            let list: Vec<String> = many
                .iter()
                .map(|a| format!("{} ({}, id {})", a.name, a.host, a.id))
                .collect();
            bail!("{wanted} could be {}; name it by id", list.join(" or "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, sub: &str) -> App {
        App {
            id: id.into(),
            name: name.into(),
            subdomain: sub.into(),
            host: format!("{sub}.example.com"),
            deploy_type: "image".into(),
            image_ref: Some("nginx".into()),
            git_url: None,
            git_branch: None,
            port: 80,
            state: "running".into(),
            uptime: None,
        }
    }

    fn apps() -> Vec<App> {
        vec![
            app("a1", "Blog", "blog"),
            app("b2", "Shop", "store"),
            app("c3", "Docs", "docs"),
            app("d4", "docs", "manual"),
        ]
    }

    #[test]
    fn finds_by_id_name_or_subdomain() {
        assert_eq!(pick(apps(), "a1").unwrap().name, "Blog");
        assert_eq!(pick(apps(), "blog").unwrap().id, "a1");
        assert_eq!(pick(apps(), "store").unwrap().id, "b2");
    }

    #[test]
    fn an_id_wins_over_a_name() {
        let mut list = apps();
        list.push(app("x9", "a1", "other"));
        assert_eq!(pick(list, "a1").unwrap().name, "Blog");
    }

    #[test]
    fn a_subdomain_wins_over_a_name() {
        // "docs" is c3's subdomain and d4's name: the subdomain is unique.
        assert_eq!(pick(apps(), "docs").unwrap().id, "c3");
    }

    #[test]
    fn an_exact_name_wins_over_a_case_blind_one() {
        let mut list = apps();
        list.push(app("e5", "Notes", "notes-a"));
        list.push(app("f6", "notes", "notes-b"));
        assert_eq!(pick(list, "notes").unwrap().id, "f6");
    }

    #[test]
    fn ambiguity_is_refused_with_the_candidates() {
        let mut list = apps();
        list.push(app("e5", "Notes", "notes-a"));
        list.push(app("f6", "NOTES", "notes-b"));
        let err = pick(list, "notes").unwrap_err().to_string();
        assert!(err.contains("id e5") && err.contains("id f6"), "{err}");
    }

    #[test]
    fn unknown_points_at_the_list() {
        let err = pick(apps(), "nope").unwrap_err().to_string();
        assert!(err.contains("qsr apps"), "{err}");
    }

    #[test]
    fn source_reads_per_type() {
        let mut a = app("a1", "Blog", "blog");
        assert_eq!(a.source(), "nginx");
        a.deploy_type = "git".into();
        a.git_url = Some("https://github.com/me/blog".into());
        a.git_branch = Some("main".into());
        assert_eq!(a.source(), "https://github.com/me/blog @ main");
    }
}
