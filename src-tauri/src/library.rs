//! Quicklinks and snippets, stored as JSON next to the extensions:
//! `$XDG_DATA_HOME/heyo/quicklinks.json` and `snippets.json`. The runtime's
//! "Create …" and "Search …" commands edit the same files.

use std::fs;
use std::path::PathBuf;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quicklink {
    pub id: String,
    pub name: String,
    /// A URL or path; may contain `{query}` and other placeholders.
    pub link: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    pub name: String,
    pub text: String,
    /// Typing it exactly in the root search selects the snippet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
}

fn path(name: &str) -> PathBuf {
    crate::paths::data().join(name)
}

fn load<T: DeserializeOwned>(name: &str) -> Vec<T> {
    let Ok(text) = fs::read_to_string(path(name)) else { return Vec::new() };
    serde_json::from_str(&text).unwrap_or_else(|e| {
        eprintln!("heyo: ignoring invalid {name}: {e}");
        Vec::new()
    })
}

pub fn quicklinks() -> Vec<Quicklink> {
    load("quicklinks.json")
}

pub fn snippets() -> Vec<Snippet> {
    load("snippets.json")
}

impl Quicklink {
    /// The site's favicon for web links, else a link icon.
    pub fn icon(&self) -> String {
        if let Some(icon) = &self.icon {
            return icon.clone();
        }
        let host = self
            .link
            .strip_prefix("https://")
            .or_else(|| self.link.strip_prefix("http://"))
            .and_then(|rest| rest.split(['/', '?', '#']).next())
            .filter(|h| !h.is_empty() && !h.contains('{'));
        match host {
            Some(host) => format!("https://www.google.com/s2/favicons?domain={host}&sz=64"),
            None => "link-16".into(),
        }
    }
}
