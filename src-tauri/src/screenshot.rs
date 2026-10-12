//! Screenshots: capture the screen, let the user select and annotate a
//! region in a fullscreen overlay, then copy it, save it, pin it on screen or
//! extract its text.
//!
//! The screen is captured before the overlay appears and shown inside it, so
//! the selection is made on a frozen image. The webview crops and draws
//! annotations, and sends back the finished PNG.
//!
//! The frozen screen is painted by the overlay window itself, natively,
//! straight from the captured pixels (`platform::overlay`); the webview on
//! top of it is transparent and draws only the dimming, the selection and
//! the tools.
//! The overlay can therefore appear as soon as the screen has been read,
//! without waiting for the webview to load a whole-screen image. The webview
//! still gets the capture as a file, in the background, to crop from.

use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::platform::capture::{grab, Area, Shot};
use crate::platform::common::notify;
use crate::platform::overlay;
use crate::platform::windows::Rect;

pub(crate) mod selecting;

pub const WINDOW: &str = "screenshot";

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Whether the overlay is the one Slint draws (`slint_windows::screenshot`)
/// and not the web view's. Until the web view's is removed, it is so where
/// `HEYO_SCREENSHOT` says `slint`.
pub(crate) fn drawn_by_slint() -> bool {
    static ASKED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ASKED.get_or_init(|| std::env::var("HEYO_SCREENSHOT").is_ok_and(|by| by == "slint"))
}

/// For measuring how soon the overlay is up: with `HEYO_OVERLAY_DEBUG`
/// set, says what happened, when by the system's clock, and how long after
/// the screenshot was asked for.
pub(crate) fn mark(what: &str, asked: std::time::Instant) {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *WANTED.get_or_init(|| std::env::var_os("HEYO_OVERLAY_DEBUG").is_some()) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_millis());
        eprintln!("heyo: {what} at {now} ms, {:.1} ms after it was asked for", asked.elapsed().as_secs_f64() * 1e3);
    }
}

/// The monitor that the overlay Slint draws is showing, as it was read:
/// what is selected is cut from it here.
struct Frozen {
    shot: Shot,
    area: Area,
}

static FROZEN: std::sync::Mutex<Option<std::sync::Arc<Frozen>>> = std::sync::Mutex::new(None);
fn cache_dir() -> PathBuf {
    crate::paths::cache().join("screenshot")
}

/// Directory the webview may load captures from.
pub fn asset_dir() -> PathBuf {
    cache_dir()
}

/// Writes the capture uncompressed, for the webview to crop from.
/// Compressing a whole screen takes a noticeable fraction of a second; the
/// file is read back once and deleted.
fn write_bmp(shot: &Shot, path: &PathBuf) -> Result<(), String> {
    use std::io::Write;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let (width, height) = (shot.width as usize, shot.height as usize);
    // 24-bit rows, bottom-up, each padded to four bytes.
    let stride = (width * 3).div_ceil(4) * 4;
    let mut out = Vec::with_capacity(54 + stride * height);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + stride * height) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for y in (0..height).rev() {
        let row = &shot.bgrx[y * width * 4..(y + 1) * width * 4];
        for pixel in row.chunks_exact(4) {
            out.extend_from_slice(&pixel[..3]);
        }
        out.resize(out.len() + stride - width * 3, 0);
    }
    std::fs::File::create(path).and_then(|mut file| file.write_all(&out)).map_err(|e| e.to_string())
}

/// An extension waiting for the area the user selects
/// (`Screen.capture("selection")`): the overlay then shows no tools, and the
/// selection goes into `path` instead of onto the clipboard.
struct Pick {
    path: PathBuf,
    done: std::sync::mpsc::Sender<Picked>,
}

/// How a selection for an extension ended.
#[derive(Debug, PartialEq)]
pub(crate) enum Picked {
    Saved { width: u32, height: u32 },
    Cancelled,
    Failed(String),
}

static PICK: std::sync::Mutex<Option<Pick>> = std::sync::Mutex::new(None);

/// Tells whoever waits for a selection how it ended.
fn end_pick(outcome: Picked) {
    let pick = PICK.lock().unwrap().take();
    if let Some(pick) = pick {
        let _ = pick.done.send(outcome);
    }
}

/// Writes the selected area, a PNG as the overlay made it, where it was asked for.
fn save_pick(png: &[u8], path: &std::path::Path) -> Picked {
    let saved = (|| {
        let (width, height) = image::ImageReader::with_format(Cursor::new(png), image::ImageFormat::Png).into_dimensions().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(path.parent().ok_or("the capture has no directory")?).map_err(|e| e.to_string())?;
        std::fs::write(path, png).map_err(|e| format!("failed to save {}: {e}", path.display()))?;
        Ok::<_, String>(Picked::Saved { width, height })
    })();
    saved.unwrap_or_else(Picked::Failed)
}

/// Has the user select an area of the screen, and saves it as `path`.
/// Returns when they have, or have given up.
pub(crate) fn pick(app: &AppHandle, path: PathBuf) -> Picked {
    let (done, picked) = std::sync::mpsc::channel();
    // One selection at a time: whoever waited for the last hears it is off.
    let previous = PICK.lock().unwrap().replace(Pick { path, done });
    if let Some(previous) = previous {
        let _ = previous.done.send(Picked::Cancelled);
    }
    start(app, "pick");
    // An overlay that went away without a word must not be waited for for ever.
    picked.recv_timeout(Duration::from_secs(600)).unwrap_or(Picked::Cancelled)
}

/// The monitor the pointer is on, else the primary one.
pub(crate) fn current_monitor(app: &AppHandle) -> Result<Area, String> {
    let cursor = app.cursor_position().ok();
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    // The desktop's word first, where the toolkit cannot follow the pointer.
    let pointed = crate::platform::desktop::desktop().pointer_monitor();
    let monitor = pointed
        .and_then(|origin| monitors.iter().find(|m| (m.position().x, m.position().y) == origin))
        .or_else(|| {
            let c = cursor?;
            monitors.iter().find(|m| {
                let (p, s) = (m.position(), m.size());
                c.x >= p.x as f64 && c.x < (p.x + s.width as i32) as f64 && c.y >= p.y as f64 && c.y < (p.y + s.height as i32) as f64
            })
        })
        .cloned()
        .or(app.primary_monitor().map_err(|e| e.to_string())?)
        .ok_or("no monitor found")?;
    let (position, size) = (monitor.position(), monitor.size());
    Ok(Area { x: position.x, y: position.y, width: size.width, height: size.height, scale: monitor.scale_factor() })
}

/// The frozen screen, as the overlay is given it (`SCREENSHOT`).
#[derive(Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "ScreenshotCapture")]
pub(crate) struct Capture {
    path: String,
    /// `capture` shows the toolbar; `text` extracts text as soon as a region
    /// is selected; `pick` is an extension asking for the area, and selecting
    /// it is all there is to do.
    #[ts(type = "\"capture\" | \"text\" | \"pick\"")]
    mode: String,
    /// Screen position of the overlay, in physical pixels.
    x: i32,
    y: i32,
    scale: f64,
    /// Size of the capture, in physical pixels: what the overlay's own
    /// width and height stand for.
    width: u32,
    height: u32,
    /// The windows in the capture, the topmost first, for selecting one with
    /// a click: in physical pixels from the capture's top left corner.
    windows: Vec<Rect>,
}

/// A window with less than this showing, either way, is not worth offering.
const MIN_WINDOW: i32 = 16;

/// Where windows are in a capture of `area`: their frames (on the screen, in
/// physical pixels, the topmost first) moved to the capture's own corner and
/// cut to what of them is on this monitor. The order is kept.
fn windows_in(area: Area, frames: &[Rect]) -> Vec<Rect> {
    let (width, height) = (area.width as i32, area.height as i32);
    frames
        .iter()
        .filter_map(|frame| {
            let (left, top) = ((frame.x - area.x).max(0), (frame.y - area.y).max(0));
            let right = (frame.x + frame.width - area.x).min(width);
            let bottom = (frame.y + frame.height - area.y).min(height);
            (right - left >= MIN_WINDOW && bottom - top >= MIN_WINDOW).then(|| Rect { x: left, y: top, width: right - left, height: bottom - top })
        })
        .collect()
}

/// Rectangles found in a monitor's area as the toolkit sizes it, for a
/// capture `width` pixels wide. The two differ under fractional scaling on
/// Wayland, where the toolkit counts more pixels than the monitor has.
fn fitted(rects: Vec<Rect>, area: Area, width: u32) -> Vec<Rect> {
    if area.width == width || area.width == 0 {
        return rects;
    }
    let fit = |v: i32| (v as f64 * width as f64 / area.width as f64).round() as i32;
    rects.into_iter().map(|r| Rect { x: fit(r.x), y: fit(r.y), width: fit(r.width), height: fit(r.height) }).collect()
}

/// The frames of other applications' windows that are on screen, the topmost
/// first. Heyo's own are never among them. None where they cannot be listed:
/// the selection is then made by dragging only.
fn window_frames() -> Vec<Rect> {
    crate::platform::windows::shown().unwrap_or_default().into_iter().filter_map(|window| window.bounds).collect()
}

/// Captures the screen and opens the selection overlay.
pub fn start(app: &AppHandle, mode: &str) {
    let asked = std::time::Instant::now();
    mark("screenshot asked for", asked);
    let app = app.clone();
    let mode = mode.to_owned();
    let picking = mode == "pick";
    // A capture of the user's own takes the overlay from an extension's.
    if !picking {
        end_pick(Picked::Cancelled);
    }
    std::thread::spawn(move || {
        // Neither the palette nor a previous overlay may end up in the capture.
        crate::palette::hide_until_gone(&app);
        if drawn_by_slint() {
            if crate::slint_windows::screenshot::up() {
                close_overlay(&app);
                std::thread::sleep(Duration::from_millis(60));
            }
        } else if app.get_webview_window(WINDOW).is_some_and(|w| w.is_visible().unwrap_or(false)) {
            close_overlay(&app);
            std::thread::sleep(Duration::from_millis(180));
        }
        let result = (|| {
            let monitor = current_monitor(&app)?;
            // Asked for meanwhile: the overlay does not wait for it to appear.
            let frames = std::thread::spawn(window_frames);
            let shot = grab(monitor)?;
            mark("screen read", asked);
            let area = Area { width: shot.width, height: shot.height, ..monitor };
            if drawn_by_slint() {
                let mode = match mode.as_str() {
                    "text" => selecting::Mode::Text,
                    "pick" => selecting::Mode::Pick,
                    _ => selecting::Mode::Capture,
                };
                let frozen = std::sync::Arc::new(Frozen { shot, area });
                *FROZEN.lock().unwrap() = Some(frozen.clone());
                if !crate::slint_windows::screenshot::show(&app, area, &frozen.shot, mode, asked) {
                    *FROZEN.lock().unwrap() = None;
                    return Err(format!("the overlay cannot be shown: {}", crate::slint_windows::failure()));
                }
                let windows = fitted(windows_in(monitor, &frames.join().unwrap_or_default()), monitor, area.width);
                let windows = windows.into_iter().map(|w| selecting::Area { x: w.x.max(0) as u32, y: w.y.max(0) as u32, width: w.width.max(0) as u32, height: w.height.max(0) as u32 });
                crate::slint_windows::screenshot::windows(windows.collect());
                return Ok(());
            }
            // A new name each time, so the webview never shows a stale capture.
            let path = cache_dir().join(format!("capture-{}.bmp", COUNTER.fetch_add(1, Ordering::Relaxed)));
            if let Ok(entries) = std::fs::read_dir(cache_dir()) {
                for entry in entries.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("capture-")) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
            // The window shows the frozen screen at once; the webview draws
            // its tools over it and loads the file in the background.
            overlay::show(&app, WINDOW, area, shot.bgrx.clone());
            write_bmp(&shot, &path)?;
            let window = app.get_webview_window(WINDOW).ok_or("screenshot window missing")?;
            let capture = Capture {
                path: path.to_string_lossy().into_owned(),
                mode,
                x: area.x,
                y: area.y,
                scale: area.scale,
                width: area.width,
                height: area.height,
                windows: fitted(windows_in(monitor, &frames.join().unwrap_or_default()), monitor, shot.width),
            };
            window.emit(crate::wire::event::SCREENSHOT, capture).map_err(|e| e.to_string())?;
            Ok::<(), String>(())
        })();
        if let Err(e) = result {
            eprintln!("heyo: screenshot failed: {e}");
            if picking {
                // The extension that asked says so in its own way.
                return end_pick(Picked::Failed(e));
            }
            notify("Screenshot failed", &e);
        }
    });
}

/// A pinned capture's window closed: its image is no longer needed.
pub fn unpinned(n: &str) {
    let _ = std::fs::remove_file(cache_dir().join(format!("pin-{n}.png")));
}

/// Closes the overlay without doing anything with the capture.
pub fn cancel(app: &AppHandle) {
    close_overlay(app);
    end_pick(Picked::Cancelled);
}

fn close_overlay(app: &AppHandle) {
    if drawn_by_slint() {
        crate::slint_windows::screenshot::hide();
        *FROZEN.lock().unwrap() = None;
    } else {
        overlay::hide(app, WINDOW);
    }
    // The capture shows the whole screen; do not keep it around.
    if let Ok(entries) = std::fs::read_dir(cache_dir()) {
        for entry in entries.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("capture-")) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn pictures_dir() -> PathBuf {
    let home = crate::paths::home();
    let pictures = Command::new("xdg-user-dir")
        .arg("PICTURES")
        .output()
        .ok()
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
        // Without a configured pictures folder, xdg-user-dir answers with the home directory.
        .filter(|p| p.is_absolute() && *p != home)
        .unwrap_or_else(|| home.join("Pictures"));
    pictures.join("Screenshots")
}

fn timestamp() -> String {
    Command::new("date")
        .arg("+%Y-%m-%d %H-%M-%S")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| COUNTER.fetch_add(1, Ordering::Relaxed).to_string())
}

/// What the user chose to do with the selected region. Cancelling needs
/// nothing but the action.
#[derive(Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ScreenshotFinish")]
pub struct Finish {
    /// `pick` is for the extension that asked for the selection.
    #[ts(type = "\"copy\" | \"save\" | \"pin\" | \"text\" | \"pick\" | \"cancel\"")]
    action: String,
    /// The cropped, annotated region as base64 PNG.
    #[serde(default)]
    #[ts(optional, as = "Option<String>")]
    png: String,
    /// Selection in logical pixels, relative to the overlay.
    #[serde(default)]
    #[ts(optional, as = "Option<f64>")]
    x: f64,
    #[serde(default)]
    #[ts(optional, as = "Option<f64>")]
    y: f64,
    #[serde(default)]
    #[ts(optional, as = "Option<f64>")]
    width: f64,
    #[serde(default)]
    #[ts(optional, as = "Option<f64>")]
    height: f64,
    /// Screen position of the overlay in physical pixels, and its scale.
    #[serde(default)]
    #[ts(optional, as = "Option<i32>")]
    origin_x: i32,
    #[serde(default)]
    #[ts(optional, as = "Option<i32>")]
    origin_y: i32,
    #[serde(default = "one")]
    #[ts(optional, as = "Option<f64>")]
    scale: f64,
}

fn one() -> f64 {
    1.0
}

/// The selected part of a capture, as it arrives to be finished: a PNG from
/// the page, or pixels cut from the capture by an overlay the host draws.
enum Selected {
    Png(Vec<u8>),
    Pixels(image::RgbaImage),
}

impl Selected {
    fn png(&self) -> Result<std::borrow::Cow<'_, [u8]>, String> {
        match self {
            Selected::Png(bytes) => Ok(bytes.into()),
            Selected::Pixels(pixels) => {
                let mut png = Vec::new();
                pixels.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
                Ok(png.into())
            }
        }
    }

    fn pixels(self) -> Result<image::RgbaImage, String> {
        match self {
            Selected::Png(bytes) => Ok(image::load(Cursor::new(&bytes), image::ImageFormat::Png).map_err(|e| e.to_string())?.into_rgba8()),
            Selected::Pixels(pixels) => Ok(pixels),
        }
    }
}

/// Where the selected part is on the screen, in logical pixels, and the
/// scale of the monitor it is on.
struct Place {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
}

/// Part of a capture (`Shot::bgrx`, `width` pixels wide) as a picture, cut
/// to what the capture has.
fn cut(bgrx: &[u8], width: u32, area: selecting::Area) -> image::RgbaImage {
    let height = (bgrx.len() / 4).checked_div(width as usize).unwrap_or(0) as u32;
    let (x, y) = (area.x.min(width), area.y.min(height));
    let (across, down) = (area.width.min(width - x), area.height.min(height - y));
    let mut rgba = Vec::with_capacity((across * down * 4) as usize);
    for row in y..y + down {
        let from = ((row * width + x) * 4) as usize;
        for pixel in bgrx[from..from + across as usize * 4].chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
    image::RgbaImage::from_raw(across, down, rgba).expect("as many bytes as the picture has pixels")
}

/// What the overlay Slint draws was told to do with the capture: does it.
/// What cannot be done is said in the overlay where that is still up, and
/// in a notification where it is not.
pub(crate) fn decided(app: &AppHandle, outcome: selecting::Outcome) {
    let selecting::Outcome::Finish { action, area } = outcome else { return cancel(app) };
    let Some(frozen) = FROZEN.lock().unwrap().clone() else { return };
    let action = match action {
        selecting::Action::Copy => "copy",
        selecting::Action::Save => "save",
        selecting::Action::Text => "text",
        selecting::Action::Pick => "pick",
    };
    let scale = if frozen.area.scale > 0.0 { frozen.area.scale } else { 1.0 };
    // A pinned image sits exactly where it was captured.
    let place = Place {
        x: (frozen.area.x as f64 + area.x as f64) / scale,
        y: (frozen.area.y as f64 + area.y as f64) / scale,
        width: area.width as f64 / scale,
        height: area.height as f64 / scale,
        scale,
    };
    let selected = Selected::Pixels(cut(&frozen.shot.bgrx, frozen.shot.width, area));
    if let Err(e) = tauri::async_runtime::block_on(complete(app.clone(), action, selected, place)) {
        eprintln!("heyo: screenshot failed: {e}");
        if crate::slint_windows::screenshot::up() {
            crate::slint_windows::screenshot::failed(e);
        } else if action == "pick" {
            end_pick(Picked::Failed(e));
        } else {
            notify("Screenshot failed", &e);
        }
    }
}

/// Completes a capture. Returns the recognized text for `text`.
pub async fn finish(app: AppHandle, request: Finish) -> Result<String, String> {
    if request.action == "cancel" {
        close_overlay(&app);
        end_pick(Picked::Cancelled);
        return Ok(String::new());
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(&request.png).map_err(|e| e.to_string())?;
    // A pinned image sits exactly where it was captured.
    let place = Place {
        x: request.origin_x as f64 / request.scale + request.x,
        y: request.origin_y as f64 / request.scale + request.y,
        width: request.width,
        height: request.height,
        scale: request.scale,
    };
    complete(app, &request.action, Selected::Png(bytes), place).await
}

/// Does with the selected part what was chosen. Returns the recognized text
/// for `text`.
async fn complete(app: AppHandle, action: &str, selected: Selected, place: Place) -> Result<String, String> {
    // Whatever else became of the selection, it is not the extension's.
    if action != "pick" {
        end_pick(Picked::Cancelled);
    }
    match action {
        "pick" => {
            let png = selected.png();
            close_overlay(&app);
            let pick = PICK.lock().unwrap().take().ok_or("nothing is waiting for this selection")?;
            let _ = pick.done.send(png.map_or_else(Picked::Failed, |png| save_pick(&png, &pick.path)));
            Ok(String::new())
        }
        "copy" => {
            let image = selected.pixels()?;
            let (width, height) = image.dimensions();
            close_overlay(&app);
            app.clipboard()
                .write_image(&tauri::image::Image::new_owned(image.into_raw(), width, height))
                .map_err(|e| e.to_string())?;
            notify("Screenshot copied", "The image is on the clipboard.");
            Ok(String::new())
        }
        "save" => {
            let dir = pictures_dir();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let path = dir.join(format!("Screenshot {}.png", timestamp()));
            std::fs::write(&path, selected.png()?).map_err(|e| format!("failed to save {}: {e}", path.display()))?;
            close_overlay(&app);
            notify("Screenshot saved", &path.to_string_lossy());
            Ok(path.to_string_lossy().into_owned())
        }
        "pin" => {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = cache_dir().join(format!("pin-{n}.png"));
            std::fs::write(&path, selected.png()?).map_err(|e| e.to_string())?;
            close_overlay(&app);
            let script = format!(
                "window.__HEYO_VIEW__ = 'pin'; window.__HEYO_PIN__ = {};",
                serde_json::to_string(&path.to_string_lossy()).map_err(|e| e.to_string())?
            );
            WebviewWindowBuilder::new(&app, format!("pin-{n}"), WebviewUrl::default())
                .title("Heyo Pinned Screenshot")
                .initialization_script(&script)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .shadow(true)
                .inner_size(place.width.max(24.0), place.height.max(24.0))
                .position(place.x, place.y)
                .build()
                .map_err(|e| e.to_string())?;
            Ok(String::new())
        }
        "text" => {
            let path = cache_dir().join("text.png");
            std::fs::create_dir_all(cache_dir()).map_err(|e| e.to_string())?;
            // Small interface text is recognized much better enlarged.
            let image = image::DynamicImage::ImageRgba8(selected.pixels()?);
            let factor = if place.scale < 1.5 { 2 } else { 1 };
            image
                .resize(image.width() * factor, image.height() * factor, image::imageops::FilterType::CatmullRom)
                .save(&path)
                .map_err(|e| e.to_string())?;
            let worker = app.clone();
            let recognized = {
                let path = path.clone();
                tauri::async_runtime::spawn_blocking(move || crate::ocr::recognize(&worker, &path)).await
            };
            // The selection may show anything; it is not kept once read.
            let _ = std::fs::remove_file(&path);
            let text = recognized.map_err(|e| e.to_string())??;
            if text.trim().is_empty() {
                return Err("No text was found in the selection.".into());
            }
            close_overlay(&app);
            crate::platform::clipboard_watch::write_text(&text)?;
            let preview: String = text.chars().take(80).collect();
            notify("Text copied", &preview);
            Ok(text)
        }
        other => Err(format!("unknown screenshot action {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
        Rect { x, y, width, height }
    }

    #[test]
    fn what_the_overlay_decided_is_read_as_it_sends_it() {
        let sent = serde_json::json!({ "action": "pin", "png": "iVBOR", "x": 12.5, "y": 40.0, "width": 300.0, "height": 200.0, "originX": 1920, "originY": -8, "scale": 2.0 });
        let pin: Finish = serde_json::from_value(sent).unwrap();
        assert_eq!((pin.action.as_str(), pin.png.as_str(), pin.x, pin.y, pin.width, pin.height), ("pin", "iVBOR", 12.5, 40.0, 300.0, 200.0));
        assert_eq!((pin.origin_x, pin.origin_y, pin.scale), (1920, -8, 2.0));
        // Cancelling needs nothing else, even from a page that lost its capture.
        let cancel: Finish = serde_json::from_value(serde_json::json!({ "action": "cancel" })).unwrap();
        assert_eq!((cancel.action.as_str(), cancel.png.as_str(), cancel.origin_x, cancel.scale), ("cancel", "", 0, 1.0));
        assert!(serde_json::from_value::<Finish>(serde_json::json!({ "png": "" })).is_err());
    }

    #[test]
    fn a_selection_for_an_extension_is_saved_where_it_asked() {
        let dir = std::env::temp_dir().join(format!("heyo-pick-test-{}", std::process::id()));
        let path = dir.join("captures").join("capture-1.png");
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(7, 3, image::Rgba([10, 20, 30, 255])).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        assert_eq!(save_pick(&png, &path), Picked::Saved { width: 7, height: 3 });
        assert_eq!(std::fs::read(&path).unwrap(), png);
        // What is not a picture is not saved as one.
        let other = dir.join("captures").join("capture-2.png");
        assert!(matches!(save_pick(b"not a png", &other), Picked::Failed(_)));
        assert!(!other.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    // One test, because the selection being waited for is one for the process.
    #[test]
    fn whoever_waits_for_a_selection_hears_how_it_ended_once() {
        let (done, picked) = std::sync::mpsc::channel();
        *PICK.lock().unwrap() = Some(Pick { path: PathBuf::from("/nowhere/a.png"), done });
        end_pick(Picked::Cancelled);
        assert_eq!(picked.try_recv(), Ok(Picked::Cancelled));
        // Nobody waits any more: a second ending is nobody's.
        end_pick(Picked::Failed("late".into()));
        assert!(picked.try_recv().is_err());
        assert!(PICK.lock().unwrap().is_none());
    }

    #[test]
    fn a_selection_is_cut_from_the_capture_with_its_colours_in_order() {
        // Three by two pixels, each its own colour: blue first, as the screen is read.
        let bgrx: Vec<u8> = (0..6u8).flat_map(|n| [n, 10 + n, 20 + n, 0]).collect();
        let part = cut(&bgrx, 3, selecting::Area { x: 1, y: 0, width: 2, height: 2 });
        assert_eq!(part.dimensions(), (2, 2));
        assert_eq!(part.into_raw(), [21, 11, 1, 255, 22, 12, 2, 255, 24, 14, 4, 255, 25, 15, 5, 255]);
        // What reaches past the capture is cut to it.
        assert_eq!(cut(&bgrx, 3, selecting::Area { x: 2, y: 1, width: 40, height: 40 }).into_raw(), [25, 15, 5, 255]);
        // Cut pixels and a PNG from the page end as the same picture.
        let cut_out = Selected::Pixels(cut(&bgrx, 3, selecting::Area { x: 0, y: 0, width: 3, height: 2 }));
        let through_png = Selected::Png(cut_out.png().unwrap().into_owned());
        assert_eq!(through_png.pixels().unwrap(), cut_out.pixels().unwrap());
    }

    #[test]
    fn windows_are_placed_from_the_captured_monitors_corner() {
        let first = Area { x: 0, y: 0, width: 1920, height: 1080, scale: 1.0 };
        assert_eq!(windows_in(first, &[rect(100, 50, 800, 600)]), [rect(100, 50, 800, 600)]);
        // A monitor to the right, and one above and to the left of the first.
        let second = Area { x: 1920, y: 0, width: 2560, height: 1440, scale: 1.0 };
        assert_eq!(windows_in(second, &[rect(2000, 100, 800, 600)]), [rect(80, 100, 800, 600)]);
        let above = Area { x: -1280, y: -1024, width: 1280, height: 1024, scale: 1.0 };
        assert_eq!(windows_in(above, &[rect(-1200, -1000, 640, 480)]), [rect(80, 24, 640, 480)]);
    }

    #[test]
    fn the_scale_does_not_enter_into_it() {
        // Frames and the capture are both in physical pixels; the overlay
        // divides by its scale when it draws.
        let hidpi = Area { x: 3840, y: 0, width: 3840, height: 2160, scale: 2.0 };
        assert_eq!(windows_in(hidpi, &[rect(4000, 200, 1600, 1200)]), [rect(160, 200, 1600, 1200)]);
    }

    #[test]
    fn a_window_is_cut_to_what_of_it_is_on_the_monitor() {
        let second = Area { x: 1920, y: 0, width: 1920, height: 1080, scale: 1.0 };
        // Across the edge between two monitors, and off the bottom.
        assert_eq!(windows_in(second, &[rect(1500, 700, 1000, 600)]), [rect(0, 700, 580, 380)]);
        // Wholly on the other monitor, a sliver showing, and none at all.
        assert_eq!(windows_in(second, &[rect(0, 0, 1920, 1080), rect(1000, 0, 930, 500), rect(2000, 100, 0, 0)]), []);
        // A maximized window is the whole capture.
        assert_eq!(windows_in(second, &[rect(1920, 0, 1920, 1080)]), [rect(0, 0, 1920, 1080)]);
    }

    #[test]
    fn windows_are_fitted_to_a_capture_with_fewer_pixels_than_the_toolkit_counts() {
        // 150 %: the toolkit counts 2560 pixels across, the capture has 1920.
        let area = Area { x: 0, y: 0, width: 2560, height: 1440, scale: 2.0 };
        assert_eq!(fitted(vec![rect(200, 100, 1600, 1200)], area, 1920), [rect(150, 75, 1200, 900)]);
        assert_eq!(fitted(vec![rect(200, 100, 1600, 1200)], area, 2560), [rect(200, 100, 1600, 1200)]);
    }

    #[test]
    fn the_topmost_stays_first() {
        let area = Area { x: 0, y: 0, width: 1920, height: 1080, scale: 1.0 };
        let frames = [rect(300, 300, 400, 300), rect(-5000, 0, 100, 100), rect(0, 0, 1920, 1080)];
        assert_eq!(windows_in(area, &frames), [rect(300, 300, 400, 300), rect(0, 0, 1920, 1080)]);
    }
}
