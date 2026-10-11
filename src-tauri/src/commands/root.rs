//! Commands for the palette: searching, opening items, favorites and
//! aliases, the clipboard, and messages to the extension runtime.

use tauri::{AppHandle, State, WebviewWindow};

use crate::*;

#[tauri::command]
pub(crate) fn search(query: String, app: AppHandle) -> Vec<root::Item> {
    with_context(&app, |index, ctx| index.search(&query, ctx))
}

#[tauri::command]
pub(crate) fn get_item(id: String, app: AppHandle) -> Option<root::Item> {
    with_context(&app, |index, ctx| index.get(&id, ctx))
}

#[tauri::command]
pub(crate) fn list_applications(app: AppHandle) -> Vec<root::Item> {
    with_context(&app, |index, ctx| index.applications(ctx))
}

/// Opens the item chosen in the palette: an application, a command of an
/// extension's or one of Heyo's own. (Quicklinks and snippets the palette
/// opens itself.) `arguments` are those typed in the search bar for a
/// command that takes some.
#[tauri::command(async)]
pub(crate) fn launch(id: String, fallback_text: Option<String>, arguments: Option<open::Arguments>, app: AppHandle) -> Result<(), String> {
    match open::open_with(&app, &id, fallback_text, arguments, open::Origin::Palette)? {
        true => Ok(()),
        false => Err(format!("{id} is the palette's own to open")),
    }
}

/// Quicklinks, for offering the ones that take a query as fallbacks.
#[tauri::command]
pub(crate) fn list_quicklinks(app: AppHandle) -> Vec<root::Item> {
    with_context(&app, |index, ctx| index.quicklink_items(ctx))
}

/// Counts a use of a root search item, for ranking.
#[tauri::command(async)]
pub(crate) fn record_usage(id: String, usage: State<UsageStore>) {
    usage.record(&id);
}

#[tauri::command(async)]
pub(crate) fn set_favorite(id: String, favorite: bool, app: AppHandle) -> Result<(), String> {
    update_settings(&app, |s| {
        s.favorites.retain(|f| *f != id);
        if favorite {
            s.favorites.push(id);
        }
    })
}

/// Moves a favorite up (`offset` < 0) or down.
#[tauri::command(async)]
pub(crate) fn move_favorite(id: String, offset: i32, app: AppHandle) -> Result<(), String> {
    update_settings(&app, |s| {
        let Some(from) = s.favorites.iter().position(|f| *f == id) else { return };
        let to = (from as i32 + offset).clamp(0, s.favorites.len() as i32 - 1) as usize;
        let item = s.favorites.remove(from);
        s.favorites.insert(to, item);
    })
}

/// Sets an item's alias; an empty one removes it.
#[tauri::command(async)]
pub(crate) fn set_alias(id: String, alias: String, app: AppHandle) -> Result<(), String> {
    let alias = alias.trim().to_owned();
    update_settings(&app, |s| {
        if alias.is_empty() {
            s.aliases.remove(&id);
        } else {
            s.aliases.insert(id, alias);
        }
    })
}

#[tauri::command(async)]
pub(crate) fn reset_ranking(id: String, usage: State<UsageStore>, app: AppHandle) {
    usage.reset(&id);
    let _ = app.emit(event::INDEX_READY, ());
}

#[tauri::command(async)]
pub(crate) fn read_clipboard() -> String {
    crate::platform::clipboard_watch::read_text().unwrap_or_default()
}

/// Copies text and hides the palette; with `paste`, also pastes it into the
/// window that regains focus. Several lines on their way into a terminal
/// are asked about first (`pasting`), and the palette stays for the answer.
#[tauri::command(async)]
pub(crate) fn copy_text(text: String, paste: bool, app: AppHandle) -> Result<(), String> {
    if paste {
        return pasting::copy_and_paste(&app, text);
    }
    crate::platform::clipboard_watch::write_text(&text)?;
    let _ = main_window(&app).hide();
    Ok(())
}

/// Opens a URL or path with the default application and hides the palette.
#[tauri::command]
pub(crate) fn open_target(target: String, app: AppHandle) -> Result<(), String> {
    crate::platform::common::open(&target)?;
    let _ = main_window(&app).hide();
    Ok(())
}

/// Whether a link from rendered content may be handed to the desktop: web
/// pages and mail, not files or schemes that start other programs.
pub(crate) fn is_safe_link(target: &str) -> bool {
    let target = target.trim().to_ascii_lowercase();
    ["http://", "https://", "mailto:"].iter().any(|scheme| target.starts_with(scheme))
}

/// Relays a message from the webview to the extension runtime.
// Not async: what is typed must reach the runtime in the order it was typed.
#[tauri::command]
pub(crate) fn runtime_send(message: WebviewMessage, runtime: State<Runtime>, app: AppHandle) -> Result<(), String> {
    // An answer to a question the host itself asked stops here.
    if matches!(&message, WebviewMessage::Reply { id, result, .. } if confirm::answered(*id, result.as_ref())) {
        return Ok(());
    }
    // "open" comes from links in rendered Markdown.
    if matches!(&message, WebviewMessage::Open { target } if !is_safe_link(target)) {
        return Err("only web and mail links can be opened from here".into());
    }
    runtime.send(&app, &serde_json::to_value(message).map_err(|e| e.to_string())?)
}

#[tauri::command(async)]
pub(crate) fn show_window(app: AppHandle) {
    show(&app);
}

#[tauri::command]
pub(crate) fn hide(window: WebviewWindow) {
    // A command that was just ending as the host put a question up must
    // not take the palette away with it; otherwise hiding answers No.
    if confirm::just_asked() {
        return;
    }
    confirm::dismiss();
    let _ = window.hide();
}
