//! Opening a root search item by its id: the one way in, whether it was
//! chosen in the palette, bound to a hotkey or asked for with `heyo --open`.
//!
//! An id says what it names: `app:<desktop id>`, `cmd:<extension>:<command>`,
//! `heyo:<native command>`, `quicklink:<id>`, `snippet:<id>`.

use tauri::{AppHandle, Emitter, Manager};

use crate::root::{Index, NATIVE};
use crate::wire::{event, HostMessage};
use crate::{apps, extensions, menubar, IndexState, Runtime, UsageStore};

/// What opening an item comes to.
#[derive(Debug)]
pub(crate) enum Way {
    /// One of Heyo's own commands, done by the host itself.
    Native,
    App(apps::App),
    /// A command that lives in the menu bar, not the palette.
    MenuBar(extensions::Command),
    /// A command the extension runtime runs in the palette.
    Command(extensions::Command),
    /// A quicklink or snippet: the webview has what it takes to fill one in.
    Webview,
    Missing,
}

pub(crate) fn way(index: &Index, id: &str) -> Way {
    if NATIVE.iter().any(|native| native.id == id) {
        return Way::Native;
    }
    if let Some(desktop_id) = id.strip_prefix("app:") {
        return index.app(desktop_id).cloned().map_or(Way::Missing, Way::App);
    }
    if id.starts_with("quicklink:") || id.starts_with("snippet:") {
        return Way::Webview;
    }
    match index.command(id) {
        Some(command) if menubar::handles(command) => Way::MenuBar(command.clone()),
        Some(command) => Way::Command(command.clone()),
        None => Way::Missing,
    }
}

/// Where the wish to open something came from.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Origin {
    /// The palette, which is up and stays for a command it started.
    Palette,
    /// A hotkey or the command line: the palette comes up only if needed.
    Outside,
}

/// Opens the item. `Ok(false)`: it is the webview's to open (a quicklink or
/// a snippet), and nothing was done.
pub(crate) fn open(app: &AppHandle, id: &str, fallback_text: Option<String>, origin: Origin) -> Result<bool, String> {
    open_with(app, id, fallback_text, None, origin)
}

/// The arguments a command is launched with: a value for each one it
/// declares (empty when none was given), and nothing it does not declare.
pub(crate) fn declared(command: &extensions::Command, mut given: Arguments) -> Arguments {
    command.arguments.iter().map(|a| (a.name.clone(), given.remove(&a.name).unwrap_or_default())).collect()
}

pub(crate) type Arguments = std::collections::BTreeMap<String, String>;

/// [`open`], with the arguments filled in in the root search for a command
/// that takes some. Without them, the command asks for them itself.
pub(crate) fn open_with(app: &AppHandle, id: &str, fallback_text: Option<String>, arguments: Option<Arguments>, origin: Origin) -> Result<bool, String> {
    let way = way(&app.state::<IndexState>().0.read().unwrap(), id);
    let hide_palette = || {
        if origin == Origin::Palette {
            let _ = crate::main_window(app).hide();
        }
    };
    match way {
        Way::Native => {
            crate::open_native(app, id);
        }
        Way::App(entry) => {
            hide_palette();
            crate::open_app(app, &entry)?;
        }
        Way::MenuBar(command) => {
            menubar::run_now(app, &command);
            hide_palette();
        }
        Way::Command(command) => {
            if origin == Origin::Outside {
                crate::show(app);
            }
            let arguments = arguments.filter(|_| !command.arguments.is_empty()).map(|given| declared(&command, given));
            let arguments = arguments.map(|given| serde_json::json!(given));
            let launch = HostMessage::Launch { extension: command.extension.clone(), command: command.name.clone(), fallback_text, arguments, launch_context: None };
            app.state::<Runtime>().send(app, &launch.json())?;
        }
        Way::Webview => return Ok(false),
        Way::Missing => return Err(format!("nothing to open for {id} (was it uninstalled?)")),
    }
    Ok(true)
}

/// `heyo --open <id>`, and the global hotkeys bound to items: counts the
/// use, and hands quicklinks and snippets to the webview (which shows
/// itself if it needs input).
pub(crate) fn open_item(app: &AppHandle, id: &str) {
    app.state::<UsageStore>().record(id);
    let result = open(app, id, None, Origin::Outside).and_then(|opened| match opened {
        true => Ok(()),
        false => crate::main_window(app).emit(event::OPEN_ITEM, id).map_err(|e| e.to_string()),
    });
    if let Err(e) = result {
        eprintln!("heyo: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Quicklink, Snippet};

    fn command(extension: &str, name: &str, mode: &str) -> extensions::Command {
        extensions::Command {
            extension: extension.into(),
            name: name.into(),
            title: name.into(),
            subtitle: None,
            keywords: Vec::new(),
            icon: None,
            mode: mode.into(),
            interval: None,
            arguments: Vec::new(),
            handles_activities: false,
        }
    }

    #[test]
    fn a_command_gets_the_arguments_it_declares_and_no_others() {
        let argument = |name: &str| extensions::Argument { name: name.into(), kind: "text".into(), placeholder: None, required: false, data: Vec::new() };
        let command = extensions::Command { arguments: vec![argument("city"), argument("unit")], ..command("/ext/weather", "at", "view") };
        let given = [("city", "Oslo"), ("verbose", "yes")].map(|(k, v)| (k.to_owned(), v.to_owned())).into();
        let expected: Arguments = [("city", "Oslo"), ("unit", "")].map(|(k, v)| (k.to_owned(), v.to_owned())).into();
        assert_eq!(declared(&command, given), expected);
    }

    fn index() -> Index {
        let app = apps::App {
            id: "org.gnome.Nautilus.desktop".into(),
            name: "Files".into(),
            generic_name: None,
            comment: None,
            keywords: Vec::new(),
            exec: "nautilus".into(),
            icon: None,
            terminal: false,
            working_dir: None,
            path: "/usr/share/applications/org.gnome.Nautilus.desktop".into(),
            startup_wm_class: None,
        };
        let commands = vec![command("@heyo", "emoji", "view"), command("/ext/weather", "now", "menu-bar")];
        let quicklinks = vec![Quicklink { id: "1".into(), name: "GitHub".into(), link: "https://github.com/{query}".into(), icon: None }];
        let snippets = vec![Snippet { id: "2".into(), name: "Address".into(), text: "1 Main St".into(), keyword: None }];
        Index::new(vec![app], commands, quicklinks, snippets)
    }

    #[test]
    fn an_id_says_what_it_opens() {
        let index = index();
        assert!(matches!(way(&index, "app:org.gnome.Nautilus.desktop"), Way::App(app) if app.name == "Files"));
        assert!(matches!(way(&index, "cmd:@heyo:emoji"), Way::Command(c) if c.name == "emoji"));
        assert!(matches!(way(&index, "cmd:/ext/weather:now"), Way::MenuBar(c) if c.name == "now"));
        assert!(matches!(way(&index, crate::root::SETTINGS_ID), Way::Native));
        assert!(matches!(way(&index, "quicklink:1"), Way::Webview));
        assert!(matches!(way(&index, "snippet:2"), Way::Webview));
    }

    #[test]
    fn what_is_gone_is_missing_rather_than_guessed() {
        let index = index();
        assert!(matches!(way(&index, "app:removed.desktop"), Way::Missing));
        assert!(matches!(way(&index, "cmd:/ext/uninstalled:x"), Way::Missing));
        assert!(matches!(way(&index, "heyo:no-such-command"), Way::Missing));
        assert!(matches!(way(&index, ""), Way::Missing));
    }
}
