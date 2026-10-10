//! What the Windows adapter (`win`) works out without asking Windows. It
//! is kept apart from the adapter so that it is compiled, and tested,
//! wherever Heyo is built.

use super::windows::Rect;

/// The name of a program from the path of its file, lowercase and without
/// ".exe": what stands in for a window class on Windows.
pub fn program_name(path: &str) -> String {
    let name = path.rsplit(['\\', '/']).next().unwrap_or_default().to_lowercase();
    name.strip_suffix(".exe").unwrap_or(&name).to_owned()
}

/// Whether a program (by `program_name`) is a console or terminal: there
/// Ctrl+C does not copy but stops whatever is running.
pub fn console(program: &str) -> bool {
    const CONSOLES: &[&str] =
        &["windowsterminal", "openconsole", "conhost", "cmd", "powershell", "pwsh", "wezterm-gui", "alacritty", "mintty", "kitty", "hyper", "tabby", "warp", "wsl", "bash", "ubuntu", "putty"];
    CONSOLES.contains(&program)
}

/// Text as Windows keeps it (UTF-16, ending at the first NUL if there is one).
pub fn text_of(units: &[u16]) -> String {
    let end = units.iter().position(|unit| *unit == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

/// Whether what the clipboard holds in this format is a block of memory
/// that can be copied and put back. The others are handles to things only
/// their owner can copy (bitmaps, palettes, metafiles), or nothing at all.
pub fn memory_format(format: u32) -> bool {
    const BITMAP: u32 = 2;
    const METAFILE_PICTURE: u32 = 3;
    const PALETTE: u32 = 9;
    const ENHANCED_METAFILE: u32 = 14;
    // Drawn by the owner, and the "display" twins of the handle formats.
    const OWNER_DISPLAY: u32 = 0x80;
    const DISPLAY_BITMAP: u32 = 0x82;
    const DISPLAY_METAFILE_PICTURE: u32 = 0x83;
    const DISPLAY_ENHANCED_METAFILE: u32 = 0x8E;
    // Private formats that are GDI objects.
    const GDI_OBJECTS: std::ops::RangeInclusive<u32> = 0x300..=0x3FF;
    format != 0
        && ![BITMAP, METAFILE_PICTURE, PALETTE, ENHANCED_METAFILE, OWNER_DISPLAY, DISPLAY_BITMAP, DISPLAY_METAFILE_PICTURE, DISPLAY_ENHANCED_METAFILE].contains(&format)
        && !GDI_OBJECTS.contains(&format)
}

/// The rectangle to ask Windows for so that a window is seen at `wanted`.
/// `outer` is the window's rectangle as Windows counts it now and `seen`
/// the part of that which shows: the difference (borders that are there
/// only to be grabbed) is added around. A difference too large to be such
/// borders is not believed.
pub fn with_borders(wanted: Rect, outer: Rect, seen: Rect) -> Rect {
    let (left, top) = (seen.x - outer.x, seen.y - outer.y);
    let (right, bottom) = (outer.x + outer.width - seen.x - seen.width, outer.y + outer.height - seen.y - seen.height);
    if [left, top, right, bottom].iter().any(|border| !(0..=32).contains(border)) {
        return wanted;
    }
    Rect { x: wanted.x - left, y: wanted.y - top, width: wanted.width + left + right, height: wanted.height + top + bottom }
}

/// An image with transparency from two pictures of it without: one drawn
/// over black, one over white (both as rows of B, G, R and an unused
/// byte). Where the two agree the image is opaque; where they differ as
/// much as black and white do, there is nothing of it. Returns RGBA.
///
/// This is how an icon is read: Windows draws every kind of icon (with a
/// mask, or with transparency of its own) correctly onto a background,
/// but says nothing dependable about its transparency otherwise.
pub fn unblended(over_black: &[u8], over_white: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(over_black.len());
    for (black, white) in over_black.chunks_exact(4).zip(over_white.chunks_exact(4)) {
        // Over white each channel is the one over black plus what shows through.
        let through = (0..3).map(|i| white[i].saturating_sub(black[i])).max().unwrap_or(0);
        let alpha = 255 - through as u32;
        // Over black a channel is the colour times alpha.
        let colour = |channel: u8| if alpha == 0 { 0 } else { ((channel as u32 * 255 + alpha / 2) / alpha).min(255) as u8 };
        rgba.extend_from_slice(&[colour(black[2]), colour(black[1]), colour(black[0]), alpha as u8]);
    }
    rgba
}

/// RGBA from rows of B, G, R, A in which the colours were already
/// multiplied by the transparency, as the shell hands out its pictures.
pub fn straightened(bgra: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(bgra.len());
    for pixel in bgra.chunks_exact(4) {
        let alpha = pixel[3] as u32;
        let colour = |channel: u8| if alpha == 0 { 0 } else { ((channel as u32 * 255 + alpha / 2) / alpha).min(255) as u8 };
        rgba.extend_from_slice(&[colour(pixel[2]), colour(pixel[1]), colour(pixel[0]), pixel[3]]);
    }
    rgba
}

/// Whether an RGBA image shows anything at all.
pub fn visible(rgba: &[u8]) -> bool {
    rgba.chunks_exact(4).any(|pixel| pixel[3] != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_named_by_its_file() {
        assert_eq!(program_name(r"C:\Program Files\Mozilla Firefox\firefox.exe"), "firefox");
        assert_eq!(program_name(r"C:\Windows\System32\NOTEPAD.EXE"), "notepad");
        assert_eq!(program_name("C:/tools/rg.exe"), "rg");
        assert_eq!(program_name(r"C:\tools\script.cmd"), "script.cmd");
        assert_eq!(program_name(""), "");
    }

    #[test]
    fn consoles_are_known_by_their_programs() {
        assert!(console(&program_name(r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal_1.21\WindowsTerminal.exe")));
        assert!(console("cmd"));
        assert!(!console("code"));
        assert!(!console("cmder-like-editor"));
    }

    #[test]
    fn text_ends_at_the_first_nul() {
        let units: Vec<u16> = "héllo\0junk".encode_utf16().collect();
        assert_eq!(text_of(&units), "héllo");
        assert_eq!(text_of(&"no end".encode_utf16().collect::<Vec<_>>()), "no end");
        assert_eq!(text_of(&[]), "");
    }

    #[test]
    fn only_memory_is_copied_from_the_clipboard() {
        // Text, a device-independent bitmap, a list of files.
        assert!([1, 13, 8, 17, 15].into_iter().all(memory_format));
        // Formats applications register ("HTML Format", …).
        assert!(memory_format(0xC0A5));
        // A bitmap handle, a palette, a metafile, a private GDI object.
        assert!(![2, 3, 9, 14, 0x80, 0x82, 0x300, 0x3FF, 0].into_iter().any(memory_format));
    }

    #[test]
    fn unseen_borders_are_added_around_what_is_wanted() {
        // Windows 10: seven pixels left, right and below, none on top.
        let outer = Rect { x: 93, y: 100, width: 814, height: 607 };
        let seen = Rect { x: 100, y: 100, width: 800, height: 600 };
        let left_half = Rect { x: 0, y: 0, width: 960, height: 1040 };
        assert_eq!(with_borders(left_half, outer, seen), Rect { x: -7, y: 0, width: 974, height: 1047 });
        // No borders: as wanted.
        assert_eq!(with_borders(left_half, seen, seen), left_half);
        // A difference that cannot be borders (the two were not measured alike).
        assert_eq!(with_borders(left_half, Rect { x: 0, y: 0, width: 1600, height: 1200 }, seen), left_half);
        assert_eq!(with_borders(left_half, seen, outer), left_half);
    }

    #[test]
    fn transparency_comes_back_from_two_backgrounds() {
        // B, G, R, unused: opaque red; nothing; half-transparent white.
        let over_black = [0, 0, 255, 0, 0, 0, 0, 0, 128, 128, 128, 0];
        let over_white = [0, 0, 255, 0, 255, 255, 255, 0, 255, 255, 255, 0];
        let rgba = unblended(&over_black, &over_white);
        assert_eq!(rgba[0..4], [255, 0, 0, 255]);
        assert_eq!(rgba[4..8], [0, 0, 0, 0]);
        assert_eq!(rgba[8..12], [255, 255, 255, 128]);
        assert!(visible(&rgba));
        assert!(!visible(&unblended(&[0, 0, 0, 0], &[255, 255, 255, 0])));
        // The same three pixels as the shell would hand them out.
        assert_eq!(straightened(&[0, 0, 255, 255, 0, 0, 0, 0, 128, 128, 128, 128]), rgba);
    }
}
