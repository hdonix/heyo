mod apps;
mod cli;
mod clipboard;
mod commands;
mod dictation;
mod extensions;
mod files;
mod helper;
mod host;
mod links;
mod menubar;
mod library;
mod ocr;
mod open;
mod palette;
mod paths;
pub mod renderer;
mod platform;
mod root;
mod runtime;
mod screenshot;
mod search;
mod settings;
mod usage;
mod wire;
mod updates;
mod confirm;
mod diagnostics;
mod pasting;
mod ext;
mod permissions;
mod expand;
mod activities;
mod slint_windows;

use std::collections::HashMap;
use std::sync::RwLock;

use tauri::{AppHandle, Emitter, Manager};

use cli::*;
use open::open_item;
use clipboard::ClipboardHistory;
use commands::{root::*, settings::*};
use host::*;
use platform::{hotkeys, windows};
use platform::{common::*, focus::*, input::*};
use dictation::Dictation;
use extensions::Command;
use root::Index;
use runtime::Runtime;
use settings::Settings;
use usage::UsageStore;
use wire::{event, HostMessage, WebviewMessage};

#[derive(Default)]
struct IndexState(RwLock<Index>);

struct SettingsState(RwLock<Settings>);

/// Installed commands, minus those disabled in the settings.
fn enabled_commands(app: &AppHandle) -> Vec<Command> {
    let settings = app.state::<SettingsState>();
    let disabled = &settings.0.read().unwrap().disabled_commands;
    extensions::discover()
        .into_iter()
        .filter(|c| !disabled.contains(&c.id()))
        // Only what can work in this session.
        .filter(|c| platform::item_available(&c.id()))
        .collect()
}

/// Rebuilds everything but the applications (commands, quicklinks, snippets).
fn reload_commands(app: &AppHandle) {
    let commands = enabled_commands(app);
    let state = app.state::<IndexState>();
    let mut index = state.0.write().unwrap();
    *index = Index::new(std::mem::take(&mut index.apps), commands, library::quicklinks(), library::snippets());
    drop(index);
    let _ = app.emit(crate::wire::event::INDEX_READY, ());
}

/// Reads the installed applications again and rebuilds the root search.
fn reload_apps(app: &AppHandle) {
    let apps = apps::discover();
    let commands = enabled_commands(app);
    let index = Index::new(apps, commands, library::quicklinks(), library::snippets());
    *app.state::<IndexState>().0.write().unwrap() = index;
    let _ = app.emit(crate::wire::event::INDEX_READY, ());
}

/// Keeps the root search current as applications are installed and removed.
fn watch_apps(app: AppHandle) {
    std::thread::spawn(move || {
        let mut known = apps::fingerprint();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3));
            let now = apps::fingerprint();
            if now != known {
                // Installers write several files; let them finish.
                std::thread::sleep(std::time::Duration::from_secs(1));
                known = apps::fingerprint();
                reload_apps(&app);
            }
        }
    });
}

/// Hotkeys bound to root search items (id → accelerator), mirrored from
/// GNOME's settings so searching never waits on `gsettings`.
#[derive(Default)]
struct ItemHotkeys(RwLock<HashMap<String, String>>);

fn with_context<T>(app: &AppHandle, f: impl FnOnce(&Index, &root::Context) -> T) -> T {
    let index = app.state::<IndexState>();
    let settings = app.state::<SettingsState>();
    let usage = app.state::<UsageStore>();
    let hotkeys = app.state::<ItemHotkeys>();
    let (settings, hotkeys) = (settings.0.read().unwrap(), hotkeys.0.read().unwrap());
    let ctx = root::Context { settings: &settings, usage: &usage, hotkeys: &hotkeys };
    let index = index.0.read().unwrap();
    f(&index, &ctx)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Has the C library give memory back to the system when a large piece of
/// it is freed.
///
/// By itself glibc stops doing so for pieces of up to 32 MB once one such
/// has been freed: it raises its "mmap threshold" to that size, and from
/// then on keeps them in a heap of the thread that freed them, where
/// nothing gives them back (`malloc_trim` leaves the end of such a heap
/// alone). A 4K screen is 31.6 MB and a screenshot holds several, on
/// several threads: after a few screenshots Heyo was 130 to 290 MB larger
/// than before, for good. A threshold that has been set is not moved:
/// every piece of 4 MB or more is then a mapping of its own, and is gone
/// when it is freed.
///
/// The price is that such a piece is new memory each time, which the
/// system hands out page by page: a screenshot of a 4K screen is up some
/// 35 ms later for it.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn give_large_memory_back() {
    // SAFETY: a setting of the allocator, made before any other thread runs.
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 4 << 20);
    }
}

pub fn run() {
    // Heyo keeps its things where the XDG directories say. Windows has no
    // such variables: there they are set, for Heyo and the helper programs
    // it starts, to the places Windows has for the same purposes.
    #[cfg(windows)]
    for (variable, place) in [("HOME", "USERPROFILE"), ("XDG_DATA_HOME", "APPDATA"), ("XDG_CONFIG_HOME", "APPDATA"), ("XDG_CACHE_HOME", "LOCALAPPDATA")] {
        if std::env::var_os(variable).is_none() {
            if let Some(path) = std::env::var_os(place) {
                std::env::set_var(variable, path);
            }
        }
    }
    // `heyo build`, `heyo dev` and the like are not the application.
    if let Some(code) = developer_tools(&std::env::args().collect::<Vec<_>>()) {
        std::process::exit(code);
    }
    #[cfg(windows)]
    platform::win::log_to_file();
    // Measuring the clipboard search needs no window and no running Heyo.
    let args: Vec<String> = std::env::args().collect();
    if let Some(at) = args.iter().position(|arg| arg == "--clipboard-eval") {
        std::process::exit(clipboard::eval::run(&args[at + 1..]));
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    give_large_memory_back();
    diagnostics::start();
    eprintln!("heyo: started");
    eprintln!("heyo: {}", renderer::chosen());
    // An optimised build made by cargo alone carries no interface: it loads
    // the dev server's, which is whatever happens to be on that port, or a
    // blank window. scripts/install.sh refuses one by these words.
    #[cfg(all(dev, not(debug_assertions)))]
    eprintln!("heyo: built without its interface (cargo build --release); it is loaded from the dev server. Build with: pnpm tauri build --no-bundle");
    tauri::Builder::default()
        // Must be registered first. Running `heyo` again (e.g. from a desktop
        // keyboard shortcut, since Wayland has no global hotkeys) toggles the
        // already-running instance instead of starting a new one.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // Run again while still starting: there is nothing to show yet.
            if !palette::exists(app) {
                return;
            }
            // Off whatever thread the desktop called on: opening an item
            // may wait on other programs.
            let app = app.clone();
            std::thread::spawn(move || run_again(&app, &args));
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(IndexState::default())
        .manage(Runtime::default())
        .manage(SettingsState(RwLock::new(Settings::load())))
        .manage(ClipboardHistory::load())
        .manage(UsageStore::load())
        .manage(ItemHotkeys::default())
        .manage(Dictation::default())
        .manage(windows::LastWindow::default())
        .manage(KeepAwake::default())
        .manage(menubar::MenuBar::default())
        .manage(runtime::Background::default())
        .manage(activities::Activities::default())
        .on_menu_event(|app, event| {
            menubar::clicked(app, event.id().as_ref());
        })
        .manage(std::sync::Arc::new(files::Files::default()))
        .invoke_handler(commands::access::guarded(tauri::generate_handler![
            search,
            launch,
            runtime_send,
            hide,
            get_settings,
            open_settings,
            set_item_hotkey,
            show_window,
            get_item,
            list_quicklinks,
            record_usage,
            set_favorite,
            move_favorite,
            set_alias,
            reset_ranking,
            read_clipboard,
            copy_text,
            open_target,
            expand::snippet_expanded,
            activities::activities,
            activities::activity_action,
            activities::activity_dismiss
        ]))
        .setup(move |app| {
            // Extension icons and assets.
            let _ = app.asset_protocol_scope().allow_directory(paths::data(), true);
            #[cfg(windows)]
            let _ = app.asset_protocol_scope().allow_directory(apps::icon_dir(), true);
            platform::clipboard_watch::attach(app.handle());
            clipboard::watch(app.handle().clone());
            links::register();
            hotkeys::start(app.handle());
            // A Heyo that was killed while dictating left the session
            // without its input method.
            std::thread::spawn(platform::composing::recover);
            dictation::watch_hold_key(app.handle());
            menubar::start(app.handle());
            updates::start(app.handle());
            expand::start(app.handle());
            // Slint's thread, ahead of the first window it draws: the
            // dictation pill is wanted at once when it is wanted. Started
            // by the first recording instead it was 40 ms later, that once.
            slint_windows::warm(app.handle());
            // And its windows follow the display when it changes.
            platform::display::watch(slint_windows::display_changed);
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let index = Index::new(
                    apps::discover(),
                    enabled_commands(&handle),
                    library::quicklinks(),
                    library::snippets(),
                );
                *handle.state::<IndexState>().0.write().unwrap() = index;
                let _ = handle.emit(crate::wire::event::INDEX_READY, ());
                watch_apps(handle.clone());
                // `heyo --open <id>` as the first instance: the item is
                // only known once the index is built.
                let args: Vec<String> = std::env::args().collect();
                if args.iter().any(|a| a == "--open") {
                    handle_args(&handle, &args);
                }
                if hotkeys::supported() {
                    *handle.state::<ItemHotkeys>().0.write().unwrap() = hotkeys::item_hotkeys();
                    let _ = handle.emit(crate::wire::event::INDEX_READY, ());
                }
            });
            match at_start(&std::env::args().collect::<Vec<_>>()) {
                AtStart::Palette => palette::show(app.handle()),
                AtStart::ClipboardHistory => palette::open_clipboard_history(app.handle()),
                AtStart::Dictation => dictation::toggle(app.handle()),
                AtStart::Nothing => {}
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == palette::WINDOW {
                return palette::on_window_event(window, event);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
