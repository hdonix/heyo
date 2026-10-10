//! The commands, for an interface that is not a web view.
//!
//! A command is one function (`#[tauri::command]`, under `commands/` and a
//! few modules of their own). The web view reaches it through Tauri; a
//! `heyo-ui` process through the table here, which calls the very same
//! function with arguments read from the very same JSON. So the two
//! interfaces cannot come to mean different things by one name.
//!
//! Each row names the function and its parameters in order. A parameter in
//! lower case is read from the arguments by its name in camel case, as
//! Tauri does; `APP`, `STATE` and `WINDOW` are what Tauri would hand the
//! function itself: the application, something it manages, and the window
//! that asked. The word in front says where the command runs, and matches
//! how Tauri runs it: `Inline` in the order asked (Tauri: the main thread),
//! `Spawn` on a thread of its own (`#[tauri::command(async)]`), `Future`
//! likewise for an `async fn`. Tests keep the table complete and the words
//! true.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use super::Window;
use crate::clipboard::{clipboard_meaning_install, clipboard_meaning_status, clipboard_sources};
use crate::expand::snippet_expanded;
use crate::permissions::{extension_permissions, revoke_extension_permission};
use crate::updates::{check_for_update, install_update};
use crate::*;

/// Where a command runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Inline,
    Spawn,
    Future,
}

/// One invocation: who asked, and with what.
pub(crate) struct Call<'a> {
    pub app: &'a AppHandle,
    pub window: &'a Window,
    pub args: &'a Value,
}

/// `fallback_text` → `fallbackText`: how Tauri names a parameter to the page.
fn camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = false;
    for c in name.chars() {
        match c {
            '_' => upper = true,
            c if upper => {
                out.push(c.to_ascii_uppercase());
                upper = false;
            }
            c => out.push(c),
        }
    }
    out
}

impl Call<'_> {
    /// The argument of that name. One left out is null, which is how an
    /// optional one is left out.
    fn arg<T: DeserializeOwned>(&self, name: &str) -> Result<T, String> {
        let key = camel(name);
        let value = self.args.get(&key).cloned().unwrap_or(Value::Null);
        serde_json::from_value(value).map_err(|e| format!("invalid args `{key}`: {e}"))
    }
}

/// What a command returned, on its way to being an answer. A command
/// returns a value or a `Result` of one; which of the two is told apart
/// by which of the two traits below applies (the first that does, going
/// from the outermost reference inwards).
pub(crate) struct Answer<T>(pub Option<T>);

pub(crate) trait Fallible {
    fn settle(self) -> Result<Value, String>;
}

impl<T: Serialize, E: std::fmt::Display> Fallible for &mut &mut Answer<Result<T, E>> {
    fn settle(self) -> Result<Value, String> {
        match self.0.take().expect("settled once") {
            Ok(value) => serde_json::to_value(value).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }
}

pub(crate) trait Plain {
    fn settle(self) -> Result<Value, String>;
}

impl<T: Serialize> Plain for &mut Answer<T> {
    fn settle(self) -> Result<Value, String> {
        serde_json::to_value(self.0.take().expect("settled once")).map_err(|e| e.to_string())
    }
}

macro_rules! arg {
    ($call:ident, APP) => {
        $call.app.clone()
    };
    ($call:ident, STATE) => {
        $call.app.state()
    };
    ($call:ident, WINDOW) => {
        $call.window.clone()
    };
    ($call:ident, $name:ident) => {
        $call.arg(stringify!($name))?
    };
}

macro_rules! finish {
    (Future, $returned:expr) => {
        tauri::async_runtime::block_on($returned)
    };
    ($mode:ident, $returned:expr) => {
        $returned
    };
}

macro_rules! commands {
    ($($mode:ident $name:ident($($arg:ident),*);)*) => {
        /// Every command an interface process can invoke, and where it runs.
        pub(crate) const COMMANDS: &[(&str, Mode)] = &[$((stringify!($name), Mode::$mode)),*];

        /// Runs the command of that name to its end.
        pub(crate) fn run(call: &Call, command: &str) -> Result<Value, String> {
            $(
                if command == stringify!($name) {
                    let returned = finish!($mode, $name($(arg!(call, $arg)),*));
                    return (&mut &mut Answer(Some(returned))).settle();
                }
            )*
            Err(format!("{command} is not a command"))
        }
    };
}

commands! {
    // commands/root.rs
    Inline search(query, APP);
    Inline get_item(id, APP);
    Inline list_applications(APP);
    Spawn launch(id, fallback_text, arguments, APP);
    Inline list_quicklinks(APP);
    Spawn record_usage(id, STATE);
    Spawn set_favorite(id, favorite, APP);
    Spawn move_favorite(id, offset, APP);
    Spawn set_alias(id, alias, APP);
    Spawn reset_ranking(id, STATE, APP);
    Spawn read_clipboard(APP);
    Spawn copy_text(text, paste, APP);
    Inline open_target(target, APP);
    Inline runtime_send(message, STATE, APP);
    Spawn show_window(APP);
    Inline hide(WINDOW);
    // commands/media.rs
    Future screenshot_finish(request, APP);
    Inline ocr_status();
    Inline ocr_install(APP);
    Spawn dictation_status(APP);
    Inline dictation_install(APP);
    Spawn dictation_toggle(APP);
    Spawn dictation_cancel(APP);
    // commands/settings.rs
    Future local_models(url);
    Spawn get_settings(APP, WINDOW);
    Spawn change_settings(changes, APP);
    Spawn get_capabilities();
    Spawn install_shell_extension();
    Spawn set_anthropic_api_key(key, APP);
    Future open_settings(APP);
    Future get_hotkeys();
    Future set_hotkey(name, accelerator);
    Future set_item_hotkey(id, title, accelerator, APP);
    Inline item_hotkeys(STATE);
    Spawn list_extensions();
    Spawn set_extension_preferences(extension, values);
    Spawn uninstall_extension(id, APP);
    Spawn clear_clipboard_history(STATE);
    Inline open_extensions_folder();
    Spawn copy_diagnostics(APP);
    // clipboard.rs, updates.rs, permissions.rs, expand.rs
    Inline clipboard_sources(APP);
    Spawn clipboard_meaning_status(APP);
    Inline clipboard_meaning_install(APP);
    Future check_for_update(APP);
    Future install_update(APP);
    Spawn extension_permissions(id);
    Spawn revoke_extension_permission(extension, permission);
    Inline snippet_expanded(request, text);
}

pub(crate) fn mode(command: &str) -> Option<Mode> {
    COMMANDS.iter().find(|(name, _)| *name == command).map(|(_, mode)| *mode)
}

/// Answers a command a window's process invoked: refused unless that window
/// may invoke it (the table the web view's windows are held to), then run
/// where it runs. `answer` is called once, with the result.
pub(crate) fn invoke(app: &AppHandle, window: &Window, command: &str, args: Value, answer: impl FnOnce(Result<Value, String>) + Send + 'static) {
    if !crate::commands::access::allowed(window.label(), command) {
        eprintln!("heyo: refused {command} to the window {}", window.label());
        return answer(Err(format!("{command} is not available to this window")));
    }
    match mode(command) {
        None => answer(Err(format!("{command} is not a command"))),
        Some(Mode::Inline) => answer(run(&Call { app, window, args: &args }, command)),
        Some(Mode::Spawn | Mode::Future) => {
            let (app, window, command) = (app.clone(), window.clone(), command.to_owned());
            std::thread::spawn(move || answer(run(&Call { app: &app, window: &window, args: &args }, &command)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameters_are_named_to_the_interface_as_tauri_names_them() {
        assert_eq!(camel("query"), "query");
        assert_eq!(camel("fallback_text"), "fallbackText");
        assert_eq!(camel("a_b_c"), "aBC");
    }

    #[test]
    fn a_result_and_a_plain_value_both_become_answers() {
        assert_eq!((&mut &mut Answer(Some(3))).settle(), Ok(Value::from(3)));
        assert_eq!((&mut &mut Answer(Some(()))).settle(), Ok(Value::Null));
        assert_eq!((&mut &mut Answer(Some(vec!["a"]))).settle(), Ok(serde_json::json!(["a"])));
        assert_eq!((&mut &mut Answer(Some(Ok::<_, String>(3)))).settle(), Ok(Value::from(3)));
        assert_eq!((&mut &mut Answer(Some(Err::<u8, _>("no".to_owned())))).settle(), Err("no".into()));
        // A result that is part of a value stays part of it.
        assert_eq!((&mut &mut Answer(Some(vec![Ok::<u8, String>(1)]))).settle(), Ok(serde_json::json!([{ "Ok": 1 }])));
    }

    /// The names between the brackets of `generate_handler![…]` in `lib.rs`.
    fn registered() -> Vec<String> {
        let source = include_str!("../lib.rs");
        let list = source.split("generate_handler![").nth(1).and_then(|rest| rest.split(']').next()).expect("lib.rs registers its commands with generate_handler!");
        list.split(',').map(|name| name.trim().rsplit("::").next().unwrap_or_default().to_owned()).filter(|name| !name.is_empty()).collect()
    }

    /// A command the web view can invoke and a `heyo-ui` window cannot
    /// would be a view that works in one interface only.
    #[test]
    fn every_command_of_the_web_view_can_be_invoked_over_the_bridge() {
        let registered = registered();
        assert!(registered.len() > 30, "could not read the commands out of lib.rs: {registered:?}");
        for command in &registered {
            let rows = COMMANDS.iter().filter(|(name, _)| name == command).count();
            assert_eq!(rows, 1, "{command} is registered in lib.rs: give it a row in shell/dispatch.rs (once)");
        }
        for (command, _) in COMMANDS {
            assert!(registered.iter().any(|name| name == command), "{command} is in shell/dispatch.rs but not registered in lib.rs");
        }
    }

    /// How each command is declared, read from where it is declared.
    fn declared() -> Vec<(String, Mode)> {
        let sources = [
            include_str!("../commands/root.rs"),
            include_str!("../commands/media.rs"),
            include_str!("../commands/settings.rs"),
            include_str!("../clipboard.rs"),
            include_str!("../updates.rs"),
            include_str!("../permissions.rs"),
            include_str!("../expand.rs"),
        ];
        let mut found = Vec::new();
        for source in sources {
            for after in source.split("#[tauri::command").skip(1) {
                let (attribute, rest) = after.split_once(']').unwrap();
                let (before, rest) = rest.split_once("fn ").unwrap();
                let name = rest.split('(').next().unwrap().trim().to_owned();
                let mode = if before.contains("async") {
                    Mode::Future
                } else if attribute.contains("async") {
                    Mode::Spawn
                } else {
                    Mode::Inline
                };
                found.push((name, mode));
            }
        }
        found
    }

    /// What is typed must reach the runtime in the order it was typed, and
    /// what may wait on other programs must not hold up what follows: both
    /// are decided where the command is declared, and hold here too.
    #[test]
    fn a_command_runs_over_the_bridge_as_tauri_runs_it() {
        let declared = declared();
        assert_eq!(declared.len(), COMMANDS.len(), "the commands declared and the rows of the table");
        for (name, expected) in declared {
            assert_eq!(mode(&name), Some(expected), "{name}: the word in front of its row in shell/dispatch.rs");
        }
    }

    #[test]
    fn what_is_not_a_command_has_no_mode() {
        assert_eq!(mode("search"), Some(Mode::Inline));
        assert_eq!(mode("launch"), Some(Mode::Spawn));
        assert_eq!(mode("open_settings"), Some(Mode::Future));
        assert_eq!(mode("format_disk"), None);
    }
}
