//! Reading what is on the screen.

use super::desktop::desktop;

/// A monitor's area in physical pixels.
#[derive(Clone, Copy)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// A screen capture: rows of B, G, R and an unused byte per pixel. This is
/// how an X server delivers it and what Cairo calls RGB24, so the overlay
/// can paint it without converting.
pub struct Shot {
    pub width: u32,
    pub height: u32,
    pub bgrx: Vec<u8>,
}

/// Captures through GNOME's screenshot tool, where the screen cannot be
/// read directly.
#[cfg(target_os = "linux")]
pub(super) fn grab_with_tool(area: Area) -> Result<Shot, String> {
    let cache = crate::paths::runtime_dir();
    let path = cache.join(format!("heyo-capture-{}.png", std::process::id()));
    let status = std::process::Command::new("gnome-screenshot").arg("-f").arg(&path).status().map_err(|e| format!("gnome-screenshot: {e}"))?;
    if !status.success() {
        return Err("gnome-screenshot failed".into());
    }
    let full = image::open(&path).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&path);
    let full = full?.into_rgba8();
    let (x, y) = (area.x.max(0) as u32, area.y.max(0) as u32);
    if x >= full.width() || y >= full.height() {
        return Err("the captured image does not cover this monitor".into());
    }
    let cropped: image::RgbaImage =
        image::imageops::crop_imm(&full, x, y, area.width.min(full.width() - x), area.height.min(full.height() - y)).to_image();
    let (width, height) = cropped.dimensions();
    let mut bgrx = cropped.into_raw();
    for pixel in bgrx.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Ok(Shot { width, height, bgrx })
}

/// Captures one monitor's area.
pub fn grab(area: Area) -> Result<Shot, String> {
    desktop().grab(area)
}
