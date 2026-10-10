//! Where copies come from: the application behind the window class a copy
//! was made in, for showing it, and for never recording from the ones the
//! user names.
//!
//! A copy carries the class of the window that was in front. An installed
//! application is matched to it by the names its windows may carry
//! (`App::window_classes`), which is how its windows are found elsewhere too.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;

use super::Entry;
use crate::apps::App;

/// Window class (lowercase) → the installed application whose windows carry it.
pub struct Lookup<'a>(HashMap<String, &'a App>);

impl<'a> Lookup<'a> {
    pub fn new(apps: &'a [App]) -> Self {
        let classes: Vec<Vec<String>> = apps.iter().map(App::window_classes).collect();
        let mut found = HashMap::new();
        // An application's names go from what it declares to what is guessed
        // from its program: where two applications share a name, the one
        // that is surer of it has it.
        for rank in 0..classes.iter().map(Vec::len).max().unwrap_or(0) {
            for (app, classes) in apps.iter().zip(&classes) {
                if let Some(class) = classes.get(rank) {
                    found.entry(class.clone()).or_insert(app);
                }
            }
        }
        Lookup(found)
    }

    pub fn app(&self, class: &str) -> Option<&'a App> {
        self.0.get(&class.to_lowercase()).copied()
    }
}

/// Whether copies made in a window of class `source` are not to be recorded.
/// `listed` is the setting: ids of installed applications, or window classes
/// (of applications nothing installed describes).
pub fn ignored(source: &str, listed: &[String], apps: &[App]) -> bool {
    let source = source.to_lowercase();
    listed.iter().any(|name| match apps.iter().find(|app| app.id == *name) {
        Some(app) => app.window_classes().contains(&source),
        None => name.to_lowercase() == source,
    })
}

/// An application that can be named as one not to record from.
#[derive(Debug, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ClipboardSource")]
pub struct Source {
    /// What the setting holds: the application's id, or the window class.
    pub id: String,
    pub name: String,
    pub icon: Option<PathBuf>,
    /// There are copies from it in the history.
    pub seen: bool,
}

/// The installed applications that have windows of their own, and the
/// window classes in the history (`seen`) that none of them accounts for;
/// by name.
pub fn choices(apps: &[App], seen: &[String]) -> Vec<Source> {
    let seen: Vec<String> = seen.iter().map(|class| class.to_lowercase()).collect();
    let lookup = Lookup::new(apps);
    let mut all: Vec<Source> = apps
        .iter()
        .filter_map(|app| {
            let classes = app.window_classes();
            (!classes.is_empty()).then(|| Source {
                id: app.id.clone(),
                name: app.name.clone(),
                icon: app.icon.clone(),
                // Only if those copies would be shown as this application's.
                seen: seen.iter().any(|class| lookup.app(class).is_some_and(|found| found.id == app.id)),
            })
        })
        .collect();
    for class in &seen {
        if lookup.app(class).is_none() && !all.iter().any(|source| source.id == *class) {
            all.push(Source { id: class.clone(), name: class.clone(), icon: None, seen: true });
        }
    }
    all.sort_by_key(|source| source.name.to_lowercase());
    all
}

/// An entry and the application it was copied in.
pub struct Shown {
    pub entry: Entry,
    /// The application's name; the window class when nothing installed has it.
    pub app_name: Option<String>,
    pub app_icon: Option<PathBuf>,
    /// It is here for what it means: the words of the query are not in it.
    pub by_meaning: bool,
}

/// The entries a search found, each with the application it came from.
/// `by_meaning` names the ones that are there for what they mean alone.
pub fn shown(entries: Vec<Entry>, by_meaning: &[u64], apps: &[App]) -> Vec<Shown> {
    // Built once for the request, and only if some entry has a source.
    let lookup = entries.iter().any(|entry| entry.app.is_some()).then(|| Lookup::new(apps));
    entries
        .into_iter()
        .map(|entry| {
            let app = entry.app.as_deref().and_then(|class| lookup.as_ref()?.app(class));
            Shown {
                app_name: app.map(|app| app.name.clone()).or_else(|| entry.app.clone()),
                app_icon: app.and_then(|app| app.icon.clone()),
                by_meaning: by_meaning.contains(&entry.id),
                entry,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, exec: &str, class: Option<&str>) -> App {
        App {
            id: id.into(),
            name: name.into(),
            generic_name: None,
            comment: None,
            keywords: vec![],
            exec: exec.into(),
            icon: Some(format!("/usr/share/icons/{name}.png").into()),
            terminal: false,
            working_dir: None,
            startup_wm_class: class.map(str::to_owned),
            path: format!("/usr/share/applications/{id}").into(),
        }
    }

    fn installed() -> Vec<App> {
        vec![
            app("org.keepassxc.KeePassXC.desktop", "KeePassXC", "keepassxc %f", None),
            app("firefox.desktop", "Firefox", "firefox %u", Some("firefox")),
            app("code.desktop", "Visual Studio Code", "/usr/share/code/code %F", Some("Code")),
            // Guesses "code" from its file name only; the editor declares it.
            app("com.example.code.desktop", "Code Counter", "counter", None),
            App { terminal: true, ..app("htop.desktop", "Htop", "htop", None) },
        ]
    }

    fn listed(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn an_application_named_by_id_is_ignored_under_any_class_its_windows_carry() {
        let apps = installed();
        let list = listed(&["org.keepassxc.KeePassXC.desktop", "code.desktop"]);
        // The desktop file's name, its last part, the program; in any case.
        assert!(ignored("org.keepassxc.KeePassXC", &list, &apps));
        assert!(ignored("KeePassXC", &list, &apps));
        assert!(ignored("keepassxc", &list, &apps));
        assert!(ignored("Code", &list, &apps));
        assert!(!ignored("firefox", &list, &apps));
        // A whole name, not part of one.
        assert!(!ignored("keepass", &list, &apps));
        assert!(!ignored("code-insiders", &list, &apps));
        assert!(!ignored("firefox", &[], &apps));
    }

    #[test]
    fn a_window_class_can_be_named_when_nothing_installed_has_it() {
        let apps = installed();
        let list = listed(&["Wine", "gone.desktop"]);
        assert!(ignored("wine", &list, &apps));
        assert!(ignored("Wine", &list, &apps));
        assert!(!ignored("winecfg", &list, &apps));
        // An application that was uninstalled: its id says nothing of its windows.
        assert!(!ignored("gone", &list, &apps));
    }

    #[test]
    fn a_terminal_program_has_no_windows_to_ignore() {
        let apps = installed();
        assert!(!ignored("htop", &listed(&["htop.desktop"]), &apps));
        assert!(choices(&apps, &[]).iter().all(|source| source.id != "htop.desktop"));
    }

    #[test]
    fn a_class_belongs_to_the_application_surest_of_it() {
        let apps = installed();
        let lookup = Lookup::new(&apps);
        assert_eq!(lookup.app("Code").unwrap().name, "Visual Studio Code");
        assert_eq!(lookup.app("FIREFOX").unwrap().name, "Firefox");
        assert_eq!(lookup.app("keepassxc").unwrap().name, "KeePassXC");
        assert!(lookup.app("wine").is_none());
    }

    #[test]
    fn the_choices_are_what_is_installed_and_what_was_copied_from() {
        let apps = installed();
        let all = choices(&apps, &listed(&["Firefox", "wine", "Wine"]));
        let names: Vec<(&str, bool)> = all.iter().map(|source| (source.name.as_str(), source.seen)).collect();
        assert_eq!(
            names,
            [("Code Counter", false), ("Firefox", true), ("KeePassXC", false), ("Visual Studio Code", false), ("wine", true)]
        );
        assert_eq!(all[1].id, "firefox.desktop");
        assert_eq!(all[4], Source { id: "wine".into(), name: "wine".into(), icon: None, seen: true });
    }

    #[test]
    fn an_entry_is_shown_with_the_application_it_came_from() {
        let entry = |id, app: Option<&str>| Entry {
            id,
            kind: super::super::Kind::Text,
            text: Some("x".into()),
            image: None,
            width: None,
            height: None,
            app: app.map(str::to_owned),
            copied_at: 1,
            first_copied_at: 1,
            times_copied: 1,
            pinned: false,
            hash: id,
        };
        let entries = shown(vec![entry(1, Some("Code")), entry(2, Some("wine")), entry(3, None)], &[2], &installed());
        assert_eq!(entries[0].app_name.as_deref(), Some("Visual Studio Code"));
        assert_eq!(entries[0].app_icon.as_deref(), Some(std::path::Path::new("/usr/share/icons/Visual Studio Code.png")));
        // The class it was recorded under is still there, as searches use it.
        assert_eq!(entries[0].entry.app.as_deref(), Some("Code"));
        assert_eq!(entries[0].entry.id, 1);
        assert_eq!((entries[1].app_name.as_deref(), entries[1].app_icon.as_ref()), (Some("wine"), None));
        assert_eq!(entries[2].app_name, None);
        // Which of them are here for what they mean alone.
        assert_eq!(entries.iter().map(|shown| shown.by_meaning).collect::<Vec<_>>(), [false, true, false]);
        assert!(shown(Vec::new(), &[], &installed()).is_empty());
    }
}
