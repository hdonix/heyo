//! Desktop services that work the same on every session type:
//! notifications and keeping the computer awake.

use tauri::{AppHandle, Manager};

use crate::*;

/// A command that runs without a console window of its own. (On Windows a
/// console program started by a windowed one would otherwise flash one up.)
pub(crate) fn quiet(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    #[allow(unused_mut)]
    let mut command = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Opens a file, folder or URL with whatever the desktop opens it with.
pub(crate) fn open(target: impl AsRef<std::ffi::OsStr>) -> Result<(), String> {
    let target = target.as_ref();
    let mut command = if cfg!(windows) {
        let mut command = quiet("cmd");
        // The empty string is the window title `start` takes its first quoted argument for.
        command.args(["/C", "start", ""]);
        command
    } else {
        quiet("xdg-open")
    };
    // What opens outlives Heyo and is not Heyo's to log.
    command.arg(target).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    let mut child = command.spawn().map_err(|e| format!("failed to open {}: {e}", target.to_string_lossy()))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// A short desktop notification.
pub(crate) fn notify(title: &str, body: &str) {
    let spawned = std::process::Command::new("notify-send")
        .args(["--app-name=Heyo", "--expire-time=2500", "--hint=int:transient:1", "--", title, body])
        .spawn();
    if let Ok(mut child) = spawned {
        std::thread::spawn(move || child.wait());
    }
}

/// The `systemd-inhibit` process that keeps the computer awake, while on.
#[derive(Default)]
pub(crate) struct KeepAwake(std::sync::Mutex<Option<std::process::Child>>);

pub(crate) fn toggle_keep_awake(app: &AppHandle) {
    let state = app.state::<KeepAwake>();
    let mut inhibitor = state.0.lock().unwrap();
    let _ = main_window(app).hide();
    if let Some(mut child) = inhibitor.take() {
        let _ = child.kill();
        let _ = child.wait();
        return notify("Keep Awake is off", "The computer can sleep again.");
    }
    let spawned = std::process::Command::new("systemd-inhibit")
        // `tail --pid` ends with Heyo, so the inhibitor never outlives it.
        .args(["--what=idle:sleep", "--who=Heyo", "--why=Keep Awake is on", "tail", "-f", "/dev/null"])
        .arg(format!("--pid={}", std::process::id()))
        .spawn();
    match spawned {
        Ok(child) => {
            *inhibitor = Some(child);
            notify("Keep Awake is on", "The computer will not sleep or dim until you turn this off.");
        }
        Err(e) => notify("Could not keep the computer awake", &e.to_string()),
    }
}
