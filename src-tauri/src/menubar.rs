//! Menu bar and background commands of extensions.
//!
//! A menu bar command becomes a tray icon whose menu is what the command
//! rendered; a command with an `interval` in its manifest runs again on that
//! schedule. Both become active once they have been run from Heyo, and
//! run in the background runtime process (see `runtime/src/background.ts`),
//! which keeps menu bar commands mounted so a click on an item is answered
//! at once.

use crate::i18n::t;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::wire::HostMessage;
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};

use crate::extensions::Command;
use crate::runtime::{Background, Runtime};

/// Prefix of the ids of menu items that belong to a command's menu.
const ITEM: &str = "menubar";
/// How often the schedule is checked.
const TICK: Duration = Duration::from_secs(5);
/// A run that never reports back is forgotten after this long.
const STUCK: Duration = Duration::from_secs(120);

#[derive(Default)]
pub struct MenuBar(Mutex<State>);

#[derive(Default)]
struct State {
    /// Tray icons by command key (`<extension dir>:<command>`). Tauri keeps
    /// its own handle to each, so one only goes away through `remove_tray`.
    trays: HashMap<String, TrayIcon>,
    /// When each command last started, by key.
    started: HashMap<String, Instant>,
    /// Runs in progress: id → (key, whether the user asked for it).
    running: HashMap<u64, (String, bool, Instant)>,
    next_run: u64,
    next_tray: u64,
}

fn key(command: &Command) -> String {
    format!("{}:{}", command.extension, command.name)
}

/// What `launchCommand` hands to the command it starts.
#[derive(Default)]
struct Given {
    arguments: Option<Value>,
    context: Option<Value>,
}

/// Starts a command in the background runtime. `action` is the path of a
/// clicked menu item.
fn run(app: &AppHandle, key: &str, user: bool, action: Option<Vec<usize>>, given: Given) {
    let Some((extension, command)) = key.rsplit_once(':') else { return };
    let id = {
        let state = app.state::<MenuBar>();
        let mut state = state.0.lock().unwrap();
        state.next_run += 1;
        let id = state.next_run;
        // A click is answered by the command as it is mounted: it is not a
        // fresh run, and does not put off the next one.
        if action.is_none() {
            state.started.insert(key.to_owned(), Instant::now());
        }
        state.running.insert(id, (key.to_owned(), user, Instant::now()));
        id
    };
    let message = HostMessage::BackgroundRun {
        id,
        extension: extension.into(),
        command: command.into(),
        launch_type: if user { "userInitiated" } else { "background" },
        action,
        arguments: given.arguments,
        launch_context: given.context,
    }
    .json();
    if let Err(e) = app.state::<Background>().0.send(app, &message) {
        app.state::<MenuBar>().0.lock().unwrap().running.remove(&id);
        eprintln!("heyo: {e}");
        if user {
            crate::notify(t("Could not run the command"), &e);
        }
    }
}

/// Runs a menu bar or background command because the user opened it.
pub fn run_now(app: &AppHandle, command: &Command) {
    run(app, &key(command), true, None, Given::default());
}

/// Where `launchCommand` starts a command.
#[derive(Debug, PartialEq)]
enum Place {
    Background,
    Palette,
}

/// Where a command of `mode` runs when it is asked for in the background or not.
fn place(mode: &str, background: bool) -> Result<Place, String> {
    match (mode, background) {
        ("menu-bar", _) | ("no-view", true) => Ok(Place::Background),
        (_, true) => Err("a command with a view cannot be launched in the background".into()),
        (_, false) => Ok(Place::Palette),
    }
}

/// A command called `launchCommand` for one that runs in the other runtime
/// process: a background run, or (from a menu bar command) one in the palette.
pub fn launch_command(
    app: &AppHandle,
    extension: &str,
    command: &str,
    background: bool,
    arguments: Option<Value>,
    context: Option<Value>,
    fallback_text: Option<String>,
) -> Result<(), String> {
    let found = {
        let index = app.state::<crate::IndexState>();
        let index = index.0.read().unwrap();
        index.commands().iter().find(|c| c.extension == extension && c.name == command).cloned()
    }
    .ok_or_else(|| format!("no command {command} in {extension} (is the extension disabled?)"))?;
    match place(&found.mode, background)? {
        Place::Background => run(app, &key(&found), !background, None, Given { arguments, context }),
        Place::Palette => {
            crate::palette::show(app);
            let launch = HostMessage::Launch { extension: found.extension, command: found.name, fallback_text, arguments, launch_context: context };
            app.state::<Runtime>().send(app, &launch.json())?;
        }
    }
    Ok(())
}

/// Runs the command that handles an extension's live activities, in the
/// background, with the event as its launch context (`activities.rs`).
/// `user` says the user caused the event, so the run may start activities.
pub fn run_for_event(app: &AppHandle, command: &Command, user: bool, context: Value) {
    run(app, &key(command), user, None, Given { arguments: None, context: Some(context) });
}

/// Whether a command belongs here rather than in the palette.
pub fn handles(command: &Command) -> bool {
    command.mode == "menu-bar"
}

/// The commands that are active: enabled, and run at least once by the user.
fn active(app: &AppHandle) -> Vec<Command> {
    let index = app.state::<crate::IndexState>();
    let index = index.0.read().unwrap();
    let usage = app.state::<crate::usage::UsageStore>();
    index
        .commands()
        .iter()
        .filter(|c| c.extension != crate::extensions::BUILTIN)
        .filter(|c| c.mode == "menu-bar" || (c.mode == "no-view" && c.interval.is_some()))
        .filter(|c| usage.used(&c.id()))
        .cloned()
        .collect()
}

/// Starts the schedule: menu bar commands appear right away, and everything
/// with an interval runs when it is due.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let launched = Instant::now();
        loop {
            let commands = active(&app);
            let mut due = Vec::new();
            {
                let state = app.state::<MenuBar>();
                let mut state = state.0.lock().unwrap();
                state.running.retain(|_, (_, _, since)| since.elapsed() < STUCK);
                for command in &commands {
                    let key = key(command);
                    if state.running.values().any(|(k, ..)| *k == key) {
                        continue;
                    }
                    let is_due = match (state.started.get(&key), command.interval) {
                        (Some(last), Some(interval)) => last.elapsed() >= interval,
                        (Some(_), None) => false,
                        // First time this session: menu bar commands show up
                        // at once; plain background ones wait for their interval.
                        (None, interval) => command.mode == "menu-bar" || interval.is_some_and(|i| launched.elapsed() >= i),
                    };
                    if is_due {
                        due.push(key);
                    }
                }
                // Commands that were disabled or uninstalled leave the menu bar,
                // and may start afresh if they come back.
                let keys: Vec<String> = commands.iter().map(key).collect();
                state.started.retain(|k, _| keys.contains(k));
                let gone: Vec<String> = state.trays.keys().filter(|k| !keys.contains(k)).cloned().collect();
                drop(state);
                for key in gone {
                    remove_tray(&app, key);
                }
            }
            for key in due {
                run(&app, &key, false, None, Given::default());
            }
            std::thread::sleep(TICK);
        }
    });
}

/// Stops the background runtime, which starts again when something is next
/// asked of it (with another Node.js, say). What was mounted in it is gone,
/// so the menu bar's commands start afresh at the schedule's next look; the
/// others keep to their intervals.
pub fn restart(app: &AppHandle) {
    app.state::<Background>().0.stop();
    let in_menu_bar: Vec<String> = active(app).iter().filter(|command| handles(command)).map(key).collect();
    app.state::<MenuBar>().0.lock().unwrap().started.retain(|key, _| !in_menu_bar.contains(key));
}

/// Takes a command's icon out of the menu bar.
fn remove_tray(app: &AppHandle, key: String) {
    let handle = app.clone();
    // Tray icons belong to the main thread.
    let _ = app.run_on_main_thread(move || {
        let tray = handle.state::<MenuBar>().0.lock().unwrap().trays.remove(&key);
        if let Some(tray) = tray {
            handle.remove_tray_by_id(tray.id());
        }
    });
}

/// The background runtime stopped: its runs will never report back.
pub fn runtime_exited(app: &AppHandle) {
    app.state::<MenuBar>().0.lock().unwrap().running.clear();
}

/// A message from the background runtime.
pub fn handle(app: &AppHandle, message: &Value) {
    match message["type"].as_str() {
        Some("menu") => {
            let Some(key) = message["key"].as_str().map(str::to_owned) else { return };
            let menu = message["menu"].clone();
            let handle = app.clone();
            // Tray icons and menus belong to the main thread.
            let _ = app.run_on_main_thread(move || {
                if let Err(e) = show(&handle, &key, &menu) {
                    eprintln!("heyo: menu bar: {e}");
                }
            });
        }
        Some("background-done") => {
            let run = message["id"].as_u64().and_then(|id| app.state::<MenuBar>().0.lock().unwrap().running.remove(&id));
            if let (Some((key, user, _)), Some(error)) = (run, message["error"].as_str()) {
                eprintln!("heyo: {key}: {error}");
                if user {
                    crate::notify(t("The command failed"), error);
                }
            }
        }
        // Toasts and the like have nowhere to go.
        _ => {}
    }
}

/// The icon for a tray: the one the command gave, the command's own, or Heyo's.
fn icon(app: &AppHandle, key: &str, file: Option<&str>) -> Option<Image<'static>> {
    let from_command = || {
        let index = app.state::<crate::IndexState>();
        let index = index.0.read().unwrap();
        index.commands().iter().find(|c| self::key(c) == key).and_then(|c| c.icon.clone())
    };
    let load = |path: String| {
        let image = image::open(path).ok()?.into_rgba8();
        let (width, height) = image.dimensions();
        Some(Image::new_owned(image.into_raw(), width, height))
    };
    file.map(str::to_owned).and_then(load).or_else(|| from_command().and_then(load)).or_else(|| app.default_window_icon().cloned().map(Image::to_owned))
}

/// Builds the items of a menu; `path` is where they are in the command's tree.
fn items(app: &AppHandle, key: &str, nodes: &[Value], path: &[usize]) -> tauri::Result<Vec<Box<dyn IsMenuItem<Wry>>>> {
    let mut out: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let path: Vec<usize> = path.iter().copied().chain([index]).collect();
        let id = format!("{ITEM}|{key}|{}", path.iter().map(usize::to_string).collect::<Vec<_>>().join("."));
        let title = node["title"].as_str().unwrap_or_default();
        let children = node["children"].as_array().map(Vec::as_slice).unwrap_or_default();
        match node["kind"].as_str() {
            Some("separator") => out.push(Box::new(PredefinedMenuItem::separator(app)?)),
            Some("submenu") => {
                let children = items(app, key, children, &path)?;
                let refs: Vec<&dyn IsMenuItem<Wry>> = children.iter().map(|c| c.as_ref()).collect();
                out.push(Box::new(Submenu::with_id_and_items(app, id, title, true, &refs)?));
            }
            Some("section") => {
                if !out.is_empty() {
                    out.push(Box::new(PredefinedMenuItem::separator(app)?));
                }
                if !title.is_empty() {
                    out.push(Box::new(MenuItem::new(app, title, false, None::<&str>)?));
                }
                out.extend(items(app, key, children, &path)?);
            }
            _ => {
                let label = match node["subtitle"].as_str().filter(|s| !s.is_empty()) {
                    Some(subtitle) => format!("{title}    {subtitle}"),
                    None => title.to_owned(),
                };
                out.push(Box::new(MenuItem::with_id(app, id, label, node["enabled"].as_bool().unwrap_or(false), None::<&str>)?));
            }
        }
    }
    Ok(out)
}

/// Shows, updates or (for a null menu) removes a command's tray icon.
fn show(app: &AppHandle, key: &str, menu: &Value) -> tauri::Result<()> {
    let state = app.state::<MenuBar>();
    if menu.is_null() {
        // Already on the main thread.
        let tray = state.0.lock().unwrap().trays.remove(key);
        if let Some(tray) = tray {
            app.remove_tray_by_id(tray.id());
        }
        return Ok(());
    }
    let mut built = items(app, key, menu["items"].as_array().map(Vec::as_slice).unwrap_or_default(), &[])?;
    if !built.is_empty() {
        built.push(Box::new(PredefinedMenuItem::separator(app)?));
    }
    built.push(Box::new(MenuItem::with_id(app, format!("{ITEM}|{key}|refresh"), t("Refresh"), true, None::<&str>)?));
    let refs: Vec<&dyn IsMenuItem<Wry>> = built.iter().map(|c| c.as_ref()).collect();
    let tray_menu = Menu::with_items(app, &refs)?;
    let title = menu["title"].as_str();
    let tooltip = menu["tooltip"].as_str();
    let icon = icon(app, key, menu["icon"].as_str());

    let mut state = state.0.lock().unwrap();
    if let Some(tray) = state.trays.get(key) {
        tray.set_menu(Some(tray_menu))?;
        tray.set_title(title)?;
        tray.set_tooltip(tooltip)?;
        return tray.set_icon(icon);
    }
    // The id ends up in a file name, so it cannot be the key (a path).
    state.next_tray += 1;
    let mut builder = TrayIconBuilder::with_id(format!("heyo-menubar-{}", state.next_tray)).menu(&tray_menu).show_menu_on_left_click(true);
    if let Some(title) = title {
        builder = builder.title(title);
    }
    if let Some(tooltip) = tooltip {
        builder = builder.tooltip(tooltip);
    }
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }
    let tray = builder.build(app)?;
    state.trays.insert(key.to_owned(), tray);
    Ok(())
}

/// What a tray menu item's id says: the command's key, and the path of the
/// item (none for "Refresh"). None if the id is not one of ours.
fn click(id: &str) -> Option<(&str, Option<Vec<usize>>)> {
    let rest = id.strip_prefix(ITEM)?.strip_prefix('|')?;
    let (key, path) = rest.rsplit_once('|')?;
    let action = match path {
        "refresh" => None,
        path => Some(path.split('.').map(str::parse).collect::<Result<Vec<usize>, _>>().ok()?),
    };
    Some((key, action))
}

/// A click on a tray menu item; false if the id is not one of ours.
pub fn clicked(app: &AppHandle, id: &str) -> bool {
    let Some((key, action)) = click(id) else { return false };
    run(app, key, true, action, Given::default());
    true
}

#[cfg(test)]
mod tests {
    use super::{click, place, Place};

    #[test]
    fn a_menu_item_s_id_names_its_command_and_its_path() {
        assert_eq!(click("menubar|/x/weather:now|0.2"), Some(("/x/weather:now", Some(vec![0, 2]))));
        assert_eq!(click("menubar|/x/weather:now|refresh"), Some(("/x/weather:now", None)));
        // A path may have a bar in it; the last one divides.
        assert_eq!(click("menubar|/x/odd|dir:now|3"), Some(("/x/odd|dir:now", Some(vec![3]))));
        assert_eq!(click("menubar|/x/weather:now|first"), None);
        assert_eq!(click("quit"), None);
    }

    #[test]
    fn a_launched_command_runs_where_it_belongs() {
        // The menu bar is in the background process, however it was asked for.
        assert_eq!(place("menu-bar", false), Ok(Place::Background));
        assert_eq!(place("menu-bar", true), Ok(Place::Background));
        assert_eq!(place("no-view", true), Ok(Place::Background));
        assert_eq!(place("no-view", false), Ok(Place::Palette));
        assert_eq!(place("view", false), Ok(Place::Palette));
        assert!(place("view", true).is_err());
    }

    #[test]
    fn command_keys_survive_colons_in_paths() {
        let key = "/home/me/odd:dir/extensions/weather:forecast";
        assert_eq!(key.rsplit_once(':'), Some(("/home/me/odd:dir/extensions/weather", "forecast")));
    }
}
