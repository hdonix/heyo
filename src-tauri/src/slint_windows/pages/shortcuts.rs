//! Shortcuts: the applications and the extensions' commands in a table,
//! each with its alias, its hotkey and (a command) whether it is on; the
//! table narrowed by a search and a menu.

use std::collections::{BTreeSet, BTreeMap};

use super::extensions::{applications_extension, extension_title, icon, CLIPBOARD_COMMAND};
use super::{enable, tile, Header, Known, Page, Reaction, Sheet};
use crate::extensions::{CommandInfo, ExtensionInfo};
use crate::i18n::{marked, t, translate};
use crate::slint_windows::ids::{Id, PageId};
use crate::slint_windows::keys;
use crate::slint_windows::model::{Entry, Line};

pub(crate) const PAGE: &str = "shortcuts";

/// What the table can be narrowed to: id, title.
const MODIFICATIONS: &[(&str, &str)] = &[("hotkey", marked("Hotkey Set")), ("alias", marked("Alias Set")), ("enabled", marked("Enabled")), ("disabled", marked("Disabled"))];
const CATEGORIES: &[(&str, &str)] = &[("all", marked("All categories")), ("applications", marked("Applications")), ("extensions", marked("Extensions"))];

/// What the table is narrowed to, and what of it is open.
#[derive(Debug)]
pub(crate) struct Shortcuts {
    pub modifications: BTreeSet<String>,
    pub category: String,
    pub expanded: BTreeSet<String>,
}

impl Default for Shortcuts {
    fn default() -> Shortcuts {
        Shortcuts { modifications: BTreeSet::new(), category: "all".into(), expanded: BTreeSet::from(["extensions".to_string()]) }
    }
}

impl Page for Shortcuts {
    fn entries(&self, _: &Known) -> Vec<Entry> {
        vec![tile(PAGE, marked("Shortcuts"), "keyboard-16")]
    }

    fn shows(&self, at: &PageId) -> bool {
        *at == PageId::own(PAGE)
    }

    /// The table: the applications and the extensions with their commands,
    /// each with an alias, a hotkey and (for a command) whether it is on.
    fn lines(&self, _: &PageId, known: &Known) -> Vec<Line> {
        let settings = &known.settings.settings;
        let query = known.find.to_lowercase();
        let filtering = !query.is_empty() || !self.modifications.is_empty();
        let disabled: BTreeSet<&str> = settings.disabled_commands.iter().map(String::as_str).collect();
        let aliases: &BTreeMap<String, String> = &settings.aliases;
        let open = |id: &str| filtering || self.expanded.contains(id);
        let visible = |command: &CommandInfo, group: &str, application: bool| {
            let id = &command.id;
            if !query.is_empty() && !command.title.to_lowercase().contains(&query) && !group.to_lowercase().contains(&query) {
                return false;
            }
            if self.modifications.contains("hotkey") && !known.item_hotkeys.contains_key(id) {
                return false;
            }
            if self.modifications.contains("alias") && !aliases.contains_key(id) {
                return false;
            }
            let (enabled, off) = (self.modifications.contains("enabled"), self.modifications.contains("disabled"));
            // Applications are always enabled.
            enabled == off || (application || !disabled.contains(id.as_str())) == enabled
        };
        let entry = |extension: &ExtensionInfo, command: &CommandInfo, indent: bool, application: bool| {
            let id = command.id.clone();
            Line {
                kind: "entry",
                label: command.title.clone(),
                icon: icon(extension, command.icon.as_deref()),
                alias: aliases.get(&id).cloned().unwrap_or_default(),
                alias_id: Id::Alias(id.clone()).into(),
                check_id: Id::Enable(id.clone()).into(),
                hotkey: if id == CLIPBOARD_COMMAND { String::new() } else { id.clone() },
                keys: known.item_hotkeys.get(&id).map(|accelerator| keys::parts(accelerator)).unwrap_or_default(),
                checkable: !application,
                on: !disabled.contains(id.as_str()),
                indent,
                id,
                ..Line::default()
            }
        };
        let group = |id: &str, label: &str, icon: &str| Line { kind: "group", id: Id::Open(id.to_string()).into(), label: label.to_string(), icon: Some(icon.to_string()), open: open(id), ..Line::default() };
        let mut page = Sheet::default();

        let applications = applications_extension(known);
        let apps: Vec<&CommandInfo> = applications.commands.iter().filter(|command| visible(command, t("Applications"), true)).collect();
        if self.category != "extensions" && (!apps.is_empty() || !filtering) {
            let mut rows = vec![group("applications", t("Applications"), "app-window-grid-3x3-16")];
            if open("applications") {
                rows.push(Line { kind: "columns", ..Line::default() });
                rows.extend(apps.iter().map(|command| entry(&applications, command, false, true)));
            }
            page.card("", rows);
        }

        let groups: Vec<(&ExtensionInfo, Vec<&CommandInfo>)> = known.extensions.iter().map(|extension| (extension, extension.commands.iter().filter(|command| visible(command, &extension.title, false)).collect::<Vec<_>>())).filter(|(_, commands)| !commands.is_empty()).collect();
        if self.category != "applications" && (!groups.is_empty() || !filtering) {
            let mut rows = vec![group("extensions", t("Extensions"), "plug-16")];
            if open("extensions") {
                rows.push(Line { kind: "columns", ..Line::default() });
                for (extension, commands) in &groups {
                    let all = &extension.commands;
                    // An extension with one command is shown as that command.
                    if all.len() == 1 {
                        rows.push(entry(extension, commands[0], false, false));
                        continue;
                    }
                    let id = &extension.id;
                    let enabled = all.iter().filter(|command| !disabled.contains(command.id.as_str())).count();
                    rows.push(Line { kind: "group", id: Id::Open(id.to_string()).into(), label: extension_title(extension), icon: icon(extension, extension.icon.as_deref()), indent: true, open: open(id), checkable: true, check_id: Id::EnableAll(id.to_string()).into(), on: enabled > 0, mixed: enabled > 0 && enabled < all.len(), ..Line::default() });
                    if open(id) {
                        rows.extend(commands.iter().map(|command| entry(extension, command, true, false)));
                    }
                }
            }
            page.card("", rows);
        }
        if filtering && apps.is_empty() && groups.is_empty() {
            page.note(t("Nothing matches."));
        }
        page.done()
    }

    fn acted(&mut self, _: &PageId, known: &mut Known, id: &Id, value: &str) -> Option<Reaction> {
        let on = super::on(value);
        let asks = match id {
            Id::Alias(item) => {
                let alias = value.trim();
                if known.settings.settings.aliases.get(item).map_or("", String::as_str) == alias {
                    return Some(Reaction::default());
                }
                known.change(|view| {
                    if alias.is_empty() {
                        view.settings.aliases.remove(item);
                    } else {
                        view.settings.aliases.insert(item.clone(), alias.to_string());
                    }
                })
            }
            Id::Enable(command) => enable(known, command, on),
            Id::EnableAll(extension) => {
                let Some(extension) = known.extensions.iter().find(|other| other.id == *extension) else { return Some(Reaction::default()) };
                let ids: Vec<String> = extension.commands.iter().map(|command| command.id.clone()).collect();
                known.change(|view| {
                    let disabled = &mut view.settings.disabled_commands;
                    disabled.retain(|other| !ids.contains(other));
                    if !on {
                        disabled.extend(ids);
                    }
                })
            }
            Id::Open(group) => {
                if !self.expanded.remove(group) {
                    self.expanded.insert(group.clone());
                }
                Vec::new()
            }
            // The menu's entries.
            Id::Modification(modification) => {
                if !self.modifications.remove(modification) {
                    self.modifications.insert(modification.clone());
                }
                Vec::new()
            }
            Id::Category(category) => {
                self.category = category.clone();
                Vec::new()
            }
            _ => return None,
        };
        Some(Reaction::asking(asks))
    }

    fn header(&self, _: &PageId) -> Header {
        let title = CATEGORIES.iter().find(|(id, _)| *id == self.category).map_or(t("All categories"), |(_, title)| translate(title));
        let mut picks = vec![(String::new(), t("Modifications").to_string(), true, false)];
        picks.extend(MODIFICATIONS.iter().map(|(id, title)| (Id::Modification(id.to_string()).into(), translate(title).to_string(), false, self.modifications.contains(*id))));
        picks.push((String::new(), t("Categories").to_string(), true, false));
        picks.extend(CATEGORIES.iter().map(|(id, title)| (Id::Category(id.to_string()).into(), translate(title).to_string(), false, self.category == *id)));
        Header { placeholder: t("Search…"), filter: Some((title.to_string(), self.modifications.len(), picks)) }
    }
}
