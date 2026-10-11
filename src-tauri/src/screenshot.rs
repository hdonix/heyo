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

pub const WINDOW: &str = "screenshot";

static COUNTER: AtomicU32 = AtomicU32::new(0);
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

#[derive(Clone, Serialize)]
struct Capture {
    path: String,
    /// `capture` shows the toolbar; `text` extracts text as soon as a region is selected.
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
    let app = app.clone();
    let mode = mode.to_owned();
    let picking = mode == "pick";
    // A capture of the user's own takes the overlay from an extension's.
    if !picking {
        end_pick(Picked::Cancelled);
    }
    std::thread::spawn(move || {
        // Let the palette disappear from the screen first.
        if let Some(main) = app.get_webview_window("main") {
            if main.is_visible().unwrap_or(false) {
                let _ = main.hide();
                std::thread::sleep(Duration::from_millis(180));
            }
        }
        // A previous overlay must not end up in the capture.
        if app.get_webview_window(WINDOW).is_some_and(|w| w.is_visible().unwrap_or(false)) {
            close_overlay(&app);
            std::thread::sleep(Duration::from_millis(180));
        }
        let result = (|| {
            let monitor = current_monitor(&app)?;
            // Asked for meanwhile: the overlay does not wait for it to appear.
            let frames = std::thread::spawn(window_frames);
            let shot = grab(monitor)?;
            let area = Area { width: shot.width, height: shot.height, ..monitor };
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
    overlay::hide(app, WINDOW);
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

/// What the user chose to do with the selected region.
#[derive(Deserialize)]
pub struct Finish {
    /// `copy`, `save`, `pin`, `text`, `cancel`, or `pick` (for the extension
    /// that asked for the selection).
    action: String,
    /// The cropped, annotated region as base64 PNG.
    #[serde(default)]
    png: String,
    /// Selection in logical pixels, relative to the overlay.
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    height: f64,
    /// Screen position of the overlay in physical pixels, and its scale.
    #[serde(default)]
    origin_x: i32,
    #[serde(default)]
    origin_y: i32,
    #[serde(default = "one")]
    scale: f64,
}

fn one() -> f64 {
    1.0
}

/// Completes a capture. Returns the recognized text for `text`.
pub async fn finish(app: AppHandle, request: Finish) -> Result<String, String> {
    if request.action == "cancel" {
        close_overlay(&app);
        end_pick(Picked::Cancelled);
        return Ok(String::new());
    }
    // Whatever else became of the selection, it is not the extension's.
    if request.action != "pick" {
        end_pick(Picked::Cancelled);
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(&request.png).map_err(|e| e.to_string())?;
    match request.action.as_str() {
        "pick" => {
            close_overlay(&app);
            let pick = PICK.lock().unwrap().take().ok_or("nothing is waiting for this selection")?;
            let _ = pick.done.send(save_pick(&bytes, &pick.path));
            Ok(String::new())
        }
        "copy" => {
            let image = image::load(Cursor::new(&bytes), image::ImageFormat::Png).map_err(|e| e.to_string())?.into_rgba8();
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
            std::fs::write(&path, &bytes).map_err(|e| format!("failed to save {}: {e}", path.display()))?;
            close_overlay(&app);
            notify("Screenshot saved", &path.to_string_lossy());
            Ok(path.to_string_lossy().into_owned())
        }
        "pin" => {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = cache_dir().join(format!("pin-{n}.png"));
            std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
            close_overlay(&app);
            // The pinned image sits exactly where it was captured.
            let x = request.origin_x as f64 / request.scale + request.x;
            let y = request.origin_y as f64 / request.scale + request.y;
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
                .inner_size(request.width.max(24.0), request.height.max(24.0))
                .position(x, y)
                .build()
                .map_err(|e| e.to_string())?;
            Ok(String::new())
        }
        "text" => {
            let path = cache_dir().join("text.png");
            std::fs::create_dir_all(cache_dir()).map_err(|e| e.to_string())?;
            // Small interface text is recognized much better enlarged.
            let image = image::load(Cursor::new(&bytes), image::ImageFormat::Png).map_err(|e| e.to_string())?;
            let factor = if request.scale < 1.5 { 2 } else { 1 };
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
