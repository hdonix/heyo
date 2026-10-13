//! Screenshots: the screen is read, part of it is selected and drawn on in
//! an overlay that covers the monitor, and the part is copied, saved,
//! pinned to the screen or has its text read.
//!
//! The screen is read before the overlay appears and shown in it, so the
//! selection is made on a frozen picture. The overlay is one of the windows
//! Slint draws (`slint_windows::screenshot`); what it shows and how it
//! answers the pointer and the keys is decided in `selecting`, which knows
//! no toolkit. This file is the host's part: reading the screen, and doing
//! with the selected part what was chosen.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::platform::capture::{grab, Area, Shot};
use crate::platform::common::notify;
use crate::platform::windows::Rect;
use selecting::{Action, Mode};

pub(crate) mod selecting;

static COUNTER: AtomicU32 = AtomicU32::new(0);

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

/// The monitor that the overlay is showing, as it was read: what is
/// selected is cut from it here.
struct Frozen {
    shot: Shot,
    area: Area,
}

static FROZEN: std::sync::Mutex<Option<std::sync::Arc<Frozen>>> = std::sync::Mutex::new(None);

fn cache_dir() -> PathBuf {
    crate::paths::cache().join("screenshot")
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

/// Writes the selected area where it was asked for, as a PNG.
fn save_pick(picture: &image::RgbaImage, path: &std::path::Path) -> Picked {
    let saved = (|| {
        std::fs::create_dir_all(path.parent().ok_or("the capture has no directory")?).map_err(|e| e.to_string())?;
        picture.save_with_format(path, image::ImageFormat::Png).map_err(|e| format!("failed to save {}: {e}", path.display()))?;
        Ok::<_, String>(Picked::Saved { width: picture.width(), height: picture.height() })
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
    start(app, Mode::Pick);
    // An overlay that went away without a word must not be waited for for ever.
    picked.recv_timeout(Duration::from_secs(600)).unwrap_or(Picked::Cancelled)
}

/// The monitor the pointer is on, else the primary one.
pub(crate) fn current_monitor(app: &AppHandle) -> Result<Area, String> {
    use crate::platform::place::{Display, Monitor, Which};
    let whole = |monitor: &tauri::Monitor| (monitor.position().x, monitor.position().y, monitor.size().width, monitor.size().height);
    let primary = app.primary_monitor().map_err(|e| e.to_string())?;
    let mut all = app.available_monitors().map_err(|e| e.to_string())?;
    // The primary one is there, whatever the list says.
    if let Some(primary) = primary.as_ref().filter(|primary| !all.iter().any(|monitor| whole(monitor) == whole(primary))) {
        all.push(primary.clone());
    }
    let primary = primary.as_ref().map(whole);
    let display = Display { monitors: all.iter().map(|monitor| Monitor::new(whole(monitor), monitor.scale_factor(), Some(whole(monitor)) == primary, None)).collect(), its: None };
    // The desktop's word first, where the toolkit cannot follow the pointer.
    let pointed = crate::platform::desktop::desktop().pointer_monitor().and_then(|origin| display.monitors.iter().find(|monitor| (monitor.whole.0, monitor.whole.1) == origin).copied());
    let monitor = pointed
        .or_else(|| match app.cursor_position() {
            Ok(pointer) => display.at((pointer.x.floor() as i32, pointer.y.floor() as i32)),
            Err(_) => display.monitor(Which::Primary),
        })
        .ok_or("no monitor found")?;
    let (x, y, width, height) = monitor.whole;
    Ok(Area { x, y, width, height, scale: monitor.scale })
}

/// For trying a pinned capture where the screen cannot be read yet (a
/// Wayland compositor Heyo has no way of capturing): with
/// `HEYO_TEST_PIN=X,Y,WIDTH,HEIGHT`, a picture of that size, its left half
/// one colour and its right another, is pinned at that place when Heyo
/// starts; several, separated by semicolons.
pub(crate) fn test_pin(app: &AppHandle) {
    let Ok(wanted) = std::env::var("HEYO_TEST_PIN") else { return };
    for one in wanted.split(';') {
        let numbers: Vec<i64> = one.split(',').filter_map(|number| number.trim().parse().ok()).collect();
        let [x, y, width, height] = numbers[..] else { continue };
        let picture = image::RgbaImage::from_fn(width.max(1) as u32, height.max(1) as u32, |across, _| if i64::from(across) * 2 < width { image::Rgba([216, 27, 96, 255]) } else { image::Rgba([30, 136, 229, 255]) });
        crate::slint_windows::screenshot::pin::show(app, picture, (x as i32, y as i32));
    }
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

/// Reads the screen and opens the overlay on it: `Capture` for a
/// screenshot of the user's own, `Text` to read the text in what is
/// selected, `Pick` for an extension that asked for an area.
pub fn start(app: &AppHandle, mode: Mode) {
    let asked = std::time::Instant::now();
    mark("screenshot asked for", asked);
    let app = app.clone();
    let picking = mode == Mode::Pick;
    // A capture of the user's own takes the overlay from an extension's.
    if !picking {
        end_pick(Picked::Cancelled);
    }
    std::thread::spawn(move || {
        // Neither the palette nor a previous overlay may end up in the capture.
        crate::palette::hide_until_gone(&app);
        if crate::slint_windows::screenshot::up() {
            close_overlay();
            std::thread::sleep(Duration::from_millis(60));
        }
        let result = (|| {
            let monitor = current_monitor(&app)?;
            // Asked for meanwhile: the overlay does not wait for it to appear.
            let frames = std::thread::spawn(window_frames);
            let shot = grab(monitor)?;
            mark("screen read", asked);
            let area = Area { width: shot.width, height: shot.height, ..monitor };
            let frozen = std::sync::Arc::new(Frozen { shot, area });
            *FROZEN.lock().unwrap() = Some(frozen.clone());
            if !crate::slint_windows::screenshot::show(&app, area, &frozen.shot, mode, asked) {
                *FROZEN.lock().unwrap() = None;
                return Err(format!("the overlay cannot be shown: {}", crate::slint_windows::failure()));
            }
            let windows = fitted(windows_in(monitor, &frames.join().unwrap_or_default()), monitor, area.width);
            let windows = windows.into_iter().map(|w| selecting::Area { x: w.x.max(0) as u32, y: w.y.max(0) as u32, width: w.width.max(0) as u32, height: w.height.max(0) as u32 });
            crate::slint_windows::screenshot::windows(windows.collect());
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

/// Closes the overlay without doing anything with the capture.
pub fn cancel() {
    close_overlay();
    end_pick(Picked::Cancelled);
}

/// Takes the overlay off the screen, and lets go of the capture: it shows
/// the whole screen, and is not kept.
fn close_overlay() {
    crate::slint_windows::screenshot::hide();
    *FROZEN.lock().unwrap() = None;
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

/// Where the selected part is on the screen, in the screen's pixels, and
/// the scale of the monitor it is on.
struct Place {
    at: (i32, i32),
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

/// What the overlay was told to do with the capture: does it. `drawn` is
/// the selected part as the overlay showed it, where something was drawn
/// on it; without, the part is cut from the capture as it was read. What
/// cannot be done is said in the overlay where that is still up, and in a
/// notification where it is not.
pub(crate) fn decided(app: &AppHandle, outcome: selecting::Outcome, drawn: Option<image::RgbaImage>) {
    let selecting::Outcome::Finish { action, area } = outcome else { return cancel() };
    let Some(frozen) = FROZEN.lock().unwrap().clone() else { return };
    // A pinned picture sits exactly where it was captured.
    let place = Place { at: (frozen.area.x + area.x as i32, frozen.area.y + area.y as i32), scale: if frozen.area.scale > 0.0 { frozen.area.scale } else { 1.0 } };
    let picture = drawn.unwrap_or_else(|| cut(&frozen.shot.bgrx, frozen.shot.width, area));
    if let Err(e) = complete(app, action, picture, place) {
        eprintln!("heyo: screenshot failed: {e}");
        if crate::slint_windows::screenshot::up() {
            crate::slint_windows::screenshot::failed(e);
        } else if action == Action::Pick {
            end_pick(Picked::Failed(e));
        } else {
            notify("Screenshot failed", &e);
        }
    }
}

/// Does with the selected part what was chosen.
fn complete(app: &AppHandle, action: Action, picture: image::RgbaImage, place: Place) -> Result<(), String> {
    // Whatever else became of the selection, it is not the extension's.
    if action != Action::Pick {
        end_pick(Picked::Cancelled);
    }
    match action {
        Action::Pick => {
            close_overlay();
            let pick = PICK.lock().unwrap().take().ok_or("nothing is waiting for this selection")?;
            let _ = pick.done.send(save_pick(&picture, &pick.path));
            Ok(())
        }
        Action::Copy => {
            let (width, height) = picture.dimensions();
            close_overlay();
            app.clipboard().write_image(&tauri::image::Image::new_owned(picture.into_raw(), width, height)).map_err(|e| e.to_string())?;
            notify("Screenshot copied", "The image is on the clipboard.");
            Ok(())
        }
        Action::Save => {
            let dir = pictures_dir();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let path = dir.join(format!("Screenshot {}.png", timestamp()));
            picture.save_with_format(&path, image::ImageFormat::Png).map_err(|e| format!("failed to save {}: {e}", path.display()))?;
            close_overlay();
            notify("Screenshot saved", &path.to_string_lossy());
            Ok(())
        }
        Action::Pin => {
            close_overlay();
            if crate::slint_windows::screenshot::pin::show(app, picture, place.at) {
                Ok(())
            } else {
                Err(format!("the capture cannot be pinned: {}", crate::slint_windows::failure()))
            }
        }
        Action::Text => {
            let path = cache_dir().join("text.png");
            std::fs::create_dir_all(cache_dir()).map_err(|e| e.to_string())?;
            // Small interface text is recognized much better enlarged.
            let picture = image::DynamicImage::ImageRgba8(picture);
            let factor = if place.scale < 1.5 { 2 } else { 1 };
            picture.resize(picture.width() * factor, picture.height() * factor, image::imageops::FilterType::CatmullRom).save(&path).map_err(|e| e.to_string())?;
            let recognized = crate::ocr::recognize(app, &path);
            // The selection may show anything; it is not kept once read.
            let _ = std::fs::remove_file(&path);
            let text = recognized?;
            if text.trim().is_empty() {
                return Err("No text was found in the selection.".into());
            }
            close_overlay();
            crate::platform::clipboard_watch::write_text(&text)?;
            let preview: String = text.chars().take(80).collect();
            notify("Text copied", &preview);
            Ok(())
        }
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
        let picture = image::RgbaImage::from_fn(7, 3, |x, y| image::Rgba([10 * x as u8, 20 * y as u8, 30, 255]));
        assert_eq!(save_pick(&picture, &path), Picked::Saved { width: 7, height: 3 });
        // A PNG, and the picture it was given.
        assert_eq!(image::open(&path).unwrap().into_rgba8(), picture);
        // Where it cannot be written, whoever asked hears why.
        std::fs::write(dir.join("a-file"), b"").unwrap();
        assert!(matches!(save_pick(&picture, &dir.join("a-file").join("capture-2.png")), Picked::Failed(_)));
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
