//! What a control of the settings window acts on, and how that is
//! written where it has to be a word.
//!
//! The window is told lines and hands back which control was used: Slint
//! has that as a string, the line's `id`. What the string says, and how,
//! is this file's alone. The model makes an [`Id`] for each control and
//! is handed one back (`Model::acted`); the markup hands back what it was
//! given and composes nothing, so a line that has more than one control
//! carries an id for each (`Line::more_id`, `check_id`, `alias_id`).
//! Every form is tried there and back in the tests below.

use std::fmt;

/// Between the parts of an id that names several things.
const SEP: char = '\u{1f}';

/// Something Heyo does when a button is pressed, with nothing to say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    InstallShellExtension,
    InstallDictation,
    InstallOcr,
    InstallMeaning,
    OpenExtensions,
    ClearClipboard,
    CopyDiagnostics,
    /// Looks for a newer Heyo, or installs the one that was found.
    Update,
}

const ACTS: [(Act, &str); 8] = [
    (Act::InstallShellExtension, "install-shell-extension"),
    (Act::InstallDictation, "install-dictation"),
    (Act::InstallOcr, "install-ocr"),
    (Act::InstallMeaning, "install-meaning"),
    (Act::OpenExtensions, "open-extensions"),
    (Act::ClearClipboard, "clear-clipboard"),
    (Act::CopyDiagnostics, "copy-diagnostics"),
    (Act::Update, "update"),
];

/// What a control acts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Id {
    /// A setting, by its name: a switch, a choice or a field of it.
    Set(String),
    /// A setting that is a number.
    Num(String),
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
    Revoke { extension: String, permission: String },
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
    /// The API key, typed; and the button that forgets it.
    ApiKey,
    ForgetApiKey,
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
            Id::Set(name) => write!(out, "set:{name}"),
            Id::Num(name) => write!(out, "num:{name}"),
            Id::Alias(item) => write!(out, "alias:{item}"),
            Id::Enable(command) => write!(out, "enable:{command}"),
            Id::EnableAll(extension) => write!(out, "enable-all:{extension}"),
            Id::Open(group) => write!(out, "open:{group}"),
            Id::Preference { extension, command, name, kind } => write!(out, "pref:{extension}{SEP}{command}{SEP}{name}{SEP}{kind}"),
            Id::Revoke { extension, permission } => write!(out, "revoke:{extension}{SEP}{permission}"),
            Id::Activities(extension) => write!(out, "activities:{extension}"),
            Id::Uninstall(extension) => write!(out, "uninstall:{extension}"),
            Id::Words => write!(out, "words"),
            Id::IgnoreAdd => write!(out, "ignore:add"),
            Id::IgnoreRemove(application) => write!(out, "ignore:remove:{application}"),
            Id::ExpandAdd => write!(out, "expand:add"),
            Id::ExpandRemove(application) => write!(out, "expand:remove:{application}"),
            Id::ApiKey => write!(out, "apikey"),
            Id::ForgetApiKey => write!(out, "more:apikey"),
            Id::Act(act) => write!(out, "act:{}", ACTS.iter().find(|(known, _)| known == act).map_or("", |(_, name)| name)),
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
            "apikey" => return Id::ApiKey,
            "more:apikey" => return Id::ForgetApiKey,
            _ => {}
        }
        if let Some(act) = id.strip_prefix("act:").and_then(|name| ACTS.iter().find(|(_, known)| *known == name)) {
            return Id::Act(act.0);
        }
        if let Some(rest) = id.strip_prefix("pref:") {
            if let [extension, command, name, kind] = rest.split(SEP).collect::<Vec<_>>()[..] {
                return Id::Preference { extension: own(extension), command: own(command), name: own(name), kind: own(kind) };
            }
        }
        if let Some((extension, permission)) = id.strip_prefix("revoke:").and_then(|rest| rest.split_once(SEP)) {
            return Id::Revoke { extension: own(extension), permission: own(permission) };
        }
        // The longer of two that begin alike first.
        let forms: [(&str, fn(String) -> Id); 12] = [
            ("set:", Id::Set),
            ("num:", Id::Num),
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
        ];
        forms.iter().find_map(|(prefix, form)| id.strip_prefix(prefix).map(|rest| form(own(rest)))).unwrap_or_else(|| Id::Named(own(id)))
    }
}

/// What a hotkey is bound to, as a hotkey field is named: one of Heyo's
/// own hotkeys, by the host's name for it, or an item of the palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Bound {
    Own(String),
    Item(String),
}

impl fmt::Display for Bound {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bound::Own(name) => write!(out, "@{name}"),
            Bound::Item(id) => write!(out, "{id}"),
        }
    }
}

impl Bound {
    pub(crate) fn parse(name: &str) -> Bound {
        name.strip_prefix('@').map_or_else(|| Bound::Item(name.to_string()), |own| Bound::Own(own.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_is_read_back_as_it_was_written() {
        let s = |word: &str| word.to_string();
        let mut all = vec![
            Id::Set(s("compactMode")),
            Id::Num(s("clipboardRetentionDays")),
            Id::Alias(s("app:files.desktop")),
            Id::Enable(s("cmd:zeta:one")),
            Id::EnableAll(s("/data/zeta")),
            Id::Open(s("/data/zeta")),
            Id::Preference { extension: s("zeta"), command: s("one"), name: s("loud"), kind: s("checkbox") },
            // An extension's own preference: no command.
            Id::Preference { extension: s("zeta"), command: s(""), name: s("token"), kind: s("password") },
            Id::Revoke { extension: s("zeta"), permission: s("windows") },
            Id::Activities(s("zeta")),
            Id::Uninstall(s("/data/zeta")),
            Id::Words,
            Id::IgnoreAdd,
            Id::IgnoreRemove(s("firefox.desktop")),
            Id::ExpandAdd,
            Id::ExpandRemove(s("org.gnome.Terminal.desktop")),
            Id::ApiKey,
            Id::ForgetApiKey,
            Id::Modification(s("hotkey")),
            Id::Category(s("applications")),
            Id::Named(s("cmd:zeta:one")),
            Id::Named(s("")),
        ];
        all.extend(ACTS.iter().map(|(act, _)| Id::Act(*act)));
        for id in all {
            assert_eq!(Id::parse(&id.to_string()), id, "{id}");
        }
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
        for bound in [Bound::Own("toggle".to_string()), Bound::Item("app:files.desktop".to_string())] {
            assert_eq!(Bound::parse(&bound.to_string()), bound);
        }
        assert_eq!(Bound::Own("toggle".to_string()).to_string(), "@toggle");
    }
}
