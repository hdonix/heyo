//! What a `heyo …` invocation or a hotkey asks for: opening an item by
//! id, native commands, and the command-line flags.

use tauri::{AppHandle, Manager};

use crate::*;

/// Runs one of Heyo's native commands; false if `id` is not one.
pub(crate) fn open_native(app: &AppHandle, id: &str) -> bool {
    match id {
        root::SETTINGS_ID => {
            tauri::async_runtime::spawn(open_settings(app.clone()));
        }
        root::SCREENSHOT_ID => screenshot::start(app, "capture"),
        root::SCREENSHOT_TEXT_ID => screenshot::start(app, "text"),
        root::KEEP_AWAKE_ID => toggle_keep_awake(app),
        root::PASTE_PLAIN_ID => {
            let _ = main_window(app).hide();
            pasting::paste_plain();
        }
        root::CLIPBOARD_PAUSE_ID => {
            let _ = main_window(app).hide();
            clipboard::toggle_paused(app);
        }
        root::SNIPPET_EXPANSION_ID => {
            let _ = main_window(app).hide();
            expand::toggle(app);
        }
        _ => match id.strip_prefix(root::WINDOW_PREFIX) {
            Some(layout) => {
                if let Err(e) = apply_layout(app, layout) {
                    notify("Could not move the window", &e);
                }
            }
            None => return false,
        },
    }
    true
}

/// Opens an application: goes to its window if it is already running,
/// starts it otherwise. Asked again while one of its windows is in front, it
/// moves on to the application's next window — or, when it has no other,
/// back to the window used before it, so one hotkey flips between the
/// application and whatever else is being worked in.
pub(crate) fn open_app(app: &AppHandle, entry: &apps::App) -> Result<(), String> {
    let classes = entry.window_classes();
    let open = windows::of_classes(&classes);
    let Some(&front) = open.first() else {
        eprintln!("heyo: {}: no window of class {classes:?}; starting it", entry.name);
        return apps::launch(entry);
    };
    // From the palette, "in front" is the window the user came from.
    let current = windows::active_external().or(*app.state::<windows::LastWindow>().0.lock().unwrap());
    let target = if current != Some(front) {
        front
    } else if open.len() > 1 {
        // Most recently used first: the last one has waited longest.
        *open.last().unwrap_or(&front)
    } else {
        windows::before(front).unwrap_or(front)
    };
    let result = windows::activate(target);
    eprintln!(
        "heyo: {}: {} window(s), in front: {}; going to {target:#x}: {}",
        entry.name,
        open.len(),
        current == Some(front),
        result.as_ref().map_or_else(|e| e.as_str(), |()| "ok"),
    );
    result
}

/// Moves the window the user is working in into a layout.
pub(crate) fn apply_layout(app: &AppHandle, layout: &str) -> Result<(), String> {
    let (_, _, layout) = windows::LAYOUTS.iter().find(|(id, ..)| *id == layout).ok_or("unknown layout")?;
    // From the palette, the target is the window that was active before it.
    let window = working_window(app).ok_or("No window is focused.")?;
    let monitors = monitors(app)?;
    let _ = main_window(app).hide();
    windows::apply(window, *layout, &monitors)
}

/// The monitors' rectangles, in physical pixels.
pub(crate) fn monitors(app: &AppHandle) -> Result<Vec<windows::Rect>, String> {
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    Ok(monitors
        .iter()
        .map(|m| windows::Rect { x: m.position().x, y: m.position().y, width: m.size().width as i32, height: m.size().height as i32 })
        .collect())
}

/// The window the user is working in: the one in front, or the one that
/// was before the palette came up.
pub(crate) fn working_window(app: &AppHandle) -> Option<windows::Window> {
    windows::active_external().or(*app.state::<windows::LastWindow>().0.lock().unwrap())
}

/// Whether the arguments (the program's name first) ask for a developer
/// tool. Only as the first word: `heyo --open dev` opens an item.
fn for_developer_tools(args: &[String]) -> bool {
    matches!(args.get(1).map(String::as_str), Some("build" | "dev" | "migrate" | "ext"))
}

/// The developer tools: `heyo build`, `heyo dev`, `heyo migrate` and `heyo
/// ext check` are a Node.js program (`runtime/src/dev/`), run here in the
/// terminal the command was typed in, with no window and whether or not
/// Heyo is running. Returns its exit code, or `None` if `args` ask for
/// something else.
pub(crate) fn developer_tools(args: &[String]) -> Option<i32> {
    if !for_developer_tools(args) {
        return None;
    }
    let mut command = match crate::helper::node_in_terminal("heyo-dev.cjs", "HEYO_DEV") {
        Ok(command) => command,
        Err(e) => {
            eprintln!("heyo: {e}");
            return Some(1);
        }
    };
    command.args(&args[1..]).envs(crate::paths::for_runtime());
    // For telling the running Heyo about a build (`--dev-reload`, below).
    if let Ok(exe) = std::env::current_exe() {
        command.env("HEYO_BIN", exe);
    }
    let node = command.get_program().to_string_lossy().into_owned();
    let failed = |e: std::io::Error| {
        eprintln!("heyo: could not start Node.js ({node}): {e}. The developer tools need Node.js 22 or newer; set its path in Heyo Settings.");
        Some(1)
    };
    // In place of this process: Ctrl+C then reaches the tool and it alone.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        failed(command.exec())
    }
    #[cfg(not(unix))]
    {
        #[cfg(windows)]
        use_parent_console(&mut command);
        match command.status() {
            Ok(status) => Some(status.code().unwrap_or(1)),
            Err(e) => failed(e),
        }
    }
}

/// Gives a console program started from here the console Heyo itself was
/// started from. A windowed program has none of its own, and so no
/// standard input or output to hand on, unless they were redirected.
#[cfg(windows)]
fn use_parent_console(command: &mut std::process::Command) {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    let missing = |which| {
        let handle = unsafe { GetStdHandle(which) };
        handle.is_null() || handle == INVALID_HANDLE_VALUE
    };
    let console = |name: &str, write: bool| std::fs::OpenOptions::new().read(!write).write(write).open(name).ok();
    if missing(STD_INPUT_HANDLE) {
        if let Some(input) = console("CONIN$", false) {
            command.stdin(input);
        }
    }
    if missing(STD_OUTPUT_HANDLE) {
        if let Some(output) = console("CONOUT$", true) {
            command.stdout(output);
        }
    }
    if missing(STD_ERROR_HANDLE) {
        if let Some(output) = console("CONOUT$", true) {
            command.stderr(output);
        }
    }
}

/// Handles command-line arguments, from this process or a second instance.
/// Returns whether they asked for something other than toggling the window.
pub(crate) fn handle_args(app: &AppHandle, args: &[String]) -> bool {
    if let Some(i) = args.iter().position(|a| a == "--open") {
        match args.get(i + 1) {
            Some(id) => open_item(app, id),
            None => eprintln!("heyo: --open needs an item id"),
        }
        return true;
    }
    if let Some(i) = args.iter().position(|a| a == "--link") {
        // A sign-in started by an extension came back from the browser.
        match args.get(i + 1).filter(|link| links::is_oauth(link)) {
            Some(link) => {
                // Either runtime may have asked: a menu bar command signs in
                // too. The state in the link tells each whether it is its own.
                if let Err(e) = crate::runtime::tell_running(app, &HostMessage::OauthCallback { url: link.clone() }.json()) {
                    eprintln!("heyo: {e}");
                }
            }
            None => eprintln!("heyo: --link: not a link Heyo handles"),
        }
        return true;
    }
    if args.iter().any(|a| a == "--clipboard") {
        open_clipboard_history(app);
        return true;
    }
    if let Some(i) = args.iter().position(|a| a == "--dev-reload") {
        // `heyo dev` built an extension, added one or took one out.
        reload_commands(app);
        if let Some(dir) = args.get(i + 1) {
            // Whichever runtime has a command of it running starts it again.
            let _ = crate::runtime::tell_running(app, &HostMessage::DevReload { extension: dir.clone() }.json());
        }
        return true;
    }
    if args.iter().any(|a| a == "--dictate") {
        dictation::toggle(app);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_developer_tools_are_asked_for_by_the_first_word() {
        let args = |line: &str| line.split(' ').map(str::to_owned).collect::<Vec<_>>();
        for line in ["heyo build", "heyo dev .", "heyo dev --remove x", "heyo migrate --dry-run", "heyo ext check --json"] {
            assert!(for_developer_tools(&args(line)), "{line}");
        }
        // Everything Heyo was started with before stays the application's.
        for line in ["heyo", "heyo --hidden", "heyo --open dev", "heyo --hidden --dev-reload /data/development/x", "heyo --clipboard", "heyo check"] {
            assert!(!for_developer_tools(&args(line)), "{line}");
        }
    }
}
