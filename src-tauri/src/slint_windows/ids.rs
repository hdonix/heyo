//! What a control of the settings window acts on, which page is shown,
//! and how each is written where it has to be a word.
//!
//! The window is told lines and hands back which control was used: Slint
//! has that as a string, the line's `id`. What the string says, and how,
//! is this file's alone. The model makes an [`Id`] for each control and
//! is handed one back (`Model::acted`); the markup hands back what it was
//! given and composes nothing, so a line that has more than one control
//! carries an id for each (`Line::more_id`, `check_id`, `alias_id`). A
//! page is named the same way ([`PageId`]): the sidebar hands back the id
//! of the entry chosen, and a link the page it goes to.
//! Every form is tried there and back in the tests below.

use std::fmt;

use crate::commands::settings::HotkeyName;
use crate::models::registry::Use;
use crate::permissions::Permission;

/// Between the parts of an id that names several things.
const SEP: char = '\u{1f}';

/// Something Heyo does when a button is pressed, with nothing to say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    InstallShellExtension,
    InstallOcr,
    InstallMeaning,
    OpenExtensions,
    ClearClipboard,
    CopyDiagnostics,
    /// Looks for a newer Heyo, or installs the one that was found.
    Update,
    /// Shows the Models page.
    OpenModels,
    /// Adds the provider whose form is filled in.
    SaveProvider,
    /// Tries the connection whose form is filled in.
    TryConnection,
    /// Adds the connection whose form is filled in, once it was tried.
    SaveConnection,
}

const ACTS: [(Act, &str); 11] = [
    (Act::InstallShellExtension, "install-shell-extension"),
    (Act::InstallOcr, "install-ocr"),
    (Act::InstallMeaning, "install-meaning"),
    (Act::OpenExtensions, "open-extensions"),
    (Act::ClearClipboard, "clear-clipboard"),
    (Act::CopyDiagnostics, "copy-diagnostics"),
    (Act::Update, "update"),
    (Act::OpenModels, "open-models"),
    (Act::SaveProvider, "save-provider"),
    (Act::TryConnection, "try-connection"),
    (Act::SaveConnection, "save-connection"),
];

/// A setting a control of the window changes by itself: a switch, a
/// choice or a field. Which of the settings it is, the page that has the
/// control says (`pages`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Setting {
    LaunchAtLogin,
    Appearance,
    Language,
    CompactMode,
    HideOnBlur,
    KeepCommand,
    LiveActivities,
    DictationHoldKey,
    DictationLive,
    DictationSource,
    DictationLanguage,
    DictationPolish,
    NodePath,
    Renderer,
    ConfirmTerminalPaste,
    ClipboardHistory,
    ClipboardPaused,
    ClipboardRetentionDays,
    ClipboardMeaning,
    SnippetExpansion,
}

const SETTINGS: [(Setting, &str); 20] = [
    (Setting::LaunchAtLogin, "launch-at-login"),
    (Setting::Appearance, "appearance"),
    (Setting::Language, "language"),
    (Setting::CompactMode, "compact-mode"),
    (Setting::HideOnBlur, "hide-on-blur"),
    (Setting::KeepCommand, "keep-command"),
    (Setting::LiveActivities, "live-activities"),
    (Setting::DictationHoldKey, "dictation-hold-key"),
    (Setting::DictationLive, "dictation-live"),
    (Setting::DictationSource, "dictation-source"),
    (Setting::DictationLanguage, "dictation-language"),
    (Setting::DictationPolish, "dictation-polish"),
    (Setting::NodePath, "node-path"),
    (Setting::Renderer, "renderer"),
    (Setting::ConfirmTerminalPaste, "confirm-terminal-paste"),
    (Setting::ClipboardHistory, "clipboard-history"),
    (Setting::ClipboardPaused, "clipboard-paused"),
    (Setting::ClipboardRetentionDays, "clipboard-retention-days"),
    (Setting::ClipboardMeaning, "clipboard-meaning"),
    (Setting::SnippetExpansion, "snippet-expansion"),
];

/// The uses of a model, as a picker of one is named.
const USES: [(Use, &str); 4] = [(Use::Answers, "answers"), (Use::DictationRecognition, "recognition"), (Use::DictationPolish, "polish"), (Use::ClipboardCases, "cases")];

/// Heyo's own hotkeys, as a hotkey field of one is named.
const HOTKEYS: [(HotkeyName, &str); 3] = [(HotkeyName::Toggle, "toggle"), (HotkeyName::Clipboard, "clipboard"), (HotkeyName::Dictation, "dictation")];

/// The word of `known` in `table`.
fn word<T: PartialEq>(table: &[(T, &'static str)], known: &T) -> &'static str {
    table.iter().find(|(one, _)| one == known).map_or("", |(_, word)| word)
}

/// What `word` is in `table`.
fn named<T: Copy>(table: &[(T, &str)], word: &str) -> Option<T> {
    table.iter().find(|(_, known)| *known == word).map(|(one, _)| *one)
}

/// What a picker's choice is written as: the provider and its model.
pub(crate) fn choice(provider: &str, model: &str) -> String {
    format!("{provider}{SEP}{model}")
}

/// The provider and model a picker's choice names; none for no model.
pub(crate) fn chosen(value: &str) -> Option<(&str, &str)> {
    value.split_once(SEP)
}

/// A page of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PageId {
    /// One of the pages of the sidebar's own, by its word (`general`,
    /// `models`, … as `pages` registers them).
    Own(String),
    /// Choosing the kind of a provider to add.
    AddProvider,
    /// The form of the provider being added.
    NewProvider,
    /// A provider's page, by its id.
    Provider(String),
    /// An extension's page, by its id: `@heyo`, `@applications`, or the
    /// directory it is in.
    Extension(String),
    /// Choosing the connector of a connection to add.
    AddConnection,
    /// The form of the connection being added.
    NewConnection,
    /// A connection's page, by its id.
    Connection(String),
}

const PROVIDER: &str = "provider:";
const CONNECTION: &str = "connection:";

impl fmt::Display for PageId {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageId::Own(word) => write!(out, "{word}"),
            PageId::AddProvider => write!(out, "{PROVIDER}add"),
            PageId::NewProvider => write!(out, "{PROVIDER}new"),
            PageId::Provider(id) => write!(out, "{PROVIDER}{id}"),
            PageId::Extension(id) => write!(out, "{id}"),
            PageId::AddConnection => write!(out, "{CONNECTION}add"),
            PageId::NewConnection => write!(out, "{CONNECTION}new"),
            PageId::Connection(id) => write!(out, "{CONNECTION}{id}"),
        }
    }
}

impl From<PageId> for String {
    fn from(page: PageId) -> String {
        page.to_string()
    }
}

impl PageId {
    /// The page `page` names. An extension is named by `@` and a word, or
    /// by a whole path; a page of the sidebar's own by a word.
    pub(crate) fn parse(page: &str) -> PageId {
        match page.strip_prefix(CONNECTION) {
            Some("add") => return PageId::AddConnection,
            Some("new") => return PageId::NewConnection,
            Some(id) => return PageId::Connection(id.to_string()),
            None => {}
        }
        match page.strip_prefix(PROVIDER) {
            Some("add") => PageId::AddProvider,
            Some("new") => PageId::NewProvider,
            Some(id) => PageId::Provider(id.to_string()),
            None if page.starts_with('@') || std::path::Path::new(page).is_absolute() => PageId::Extension(page.to_string()),
            None => PageId::Own(page.to_string()),
        }
    }

    /// One of the pages of the sidebar's own.
    pub(crate) fn own(word: &str) -> PageId {
        PageId::Own(word.to_string())
    }
}

/// What a control acts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Id {
    /// A setting: a switch, a choice or a field of it.
    Set(Setting),
    /// The alias of an item of the palette.
    Alias(String),
    /// Whether a command is enabled.
    Enable(String),
    /// Whether all the commands of an extension are, by its id.
    EnableAll(String),
    /// A group of the Shortcuts table, opened or closed.
    Open(String),
    /// A preference of an extension, or of one of its commands (none:
    /// the extension's own); `kind` is what kind of control it has.
    Preference { extension: String, command: String, name: String, kind: String },
    /// A permission an extension was given, to take it back.
    Revoke { extension: String, permission: Permission },
    /// Whether an extension shows live activities.
    Activities(String),
    Uninstall(String),
    /// The words dictation is to know.
    Words,
    /// An application to leave out of the clipboard history, added or
    /// taken off the list; and the same for snippet expansion.
    IgnoreAdd,
    IgnoreRemove(String),
    ExpandAdd,
    ExpandRemove(String),
    /// The model a use refers to: its picker.
    Pick(Use),
    /// A field of a provider's configuration, or its `name`.
    ProviderField { provider: String, field: String },
    /// The button that forgets a provider's secret field.
    ForgetSecret { provider: String, field: String },
    RemoveProvider(String),
    /// Checks whether a provider answers as it is set up, by its id.
    CheckProvider(String),
    /// A new provider of this kind (by its word), whose form is then shown.
    NewProvider(String),
    /// A field of that new provider's form, or its `name`.
    DraftField(String),
    /// A field of a connection, by its connector's name for it; its name
    /// when that is empty (no field is named so).
    ConnectionField { account: String, field: String },
    RemoveConnection(String),
    /// Checks whether a connection works as it is set up, by its id.
    CheckConnection(String),
    /// Syncs a connection now, by its id.
    SyncConnection(String),
    /// A new connection of this connector (`<extension>:<command>`), whose
    /// form is then shown.
    NewConnection(String),
    /// A field of that new connection's form; its name when empty.
    ConnectionDraft(String),
    /// The preset that new connection's form is filled in from.
    ConnectionPreset,
    /// Shows that page.
    Go(PageId),
    /// A speech model of this computer, by its id: downloaded, its
    /// download called off, deleted, or used for dictation.
    SpeechDownload(String),
    SpeechCancel(String),
    SpeechDelete(String),
    SpeechUse(String),
    Act(Act),
    /// In the menu that narrows the Shortcuts table: one of its
    /// modifications, or one of its categories.
    Modification(String),
    Category(String),
    /// A line that only has a name: a row of the table, by its item's id;
    /// or none at all.
    Named(String),
}

impl fmt::Display for Id {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Id::Set(setting) => write!(out, "set:{}", word(&SETTINGS, setting)),
            Id::Alias(item) => write!(out, "alias:{item}"),
            Id::Enable(command) => write!(out, "enable:{command}"),
            Id::EnableAll(extension) => write!(out, "enable-all:{extension}"),
            Id::Open(group) => write!(out, "open:{group}"),
            Id::Preference { extension, command, name, kind } => write!(out, "pref:{extension}{SEP}{command}{SEP}{name}{SEP}{kind}"),
            Id::Revoke { extension, permission } => write!(out, "revoke:{extension}{SEP}{}", permission.name()),
            Id::Activities(extension) => write!(out, "activities:{extension}"),
            Id::Uninstall(extension) => write!(out, "uninstall:{extension}"),
            Id::Words => write!(out, "words"),
            Id::IgnoreAdd => write!(out, "ignore:add"),
            Id::IgnoreRemove(application) => write!(out, "ignore:remove:{application}"),
            Id::ExpandAdd => write!(out, "expand:add"),
            Id::ExpandRemove(application) => write!(out, "expand:remove:{application}"),
            Id::Pick(need) => write!(out, "pick:{}", word(&USES, need)),
            Id::ProviderField { provider, field } => write!(out, "provider:{provider}{SEP}{field}"),
            Id::ForgetSecret { provider, field } => write!(out, "forget:{provider}{SEP}{field}"),
            Id::CheckProvider(provider) => write!(out, "check-provider:{provider}"),
            Id::NewProvider(kind) => write!(out, "new-provider:{kind}"),
            Id::DraftField(field) => write!(out, "draft:{field}"),
            Id::ConnectionField { account, field } => write!(out, "{CONNECTION}{account}{SEP}{field}"),
            Id::RemoveConnection(account) => write!(out, "remove-connection:{account}"),
            Id::CheckConnection(account) => write!(out, "check-connection:{account}"),
            Id::SyncConnection(account) => write!(out, "sync-connection:{account}"),
            Id::NewConnection(connector) => write!(out, "new-connection:{connector}"),
            Id::ConnectionDraft(field) => write!(out, "connection-draft:{field}"),
            Id::ConnectionPreset => write!(out, "connection-preset"),
            Id::Go(page) => write!(out, "go:{page}"),
            Id::RemoveProvider(provider) => write!(out, "remove-provider:{provider}"),
            Id::SpeechDownload(id) => write!(out, "speech-download:{id}"),
            Id::SpeechCancel(id) => write!(out, "speech-cancel:{id}"),
            Id::SpeechDelete(id) => write!(out, "speech-delete:{id}"),
            Id::SpeechUse(id) => write!(out, "speech-use:{id}"),
            Id::Act(act) => write!(out, "act:{}", word(&ACTS, act)),
            Id::Modification(which) => write!(out, "mod:{which}"),
            Id::Category(which) => write!(out, "cat:{which}"),
            Id::Named(name) => write!(out, "{name}"),
        }
    }
}

impl From<Id> for String {
    fn from(id: Id) -> String {
        id.to_string()
    }
}

impl Id {
    /// What a control handed back.
    pub(crate) fn parse(id: &str) -> Id {
        let own = |rest: &str| rest.to_string();
        match id {
            "words" => return Id::Words,
            "ignore:add" => return Id::IgnoreAdd,
            "expand:add" => return Id::ExpandAdd,
            "connection-preset" => return Id::ConnectionPreset,
            _ => {}
        }
        if let Some(act) = id.strip_prefix("act:").and_then(|name| named(&ACTS, name)) {
            return Id::Act(act);
        }
        if let Some(setting) = id.strip_prefix("set:").and_then(|name| named(&SETTINGS, name)) {
            return Id::Set(setting);
        }
        if let Some(need) = id.strip_prefix("pick:").and_then(|name| named(&USES, name)) {
            return Id::Pick(need);
        }
        if let Some(rest) = id.strip_prefix("pref:") {
            if let [extension, command, name, kind] = rest.split(SEP).collect::<Vec<_>>()[..] {
                return Id::Preference { extension: own(extension), command: own(command), name: own(name), kind: own(kind) };
            }
        }
        if let Some((extension, permission)) = id.strip_prefix("revoke:").and_then(|rest| rest.split_once(SEP)).and_then(|(extension, name)| Some((extension, Permission::named(name)?))) {
            return Id::Revoke { extension: own(extension), permission };
        }
        if let Some((provider, field)) = id.strip_prefix("provider:").and_then(|rest| rest.split_once(SEP)) {
            return Id::ProviderField { provider: own(provider), field: own(field) };
        }
        if let Some((account, field)) = id.strip_prefix(CONNECTION).and_then(|rest| rest.split_once(SEP)) {
            return Id::ConnectionField { account: own(account), field: own(field) };
        }
        if let Some((provider, field)) = id.strip_prefix("forget:").and_then(|rest| rest.split_once(SEP)) {
            return Id::ForgetSecret { provider: own(provider), field: own(field) };
        }
        if let Some(page) = id.strip_prefix("go:") {
            return Id::Go(PageId::parse(page));
        }
        // The longer of two that begin alike first.
        let forms: [(&str, fn(String) -> Id); 23] = [
            ("alias:", Id::Alias),
            ("enable-all:", Id::EnableAll),
            ("enable:", Id::Enable),
            ("open:", Id::Open),
            ("activities:", Id::Activities),
            ("uninstall:", Id::Uninstall),
            ("ignore:remove:", Id::IgnoreRemove),
            ("expand:remove:", Id::ExpandRemove),
            ("mod:", Id::Modification),
            ("cat:", Id::Category),
            ("remove-provider:", Id::RemoveProvider),
            ("check-provider:", Id::CheckProvider),
            ("new-provider:", Id::NewProvider),
            ("draft:", Id::DraftField),
            ("speech-download:", Id::SpeechDownload),
            ("speech-cancel:", Id::SpeechCancel),
            ("speech-delete:", Id::SpeechDelete),
            ("speech-use:", Id::SpeechUse),
            ("remove-connection:", Id::RemoveConnection),
            ("check-connection:", Id::CheckConnection),
            ("sync-connection:", Id::SyncConnection),
            ("new-connection:", Id::NewConnection),
            ("connection-draft:", Id::ConnectionDraft),
        ];
        forms.iter().find_map(|(prefix, form)| id.strip_prefix(prefix).map(|rest| form(own(rest)))).unwrap_or_else(|| Id::Named(own(id)))
    }
}

/// What a hotkey is bound to, as a hotkey field is named: one of Heyo's
/// own hotkeys, or an item of the palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Bound {
    Own(HotkeyName),
    Item(String),
}

impl fmt::Display for Bound {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bound::Own(name) => write!(out, "@{}", word(&HOTKEYS, name)),
            Bound::Item(id) => write!(out, "{id}"),
        }
    }
}

impl Bound {
    pub(crate) fn parse(name: &str) -> Bound {
        match name.strip_prefix('@').and_then(|own| named(&HOTKEYS, own)) {
            Some(own) => Bound::Own(own),
            None => Bound::Item(name.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_is_read_back_as_it_was_written() {
        let s = |word: &str| word.to_string();
        let mut all = vec![
            Id::Alias(s("app:files.desktop")),
            Id::Enable(s("cmd:zeta:one")),
            Id::EnableAll(s("/data/zeta")),
            Id::Open(s("/data/zeta")),
            Id::Preference { extension: s("zeta"), command: s("one"), name: s("loud"), kind: s("checkbox") },
            // An extension's own preference: no command.
            Id::Preference { extension: s("zeta"), command: s(""), name: s("token"), kind: s("password") },
            Id::Revoke { extension: s("zeta"), permission: Permission::Windows },
            Id::Revoke { extension: s("zeta"), permission: Permission::ScreenCapture },
            Id::Activities(s("zeta")),
            Id::Uninstall(s("/data/zeta")),
            Id::Words,
            Id::IgnoreAdd,
            Id::IgnoreRemove(s("firefox.desktop")),
            Id::ExpandAdd,
            Id::ExpandRemove(s("org.gnome.Terminal.desktop")),
            Id::ProviderField { provider: s("openai-compatible-2"), field: s("url") },
            Id::ProviderField { provider: s("claude"), field: s("name") },
            Id::ForgetSecret { provider: s("claude"), field: s("key") },
            Id::RemoveProvider(s("codex")),
            Id::CheckProvider(s("doubao")),
            Id::NewProvider(s("openai-compatible")),
            Id::DraftField(s("url")),
            Id::SpeechDownload(s("whisper-turbo")),
            Id::SpeechCancel(s("parakeet-tdt-0.6b-v3")),
            Id::SpeechDelete(s("x-asr-2026-06-05")),
            Id::SpeechUse(s("sense-voice-2024-07-17")),
            Id::ConnectionField { account: s("imap-2"), field: s("host") },
            Id::ConnectionField { account: s("imap"), field: s("") },
            Id::RemoveConnection(s("imap")),
            Id::CheckConnection(s("imap")),
            Id::SyncConnection(s("imap")),
            Id::NewConnection(s("@heyo:imap")),
            Id::NewConnection(s("fake-connector:fake")),
            Id::ConnectionDraft(s("password")),
            Id::ConnectionDraft(s("")),
            Id::ConnectionPreset,
            Id::Modification(s("hotkey")),
            Id::Category(s("applications")),
            Id::Named(s("cmd:zeta:one")),
            Id::Named(s("")),
        ];
        all.extend(ACTS.iter().map(|(act, _)| Id::Act(*act)));
        all.extend(SETTINGS.iter().map(|(setting, _)| Id::Set(*setting)));
        all.extend(USES.iter().map(|(need, _)| Id::Pick(*need)));
        all.extend(pages().into_iter().map(Id::Go));
        for id in all {
            assert_eq!(Id::parse(&id.to_string()), id, "{id}");
        }
    }

    /// One page of each form.
    fn pages() -> Vec<PageId> {
        let s = |word: &str| word.to_string();
        vec![
            PageId::Own(s("general")),
            PageId::Own(s("models")),
            PageId::AddProvider,
            PageId::NewProvider,
            PageId::Provider(s("openai-compatible-2")),
            PageId::Extension(s("@heyo")),
            PageId::Extension(s("@applications")),
            PageId::Extension(s("/data/zeta")),
            PageId::AddConnection,
            PageId::NewConnection,
            PageId::Connection(s("imap-2")),
        ]
    }

    #[test]
    fn every_page_is_read_back_as_it_was_written() {
        for page in pages() {
            assert_eq!(PageId::parse(&page.to_string()), page, "{page}");
        }
        // As the window had them before they were typed.
        assert_eq!(PageId::parse("provider:add").to_string(), "provider:add");
        assert_eq!(PageId::parse("/src/alpha"), PageId::Extension("/src/alpha".to_string()));
    }

    #[test]
    fn every_word_is_said_once() {
        fn once<T>(table: &[(T, &str)]) {
            let mut words: Vec<&str> = table.iter().map(|(_, word)| *word).collect();
            words.sort();
            words.dedup();
            assert_eq!(words.len(), table.len());
        }
        once(&ACTS);
        once(&SETTINGS);
        once(&USES);
        once(&HOTKEYS);
    }

    #[test]
    fn a_choice_is_read_back_as_its_provider_and_model() {
        assert_eq!(chosen(&choice("ollama", "qwen3:8b")), Some(("ollama", "qwen3:8b")));
        assert_eq!(chosen(&choice("codex", "")), Some(("codex", "")));
        assert_eq!(chosen(""), None);
    }

    #[test]
    fn no_two_ids_are_written_alike() {
        // An item's id is no control's: what begins it is nothing an id does.
        for item in ["app:files.desktop", "cmd:@heyo:clipboard-history", "heyo:screenshot"] {
            assert_eq!(Id::parse(item), Id::Named(item.to_string()));
        }
        // All the commands of an extension, and one command.
        assert_eq!(Id::parse("enable-all:x"), Id::EnableAll("x".to_string()));
        assert_eq!(Id::parse("enable:x"), Id::Enable("x".to_string()));
    }

    #[test]
    fn a_hotkey_is_heyos_own_or_an_items() {
        for bound in [Bound::Own(HotkeyName::Toggle), Bound::Own(HotkeyName::Dictation), Bound::Item("app:files.desktop".to_string())] {
            assert_eq!(Bound::parse(&bound.to_string()), bound);
        }
        assert_eq!(Bound::Own(HotkeyName::Toggle).to_string(), "@toggle");
    }
}
