//! The icons of programs and files, as Windows shows them, written out as
//! PNG files the palette can show.

use std::path::Path;
use std::ptr::null_mut;

use ::windows::core::HSTRING;
use ::windows::Win32::Foundation::SIZE as Size;
use ::windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY};
use windows_sys::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows_sys::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, DrawIconEx, PrivateExtractIconsW, DI_NORMAL, HICON};

use super::capture::Canvas;
use super::wide;
use crate::platform::win_rules::{straightened, unblended, visible};

/// The side of the pictures, in pixels: large enough for the palette's
/// rows on a dense screen.
const SIDE: i32 = 64;

/// Where an icon may be had from.
pub enum Source {
    /// One of the icons inside a program or icon file: its place there, or
    /// (negative) its resource number.
    Inside(String, i32),
    /// Whatever the shell shows for this file or folder.
    Shell(String),
    /// The picture of anything the shell can name, files or not: for an
    /// application from the Store, `shell:AppsFolder\` and its id.
    Item(String),
}

/// An icon of Heyo's own to destroy.
struct Icon(HICON);

impl Drop for Icon {
    fn drop(&mut self) {
        unsafe { DestroyIcon(self.0) };
    }
}

/// A bitmap of Heyo's own to delete.
struct Bitmap(HBITMAP);

impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe { DeleteObject(self.0) };
    }
}

/// The screen's device context, given back when dropped.
struct Screen(HDC);

impl Screen {
    fn get() -> Option<Screen> {
        let context = unsafe { GetDC(null_mut()) };
        (!context.is_null()).then_some(Screen(context))
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        unsafe { ReleaseDC(null_mut(), self.0) };
    }
}

fn load(source: &Source) -> Option<Icon> {
    unsafe {
        match source {
            Source::Inside(file, index) => {
                let (mut icon, mut id): (HICON, u32) = (null_mut(), 0);
                // Unlike the shell's own functions this one gives the size asked for.
                let found = PrivateExtractIconsW(wide(file).as_ptr(), *index, SIDE, SIDE, &mut icon, &mut id, 1, 0);
                (found != 0 && found != u32::MAX && !icon.is_null()).then(|| Icon(icon))
            }
            Source::Shell(file) => {
                let mut info: SHFILEINFOW = std::mem::zeroed();
                let found = SHGetFileInfoW(wide(file).as_ptr(), 0, &mut info, std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON);
                (found != 0 && !info.hIcon.is_null()).then(|| Icon(info.hIcon))
            }
            Source::Item(_) => None,
        }
    }
}

/// The icon as RGBA, `SIDE` pixels each way.
fn drawn(icon: &Icon) -> Option<image::RgbaImage> {
    let screen = Screen::get()?;
    let mut canvas = Canvas::new(screen.0, SIDE, SIDE).ok()?;
    // Drawn over black and over white: see `unblended`.
    let mut over = |background: u8| {
        canvas.fill(background);
        (unsafe { DrawIconEx(canvas.context, 0, 0, icon.0, SIDE, SIDE, 0, null_mut(), DI_NORMAL) } != 0).then(|| canvas.pixels())
    };
    let (over_black, over_white) = (over(0)?, over(255)?);
    image::RgbaImage::from_raw(SIDE as u32, SIDE as u32, unblended(&over_black, &over_white))
}

/// The shell's picture of something it can name.
fn of_item(name: &str) -> Option<image::RgbaImage> {
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(name), None).ok()?;
        let bitmap = Bitmap(factory.GetImage(Size { cx: SIDE, cy: SIDE }, SIIGBF_ICONONLY).ok()?.0);
        if bitmap.0.is_null() {
            return None;
        }
        // It may come smaller than asked for.
        let mut is: BITMAP = std::mem::zeroed();
        if GetObjectW(bitmap.0, std::mem::size_of::<BITMAP>() as i32, &mut is as *mut BITMAP as *mut core::ffi::c_void) == 0 {
            return None;
        }
        let (width, height) = (is.bmWidth, is.bmHeight.abs());
        if !(1..=1024).contains(&width) || !(1..=1024).contains(&height) {
            return None;
        }
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
        let mut bgra = vec![0u8; width as usize * height as usize * 4];
        let screen = Screen::get()?;
        let rows = GetDIBits(screen.0, bitmap.0, 0, height as u32, bgra.as_mut_ptr() as *mut core::ffi::c_void, &mut info, DIB_RGB_COLORS);
        if rows != height {
            return None;
        }
        image::RgbaImage::from_raw(width as u32, height as u32, straightened(&bgra))
    }
}

fn picture(source: &Source) -> Option<image::RgbaImage> {
    match source {
        Source::Item(name) => of_item(name),
        _ => drawn(&load(source)?),
    }
}

/// Writes the first icon that can be had of `sources` to `file` as PNG.
/// The shell's ways of finding icons want COM on the calling thread
/// (`super::Com`).
pub fn save(sources: &[Source], file: &Path) -> Result<(), String> {
    // A blank is no better than none.
    let image = sources.iter().filter_map(picture).find(|image| visible(image.as_raw())).ok_or("no icon")?;
    // Under another name until whole: the palette may be reading the old one.
    let partial = file.with_extension("part");
    image.save_with_format(&partial, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    std::fs::rename(&partial, file).map_err(|e| e.to_string())
}
