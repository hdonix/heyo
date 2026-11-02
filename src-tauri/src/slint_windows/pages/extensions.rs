//! The extensions' pages: each with its preferences, its commands, what it
//! may ask Heyo for, and its removal. The applications are listed as an
//! extension of their own, and Heyo's own commands as another, with the
//! clipboard history and snippet expansion.

use serde_json::Value;

use super::{enable, listed, on, reload, Header, Known, Page, Reaction, Sheet, To};
use crate::extensions::{CommandInfo, ExtensionInfo, Preference, PreferenceValues};
use crate::i18n::{self, marked, t, tf, tn, translate};
use crate::commands::settings::HotkeyName;
use crate::permissions::Permission;
use crate::platform::Support;
use crate::slint_windows::answer::Answer;
use crate::slint_windows::host::Command;
use crate::slint_windows::ids::{Act, Bound, Id, PageId, Setting};
use crate::slint_windows::model::{ask, Ask, Entry, Line, Then};

/// The id the applications have among the extensions of the sidebar.
pub(crate) const APPLICATIONS: &str = "@applications";
/// Heyo's own commands.
pub(crate) const BUILTIN: &str = "@heyo";
/// The command whose hotkey is one of Heyo's own, set on its own row.
pub(crate) const CLIPBOARD_COMMAND: &str = "cmd:@heyo:clipboard-history";

const RETENTION: &[(&str, &str)] = &[("1", marked("1 Day")), ("7", marked("7 Days")), ("30", marked("1 Month")), ("90", marked("3 Months")), ("180", marked("6 Months")), ("365", marked("1 Year"))];

/// The applications as the extension the sidebar lists them as.
pub(crate) fn applications_extension(known: &Known) -> ExtensionInfo {
    let commands = known.applications.iter().map(|item| CommandInfo { id: item.id.clone(), name: item.id.clone(), title: item.title.clone(), description: item.subtitle.clone(), mode: String::new(), icon: item.icon.clone(), preferences: Vec::new() }).collect();
    ExtensionInfo {
        id: APPLICATIONS.into(),
        name: APPLICATIONS.into(),
        title: t("Applications").into(),
        description: Some(t("Give an application a hotkey to launch it from anywhere.").into()),
        author: None,
        icon: Some("app-window-grid-3x3-16".into()),
        builtin: true,
        development: None,
        commands,
        preferences: Vec::new(),
        values: PreferenceValues::default(),
    }
}

pub(crate) fn extension_title(extension: &ExtensionInfo) -> String {
    if extension.id == BUILTIN { t("Heyo Commands").into() } else { extension.title.clone() }
}

/// What to show for an icon the host named: a file as it is, a file of
/// the extension's own, or a symbol by its name.
pub(crate) fn icon(extension: &ExtensionInfo, icon: Option<&str>) -> Option<String> {
    let icon = icon.filter(|icon| !icon.is_empty())?;
    if std::path::Path::new(icon).is_absolute() || !icon.contains('.') {
        return Some(icon.to_string());
    }
    Some(format!("{}/assets/{icon}", extension.id))
}

/// The extension `at` is the page of.
fn extension(at: &PageId, known: &Known) -> Option<ExtensionInfo> {
    let PageId::Extension(id) = at else { return None };
    if id == APPLICATIONS {
        return Some(applications_extension(known));
    }
    known.extensions.iter().find(|extension| extension.id == *id).cloned()
}

/// One preference of an extension or of one of its commands, as a row.
fn preference(extension: &ExtensionInfo, command: &str, preference: &Preference, stored: Option<&Value>) -> Line {
    let (name, kind) = (&preference.name, preference.kind.as_str());
    let value = stored.filter(|stored| !stored.is_null()).or(preference.default.as_ref()).unwrap_or(&Value::Null);
    let (title, label) = (preference.title.as_deref(), preference.label.as_deref());
    let row = Line::row(Id::Preference { extension: extension.name.clone(), command: command.to_string(), name: name.clone(), kind: kind.to_string() }, format!("{}{}", title.or(label).unwrap_or(name), if preference.required { " *" } else { "" }));
    // A checkbox's label says what ticking it does, where it has a title too.
    let row = row.says(if kind == "checkbox" && label.is_some() && title.is_some() { label.unwrap_or_default().to_string() } else { preference.description.clone().unwrap_or_default() });
    match kind {
        "checkbox" => row.switch(*value == true),
        "dropdown" => {
            let choices: Vec<(&str, &str)> = preference.data.iter().map(|choice| (choice.value.as_str(), choice.title.as_str())).collect();
            let chosen = value.as_str().map(str::to_string).or_else(|| choices.first().map(|(value, _)| value.to_string())).unwrap_or_default();
            row.select(&chosen, choices)
        }
        _ => {
            let shown = match value {
                Value::Null => String::new(),
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            let placeholder = preference.placeholder.as_deref().unwrap_or(if kind == "directory" || kind == "file" { "/path" } else { "" });
            let mut row = row.field(shown, placeholder);
            if kind == "password" {
                row.control = "password";
            }
            row
        }
    }
}

/// An application as a row of a card: one that is left out of
/// something, with the button that takes it off the list.
fn left_out(known: &Known, id: &str, remove: fn(String) -> Id, missing: &str) -> Line {
    let source = known.sources.iter().find(|source| source.id == id);
    Line::row(remove(id.to_string()), source.map_or(id.to_string(), |source| source.name.clone()))
        .says(if source.is_some() { "" } else { missing })
        .pictured(Some(source.and_then(|source| source.icon.as_ref()).map_or("app-window-16".to_string(), |icon| icon.to_string_lossy().into_owned())))
        .button(t("Remove"))
}

/// The applications that can be added to a list of them, of those not on it: by id and name.
fn offered<'a>(known: &'a Known, on: &[String], seen: Option<bool>) -> Vec<(&'a str, &'a str)> {
    known.sources.iter().filter(|source| seen.is_none_or(|seen| source.seen == seen) && !on.contains(&source.id)).map(|source| (source.id.as_str(), source.name.as_str())).collect()
}

/// The cards about the clipboard history, on the page of Heyo's commands.
fn clipboard(known: &Known, page: &mut Sheet) {
    let settings = &known.settings.settings;
    let recording = settings.clipboard_history;
    let mut rows = Vec::new();
    if known.hotkeys_supported() {
        rows.push(Line::row("", t("Hotkey")).says(t("Opens Clipboard History from anywhere")).hotkey(&Bound::Own(HotkeyName::Clipboard).to_string(), known.hotkeys.clipboard.as_deref()));
    }
    rows.push(Line::row(Id::Set(Setting::ClipboardHistory), t("Record Clipboard History")).says(t("Copies marked as secret by password managers are never recorded")).switch(recording));
    if recording {
        rows.push(Line::row(Id::Set(Setting::ClipboardPaused), t("Pause Recording")).says(t("Nothing you copy is recorded while this is on. “Pause or Resume Clipboard History” in the root search does the same, and can have a hotkey.")).switch(settings.clipboard_paused));
    }
    rows.push(Line::row(Id::Set(Setting::ClipboardRetentionDays), t("Keep History For")).says(t("Pinned entries are kept forever")).select(&settings.clipboard_retention_days.to_string(), RETENTION.iter().map(|(value, title)| (*value, translate(title)))));
    rows.push(Line::row(Id::Act(Act::ClearClipboard), t("Clear History")).says(t("Pinned entries are not deleted")).confirm(t("Clear…"), t("Click Again to Clear")));
    if recording {
        // Finding entries by what they mean: the switch, the download
        // it needs, and how far reading the history has got.
        let meaning = known.meaning.as_ref();
        let (installed, on) = (meaning.is_some_and(|meaning| meaning.installed), settings.clipboard_meaning);
        let (waiting, indexed) = meaning.map_or((0, 0), |meaning| (meaning.waiting as u64, meaning.indexed as u64));
        let state = if !installed {
            t("A one-time download of about 135 MB, or 250 MB if text recognition is not installed. It takes about 350 MB of memory while you search, and none a few minutes after.").to_string()
        } else if !on {
            String::new()
        } else if let Some(problem) = meaning.and_then(|meaning| meaning.problem.as_ref()) {
            tf("It is not working at present: {0}", &[problem])
        } else if waiting > 0 {
            tn("Reading the history: {0} entry to go; until then some are found by their words only.", "Reading the history: {0} entries to go; until then some are found by their words only.", waiting, &[&i18n::number(waiting)])
        } else {
            tn("{0} entry can be found this way.", "{0} entries can be found this way.", indexed, &[&i18n::number(indexed)])
        };
        let row = Line::row(Id::Set(Setting::ClipboardMeaning), t("Find by Meaning")).says(format!("{} {state}", t("“服务器登录命令” or “server login command” finds an ssh line: entries are found by what they are about as well as by their words, in Chinese and English alike. A language model on this computer reads what you copy; nothing is sent anywhere.")));
        rows.push(if meaning.is_none() || installed {
            row.switch(on)
        } else if known.meaning_installing || meaning.is_some_and(|meaning| meaning.installing) {
            row.text("Downloading…")
        } else {
            Line { id: Id::Act(Act::InstallMeaning).into(), ..row }.button(t("Download"))
        });
    }
    page.card(t("Clipboard History"), rows);
    if recording {
        let ignored = &settings.clipboard_ignored_apps;
        let mut rows: Vec<Line> = ignored.iter().map(|id| left_out(known, id, Id::IgnoreRemove, t("Not installed"))).collect();
        // Those copied from first, then the rest of the installed.
        rows.push(Line::row(Id::IgnoreAdd, t("Never Record Copies From")).says(t("What you copy while a window of one of these applications is in front stays out of the history. Password managers are always left out.")).select("", std::iter::once(("", t("Add Application…"))).chain(offered(known, ignored, Some(true))).chain(offered(known, ignored, Some(false)))));
        page.card(t("Ignored Applications"), rows);
    }
}

/// Snippets that expand as their keyword is typed: the switch, and the
/// applications it leaves alone.
fn snippet_expansion(known: &Known, page: &mut Sheet) {
    let Some(capabilities) = &known.capabilities else { return };
    let available = capabilities.snippet_expansion != Support::None;
    let on = known.settings.settings.snippet_expansion;
    let row = Line::row(Id::Set(Setting::SnippetExpansion), t("Expand Snippets as You Type"));
    let mut rows = vec![if available {
        row.says(t("Typing a snippet's keyword in any application replaces it with the snippet's text. While this is on, Heyo watches what you type to find the keywords; none of it is stored. “Pause or Resume Snippet Expansion” in the root search switches it too.")).switch(on)
    } else if capabilities.session == "wayland" {
        row.says(t("Not available on Wayland, where an application cannot see what is typed in another. Snippets can still be pasted from the root search."))
    } else {
        row.says(t("Not available in this desktop session. Snippets can still be pasted from the root search."))
    }];
    if available && on {
        let excluded = &known.settings.settings.snippet_expansion_ignored_apps;
        rows.extend(excluded.iter().map(|id| left_out(known, id, Id::ExpandRemove, "")));
        rows.push(Line::row(Id::ExpandAdd, t("Never Expand In")).says(t("Password managers are always left out. A password field in any other application cannot be told from an ordinary one. Nothing expands while an input method (Chinese, Japanese, …) is in use.")).select("", std::iter::once(("", t("Add Application…"))).chain(offered(known, excluded, None))));
    }
    page.card(t("Snippet Expansion"), rows);
}

/// An extension's page.
fn page_of(known: &Known, extension: &ExtensionInfo) -> Vec<Line> {
    let id = &extension.id;
    let (applications, builtin) = (id == APPLICATIONS, extension.builtin);
    let disabled = &known.settings.settings.disabled_commands;
    let mut page = Sheet::default();
    let about = match (&extension.development, &extension.author) {
        (Some(directory), _) => tf("In development from {0}", &[directory]),
        (None, Some(author)) => tf("by {0}", &[author]),
        (None, None) if builtin && !applications => t("Built into Heyo").into(),
        _ => String::new(),
    };
    page.0.push(Line { kind: "head", label: extension_title(extension), description: about, icon: icon(extension, extension.icon.as_deref()), ..Line::default() });
    if let Some(description) = extension.description.as_deref().filter(|description| !description.is_empty()) {
        page.note(description);
    }
    page.card(t("Preferences"), extension.preferences.iter().map(|one| preference(extension, "", one, extension.values.extension.get(&one.name))).collect());

    let query = if applications { known.find.to_lowercase() } else { String::new() };
    let mut commands: Vec<Line> = extension
        .commands
        .iter()
        .filter(|command| query.is_empty() || command.title.to_lowercase().contains(&query))
        .map(|command| {
            let row = Line::row(Id::Enable(command.id.clone()), command.title.clone()).says(command.description.clone().unwrap_or_default()).pictured(icon(extension, command.icon.as_deref()));
            let row = if command.id == CLIPBOARD_COMMAND { row } else { row.hotkey(&command.id, known.item_hotkeys.get(&command.id).map(String::as_str)) };
            if applications { row } else { row.switch(!disabled.contains(&command.id)) }
        })
        .collect();
    if commands.is_empty() {
        commands.push(Line::row("", t("No matches")));
    }
    page.card(if applications { "" } else { t("Commands") }, commands);

    if id == BUILTIN {
        clipboard(known, &mut page);
        snippet_expansion(known, &mut page);
    }
    for command in &extension.commands {
        let stored = extension.values.commands.get(&command.name);
        page.card(&tf("{} Preferences", &[&command.title]), command.preferences.iter().map(|one| preference(extension, &command.name, one, stored.and_then(|values| values.get(&one.name)))).collect());
    }
    if !builtin && known.permissions.0 == *id && !known.permissions.1.is_empty() {
        // What it may ask Heyo for: what its manifest lists and what
        // the user has allowed, which can be taken back here. Live
        // activities are not asked about; they have a switch instead.
        let name = &extension.name;
        let activities = known.settings.settings.live_activities;
        let rows = known.permissions.1.iter().map(|row| {
            let mut letters = row.allows.chars();
            let label = letters.next().map(|first| first.to_uppercase().chain(letters).collect::<String>()).unwrap_or_default();
            if row.permission == Permission::Activity {
                return Line::row(Id::Activities(name.to_string()), label).says(if activities { t("Declared; turn it off here") } else { t("Live activities are off in General") }).switch(activities && !known.settings.settings.activities_off.contains(name));
            }
            let line = Line::row(Id::Revoke { extension: name.to_string(), permission: row.permission }, label).says(match row.state.as_ref() {
                "allowed" => t("Allowed by you"),
                "asks" => t("Asks you the first time it is used"),
                _ => t("Needs no asking"),
            });
            if row.state == "allowed" { line.button(t("Revoke")) } else { line }
        });
        page.card(t("Permissions"), rows.collect());
        page.note(t("Permissions are about what Heyo hands over or does for an extension. They do not confine it: an extension is a program on this computer, and can read your files and start other programs by itself."));
    }
    if !builtin {
        let developed = extension.development.is_some();
        page.card("", vec![Line::row(Id::Uninstall(id.to_string()), if developed { t("Stop Developing") } else { t("Uninstall Extension") }).says(if developed { t("Takes it out of Heyo; its sources are not touched") } else { t("Its preferences are kept") }).confirm(if developed { t("Remove") } else { t("Uninstall") }, if developed { t("Click Again to Remove") } else { t("Click Again to Uninstall") })]);
    }
    page.done()
}

pub(crate) struct Extensions;

impl Page for Extensions {
    /// Applications first, then Heyo's own commands, then the installed
    /// extensions by name. An extension is found by its commands too, the
    /// applications by their name alone.
    fn entries(&self, known: &Known) -> Vec<Entry> {
        let rank = |extension: &ExtensionInfo| match extension.id.as_str() {
            APPLICATIONS => 0,
            BUILTIN => 1,
            _ => 2,
        };
        let mut extensions: Vec<ExtensionInfo> = known.extensions.clone();
        extensions.push(applications_extension(known));
        extensions.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
        extensions
            .iter()
            .map(|extension| {
                let mut found_by = vec![extension.title.clone()];
                if extension.id != APPLICATIONS {
                    found_by.extend(extension.commands.iter().map(|command| command.title.clone()));
                }
                Entry { id: extension.id.clone(), title: extension_title(extension), icon: icon(extension, extension.icon.as_deref()), extension: true, found_by }
            })
            .collect()
    }

    fn shows(&self, at: &PageId) -> bool {
        matches!(at, PageId::Extension(_))
    }

    fn lines(&self, at: &PageId, known: &Known) -> Vec<Line> {
        extension(at, known).map(|extension| page_of(known, &extension)).unwrap_or_default()
    }

    fn acted(&mut self, at: &PageId, known: &mut Known, id: &Id, value: &str) -> Option<Reaction> {
        let then = Then::Acted(id.clone());
        let settings = &known.settings.settings;
        let asks = match id {
            Id::Enable(command) => enable(known, command, on(value)),
            Id::Set(Setting::ClipboardHistory) => known.change(|view| view.settings.clipboard_history = on(value)),
            Id::Set(Setting::ClipboardPaused) => known.change(|view| view.settings.clipboard_paused = on(value)),
            Id::Set(Setting::ClipboardMeaning) => known.change(|view| view.settings.clipboard_meaning = on(value)),
            Id::Set(Setting::SnippetExpansion) => known.change(|view| view.settings.snippet_expansion = on(value)),
            Id::Set(Setting::ClipboardRetentionDays) => match value.parse::<u32>() {
                Ok(days) => known.change(|view| view.settings.clipboard_retention_days = days),
                Err(_) => Vec::new(),
            },
            Id::IgnoreAdd | Id::ExpandAdd if value.is_empty() => Vec::new(),
            Id::IgnoreAdd => {
                let apps = listed(&settings.clipboard_ignored_apps, value, true);
                known.change(|view| view.settings.clipboard_ignored_apps = apps)
            }
            Id::ExpandAdd => {
                let apps = listed(&settings.snippet_expansion_ignored_apps, value, true);
                known.change(|view| view.settings.snippet_expansion_ignored_apps = apps)
            }
            Id::IgnoreRemove(application) => {
                let apps = listed(&settings.clipboard_ignored_apps, application, false);
                known.change(|view| view.settings.clipboard_ignored_apps = apps)
            }
            Id::ExpandRemove(application) => {
                let apps = listed(&settings.snippet_expansion_ignored_apps, application, false);
                known.change(|view| view.settings.snippet_expansion_ignored_apps = apps)
            }
            Id::Activities(extension) => {
                let off = listed(&settings.activities_off, extension, !on(value));
                known.change(|view| view.settings.activities_off = off)
            }
            Id::Preference { extension, command, name, kind } => {
                let Some(found) = known.extensions.iter().find(|other| other.name == *extension) else { return Some(Reaction::default()) };
                let mut values = found.values.clone();
                let value = if kind == "checkbox" { Value::Bool(on(value)) } else { Value::String(value.to_string()) };
                if command.is_empty() {
                    values.extension.insert(name.clone(), value);
                } else {
                    values.commands.entry(command.clone()).or_default().insert(name.clone(), value);
                }
                vec![ask(Command::SetExtensionPreferences { extension: extension.clone(), values }, then)]
            }
            Id::Revoke { extension, permission } => {
                let page = at.to_string();
                vec![ask(Command::RevokeExtensionPermission { extension: extension.clone(), permission: *permission }, Then::Permissions(page.clone())), ask(Command::ExtensionPermissions(page.clone()), Then::Permissions(page))]
            }
            Id::Uninstall(extension) => vec![ask(Command::UninstallExtension(extension.clone()), then)],
            Id::Act(Act::ClearClipboard) => vec![ask(Command::ClearClipboardHistory, Then::Known)],
            Id::Act(Act::InstallMeaning) => {
                known.meaning_installing = true;
                // A download was started; an answer only if it could not be.
                vec![ask(Command::ClipboardMeaningInstall, then)]
            }
            _ => return None,
        };
        Some(Reaction::asking(asks))
    }

    fn answered(&mut self, known: &mut Known, id: &Id, answer: &Result<Answer, String>) -> Reaction {
        match (id, answer) {
            (Id::Act(Act::InstallMeaning), Err(_)) => known.meaning_installing = false,
            // A preference that was saved: the extensions are read again.
            (Id::Preference { .. }, Ok(_)) => return Reaction::asking(reload()),
            // Removed: the window is elsewhere.
            (Id::Uninstall(_), Ok(_)) => return Reaction { asks: reload(), to: Some(To::Move(PageId::own(super::general::PAGE))), said: false },
            _ => {}
        }
        Reaction::default()
    }

    fn entered(&self, at: &PageId, known: &Known) -> Vec<Ask> {
        match at {
            PageId::Extension(id) if id == BUILTIN => vec![ask(Command::ClipboardSources, Then::Known), ask(Command::ClipboardMeaningStatus, Then::Known)],
            PageId::Extension(id) if known.extensions.iter().any(|extension| extension.id == *id && !extension.builtin) => vec![ask(Command::ExtensionPermissions(id.clone()), Then::Permissions(id.clone()))],
            _ => Vec::new(),
        }
    }

    fn header(&self, at: &PageId) -> Header {
        Header { placeholder: if *at == PageId::Extension(APPLICATIONS.into()) { t("Search applications…") } else { "" }, filter: None }
    }

    /// Asked again every two seconds while the history is being read for
    /// finding by meaning, so that the page says how much is left.
    fn poll(&self, at: &PageId, known: &Known) -> Vec<Ask> {
        let reading = *at == PageId::Extension(BUILTIN.into()) && known.settings.settings.clipboard_meaning && known.meaning.as_ref().is_some_and(|meaning| meaning.installed && meaning.waiting > 0);
        if reading { vec![ask(Command::ClipboardMeaningStatus, Then::Known)] } else { Vec::new() }
    }
}
