//! What the extension runtime asks of the host for itself: its `host.*`
//! requests. What a command asks for through `@heyo/api`, one of Heyo's own
//! or an extension's, is answered in `ext.rs`.

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::wire::HostRequest;
use crate::*;

/// An application as extensions know one.
pub(crate) fn application(entry: &apps::App) -> Value {
    json!({ "name": entry.name, "localizedName": entry.name, "path": entry.path, "bundleId": entry.id, "icon": entry.icon })
}

fn value(result: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(result).map_err(|e| e.to_string())
}

/// Answers a request of the extension runtime.
pub(crate) fn host_call(app: &AppHandle, request: HostRequest) -> Result<Value, String> {
    use HostRequest::*;
    match request {
        Hide => palette::hide(app),
        Show => palette::bring_back(app),
        ClipboardWrite { text, html, concealed } => {
            if concealed.unwrap_or(false) {
                clipboard::skip_next();
            }
            match html {
                Some(html) => crate::platform::clipboard_watch::write_html(&html, &text)?,
                None => crate::platform::clipboard_watch::write_text(&text)?,
            }
        }
        ClipboardRead => return Ok(crate::platform::clipboard_watch::read_text().unwrap_or_default().into()),
        Reload => reload_commands(app),
        Applications => {
            let index = app.state::<IndexState>();
            let index = index.0.read().unwrap();
            return Ok(index.apps.iter().map(application).collect());
        }
        Paste => pasting::paste_clipboard(app),
        Selection => return Ok(selected_text().into()),
        FilesRefresh => app.state::<std::sync::Arc<files::Files>>().refresh(),
        // Answered over time, where the runtime's messages are read (`ai.rs`).
        AiAsk(_) | Cancel { .. } => return Err("the host answers this elsewhere".into()),
        Hud { text } => notify(&text, ""),
        Open { target, application } => platform::system::open(&target, application.as_deref())?,
        Trash { paths } => platform::system::trash(&paths)?,
        DefaultApplication { target } => return value(platform::system::default_application(&target)),
        ClipboardHistoryStatus => return value(clipboard::status(app)),
        ClipboardHistoryChose { id, query, rank } => app.state::<ClipboardHistory>().chose(&query, id, rank),
        ClipboardHistoryPin { id, pinned } => app.state::<ClipboardHistory>().set_pinned(id, pinned),
        ClipboardHistoryDelete { id } => app.state::<ClipboardHistory>().delete(id),
        ClipboardHistoryClear => app.state::<ClipboardHistory>().clear(),
        ClipboardHistoryPause { paused } => clipboard::set_paused(app, paused)?,
        LaunchCommand { extension, command, background, arguments, context, fallback_text } => {
            menubar::launch_command(app, &extension, &command, background, arguments, context, fallback_text)?
        }
        Processes => return value(platform::system::processes()?),
        ProcessEnd { pid, force } => platform::system::end_process(pid, force)?,
    }
    Ok(Value::Null)
}
