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

use std::collections::HashMap;
use std::sync::RwLock;

use tauri::{AppHandle, Emitter, Manager, WindowEvent};

use cli::*;
use open::open_item;
use clipboard::ClipboardHistory;
use commands::{media::*, root::*, settings::*};
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
            change_settings,
            set_anthropic_api_key,
            open_settings,
            open_extensions_folder,
            list_extensions,
            set_extension_preferences,
            uninstall_extension,
            clear_clipboard_history,
            get_hotkeys,
            set_hotkey,
            set_item_hotkey,
            item_hotkeys,
            screenshot_finish,
            ocr_status,
            ocr_install,
            dictation_status,
            dictation_install,
            dictation_toggle,
            dictation_cancel,
            show_window,
            get_item,
            list_applications,
            list_quicklinks,
            get_capabilities,
            install_shell_extension,
            local_models,
            record_usage,
            set_favorite,
            move_favorite,
            set_alias,
            reset_ranking,
            read_clipboard,
            copy_text,
            open_target,
            clipboard::clipboard_sources,
            clipboard::clipboard_meaning_status,
            clipboard::clipboard_meaning_install,
            updates::check_for_update,
            updates::install_update,
            copy_diagnostics,
            permissions::extension_permissions,
            permissions::revoke_extension_permission,
            expand::snippet_expanded,
            activities::activities,
            activities::activity_action,
            activities::activity_dismiss
        ]))
        .setup(move |app| {
            // Extension icons and assets.
            let _ = app.asset_protocol_scope().allow_directory(paths::data(), true);
            let _ = app.asset_protocol_scope().allow_directory(screenshot::asset_dir(), true);
            #[cfg(windows)]
            let _ = app.asset_protocol_scope().allow_directory(apps::icon_dir(), true);
            platform::clipboard_watch::attach(app.handle());
            clipboard::watch(app.handle().clone());
            links::register();
            hotkeys::start(app.handle());
            dictation::watch_hold_key(app.handle());
            menubar::start(app.handle());
            updates::start(app.handle());
            expand::start(app.handle());
            // The dictation window must never take keyboard focus from the
            // application being dictated into.
            #[cfg(target_os = "linux")]
            if let Some(window) = app.get_webview_window(dictation::WINDOW) {
                use gtk::prelude::GtkWindowExt;
                if let Ok(gtk_window) = window.gtk_window() {
                    gtk_window.set_accept_focus(false);
                    gtk_window.set_focus_on_map(false);
                }
            }
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
            // A pinned capture is deleted with its window.
            if let (WindowEvent::Destroyed, Some(n)) = (event, window.label().strip_prefix("pin-")) {
                screenshot::unpinned(n);
            }
            // The dictation window and the screenshot overlay live for as
            // long as Heyo does, as the palette: closing one (Alt+F4) only
            // hides it.
            if let WindowEvent::CloseRequested { api, .. } = event {
                match window.label() {
                    dictation::WINDOW => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    screenshot::WINDOW => {
                        api.prevent_close();
                        screenshot::cancel(window.app_handle());
                    }
                    _ => {}
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
