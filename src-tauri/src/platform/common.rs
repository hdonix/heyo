//! Desktop services that work the same on every session type:
//! notifications and keeping the computer awake.

use crate::i18n::{t, tf};
use tauri::{AppHandle, Manager};

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
    } else if cfg!(target_os = "macos") {
        quiet("open")
    } else {
        quiet("xdg-open")
    };
    // What opens outlives Heyo and is not Heyo's to log.
    command.arg(target).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    let mut child = command.spawn().map_err(|e| tf("failed to open {0}: {1}", &[&target.to_string_lossy(), &e]))?;
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

/// A notification that takes the place of an earlier one (`previous`, as
/// returned before) rather than piling up beside it: a live activity has
/// one notification, with its latest state. Answers with the id to replace
/// it by next time; none where the desktop does not say.
pub(crate) fn notify_replacing(previous: Option<u32>, title: &str, body: &str) -> Option<u32> {
    let mut command = std::process::Command::new("notify-send");
    command.args(["--app-name=Heyo", "--print-id", "--hint=int:transient:1"]);
    if let Some(id) = previous {
        command.arg(format!("--replace-id={id}"));
    }
    let output = command.args(["--", title, body]).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

/// The `systemd-inhibit` process that keeps the computer awake, while on.
#[derive(Default)]
pub(crate) struct KeepAwake(std::sync::Mutex<Option<std::process::Child>>);

pub(crate) fn toggle_keep_awake(app: &AppHandle) {
    let state = app.state::<KeepAwake>();
    let mut inhibitor = state.0.lock().unwrap();
    if let Some(mut child) = inhibitor.take() {
        let _ = child.kill();
        let _ = child.wait();
        return notify(t("Keep Awake is off"), t("The computer can sleep again."));
    }
    let spawned = std::process::Command::new("systemd-inhibit")
        // `tail --pid` ends with Heyo, so the inhibitor never outlives it.
        .args(["--what=idle:sleep", "--who=Heyo", "--why=Keep Awake is on", "tail", "-f", "/dev/null"])
        .arg(format!("--pid={}", std::process::id()))
        .spawn();
    match spawned {
        Ok(child) => {
            *inhibitor = Some(child);
            notify(t("Keep Awake is on"), t("The computer will not sleep or dim until you turn this off."));
        }
        Err(e) => notify(t("Could not keep the computer awake"), &e.to_string()),
    }
}
