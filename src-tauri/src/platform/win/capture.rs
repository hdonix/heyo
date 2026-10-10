//! Reading the screen with GDI: the screen's device context covers every
//! monitor (those left of or above the primary one at negative
//! coordinates), and a copy of part of it into a bitmap of Heyo's own is
//! the capture.

use std::ptr::null_mut;

use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, GetDC, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    CAPTUREBLT, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, SRCCOPY,
};
use windows_sys::Win32::UI::HiDpi::{
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN};

use crate::platform::capture::{Area, Shot};
use crate::platform::windows::Rect;

/// More than any monitor has; a larger request is a mistake, not a screen.
const LARGEST: u32 = 16384;

/// Makes this thread see the screen in physical pixels for as long as it
/// lives. Heyo's process does already (the window toolkit declares it
/// aware of each monitor's scale when it starts); this holds even if that
/// should change, for a capture in scaled coordinates would be a blurred
/// part of the wrong place.
pub(super) struct PhysicalPixels(DPI_AWARENESS_CONTEXT);

impl PhysicalPixels {
    pub(super) fn new() -> Self {
        unsafe {
            let mut before = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            if before.is_null() {
                // Windows 10 before 1703 knows only the first version.
                before = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE);
            }
            PhysicalPixels(before)
        }
    }
}

impl Drop for PhysicalPixels {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { SetThreadDpiAwarenessContext(self.0) };
        }
    }
}

/// The screen's device context, given back when dropped.
struct Screen(HDC);

impl Drop for Screen {
    fn drop(&mut self) {
        unsafe { ReleaseDC(null_mut(), self.0) };
    }
}

/// A device context in memory with a bitmap to draw into: `pixels` are the
/// bitmap's own, top row first, four bytes (B, G, R, unused) each.
pub(super) struct Canvas {
    pub(super) context: HDC,
    bitmap: HBITMAP,
    /// What the context held before the bitmap; put back before both go.
    before: HGDIOBJ,
    pixels: *mut u8,
    length: usize,
}

impl Canvas {
    /// A canvas of that size, all zero. `like` is a device context whose
    /// colours it should share (the screen's).
    pub(super) fn new(like: HDC, width: i32, height: i32) -> Result<Self, String> {
        if width <= 0 || height <= 0 {
            return Err("nothing to draw into".into());
        }
        unsafe {
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // Negative: the first row is the top one.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };
            let context = CreateCompatibleDC(like);
            if context.is_null() {
                return Err("Windows gave no device context to draw into.".into());
            }
            let mut pixels: *mut core::ffi::c_void = null_mut();
            let bitmap = CreateDIBSection(like, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
            if bitmap.is_null() || pixels.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                DeleteDC(context);
                return Err("Windows gave no memory for the picture.".into());
            }
            let before = SelectObject(context, bitmap);
            Ok(Canvas { context, bitmap, before, pixels: pixels as *mut u8, length: width as usize * height as usize * 4 })
        }
    }

    /// The pixels as they are now.
    pub(super) fn pixels(&self) -> Vec<u8> {
        unsafe {
            // GDI may still be drawing.
            GdiFlush();
            std::slice::from_raw_parts(self.pixels, self.length).to_vec()
        }
    }

    /// Sets every byte of every pixel to `value`.
    pub(super) fn fill(&mut self, value: u8) {
        unsafe {
            GdiFlush();
            std::ptr::write_bytes(self.pixels, value, self.length);
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            if !self.before.is_null() {
                SelectObject(self.context, self.before);
            }
            DeleteObject(self.bitmap);
            DeleteDC(self.context);
        }
    }
}

/// All the monitors together, in physical pixels.
fn whole_screen() -> Rect {
    unsafe {
        Rect {
            x: GetSystemMetrics(SM_XVIRTUALSCREEN),
            y: GetSystemMetrics(SM_YVIRTUALSCREEN),
            width: GetSystemMetrics(SM_CXVIRTUALSCREEN),
            height: GetSystemMetrics(SM_CYVIRTUALSCREEN),
        }
    }
}

/// Captures an area (physical pixels, relative to the primary monitor's
/// corner, so possibly negative). What of it lies on no monitor is black.
pub fn grab(area: Area) -> Result<Shot, String> {
    if area.width == 0 || area.height == 0 || area.width > LARGEST || area.height > LARGEST {
        return Err(format!("cannot capture an area of {}×{} pixels", area.width, area.height));
    }
    let (width, height) = (area.width as i32, area.height as i32);
    let _physical = PhysicalPixels::new();
    let wanted = Rect { x: area.x, y: area.y, width, height };
    let on_screen = wanted.intersect(&whole_screen());
    if on_screen.width == 0 || on_screen.height == 0 {
        return Err("the area to capture is not on any monitor".into());
    }
    unsafe {
        let screen = GetDC(null_mut());
        if screen.is_null() {
            return Err("Windows did not let the screen be read.".into());
        }
        let screen = Screen(screen);
        let canvas = Canvas::new(screen.0, width, height)?;
        // CAPTUREBLT: windows that are composited over the others (menus,
        // tooltips, translucent ones) are part of the picture too.
        if BitBlt(canvas.context, 0, 0, width, height, screen.0, area.x, area.y, SRCCOPY | CAPTUREBLT) == 0 {
            // Fails while the secure desktop is up (a lock screen, a UAC prompt).
            return Err("Windows did not let the screen be read.".into());
        }
        Ok(Shot { width: area.width, height: area.height, bgrx: canvas.pixels() })
    }
}
