//! The Windows clipboard: noticing copies, and borrowing it for a moment
//! (to learn what is selected in an application that will only say so by
//! copying) without the borrowing being taken for a copy.

use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{GlobalFree, HANDLE};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};

use super::wide;
use crate::platform::clipboard_watch::Change;
use crate::platform::win_rules::{memory_format, text_of};

const TEXT: u32 = 13; // CF_UNICODETEXT
const BITMAP: u32 = 8; // CF_DIB

/// More than this is not set aside to be put back.
const LARGEST_SAVED: usize = 64 << 20;

/// Heyo is borrowing the clipboard: what changes now is not a copy.
static BORROWED: AtomicBool = AtomicBool::new(false);
/// The number Windows gave the clipboard as Heyo last left it (0: never).
static LEFT_AT: AtomicU32 = AtomicU32::new(0);

fn format(name: &str) -> u32 {
    unsafe { RegisterClipboardFormatW(wide(name).as_ptr()) }
}

/// The clipboard, open; closed again when dropped. Only one program can
/// have it open, so it is held as briefly as possible.
struct Open;

impl Open {
    fn new() -> Option<Open> {
        // Another program may have it for a moment.
        for _ in 0..10 {
            if unsafe { OpenClipboard(null_mut()) } != 0 {
                return Some(Open);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    /// What the clipboard holds in a format, if that is a block of memory.
    fn bytes(&self, format: u32) -> Option<Vec<u8>> {
        unsafe {
            let handle = GetClipboardData(format);
            if handle.is_null() {
                return None;
            }
            let size = GlobalSize(handle);
            let data = GlobalLock(handle);
            if data.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(data as *const u8, size).to_vec();
            GlobalUnlock(handle);
            Some(bytes)
        }
    }

    fn text(&self) -> Option<String> {
        let bytes = self.bytes(TEXT)?;
        let units: Vec<u16> = bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
        Some(text_of(&units))
    }

    /// Puts a block of memory there in a format. The clipboard must have
    /// been emptied by this program first.
    fn put(&self, format: u32, bytes: &[u8]) -> bool {
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
            if handle.is_null() {
                return false;
            }
            let data = GlobalLock(handle);
            if data.is_null() {
                GlobalFree(handle);
                return false;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), data as *mut u8, bytes.len());
            GlobalUnlock(handle);
            // Windows owns the memory from here on, if it takes it.
            if SetClipboardData(format, handle as HANDLE).is_null() {
                GlobalFree(handle);
                return false;
            }
            true
        }
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        unsafe { CloseClipboard() };
    }
}

/// Everything on the clipboard that can be copied: format and contents, in
/// the order their owner put them there.
struct Saved(Vec<(u32, Vec<u8>)>);

impl Saved {
    /// `None`: the clipboard could not be opened, or holds more than is
    /// reasonable to copy; either way it cannot be put back, so it had
    /// better be left alone.
    fn take() -> Option<Saved> {
        let clipboard = Open::new()?;
        let (mut saved, mut total, mut format) = (Vec::new(), 0usize, 0u32);
        loop {
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                break;
            }
            if !memory_format(format) {
                continue;
            }
            let Some(bytes) = clipboard.bytes(format) else { continue };
            total += bytes.len();
            if total > LARGEST_SAVED {
                eprintln!("heyo: the clipboard holds too much to set aside");
                return None;
            }
            saved.push((format, bytes));
        }
        Some(Saved(saved))
    }

    fn put_back(&self) -> bool {
        let Some(clipboard) = Open::new() else { return false };
        if unsafe { EmptyClipboard() } == 0 {
            return false;
        }
        let mut whole = true;
        for (format, bytes) in &self.0 {
            whole &= clipboard.put(*format, bytes);
        }
        // It was in Windows' own history (Win+V) since it was first copied;
        // this says not to list it there a second time.
        if !self.0.is_empty() {
            clipboard.put(self::format("CanIncludeInClipboardHistory"), &0u32.to_le_bytes());
        }
        whole
    }
}

/// Marks the time during which the clipboard's changes are Heyo's own
/// doing, for `watch` to pass over.
struct Borrowing {
    /// The clipboard's number when the borrowing began.
    found_at: u32,
}

impl Borrowing {
    fn begin() -> Borrowing {
        BORROWED.store(true, Ordering::SeqCst);
        Borrowing { found_at: unsafe { GetClipboardSequenceNumber() } }
    }
}

impl Drop for Borrowing {
    fn drop(&mut self) {
        let now = unsafe { GetClipboardSequenceNumber() };
        // Untouched, it is still whatever the user last copied.
        if now != self.found_at {
            LEFT_AT.store(now, Ordering::SeqCst);
        }
        BORROWED.store(false, Ordering::SeqCst);
    }
}

/// The text that `copy` (which presses the application's copy shortcut)
/// puts on the clipboard, with the clipboard as it was afterwards. Empty
/// when nothing was copied: with nothing selected most applications copy
/// nothing, and then the clipboard is not touched at all.
pub(super) fn text_copied_by(copy: impl FnOnce() -> Result<(), String>) -> String {
    /// How long an application gets to answer the shortcut.
    const ANSWER: Duration = Duration::from_millis(400);
    /// And then to finish writing: some put their formats there one by one.
    const SETTLE: Duration = Duration::from_millis(30);

    // Without the old contents in hand, nothing is risked.
    let Some(saved) = Saved::take() else { return String::new() };
    let borrowing = Borrowing::begin();
    // The history's own guard against Heyo's plumbing, for good measure.
    crate::clipboard::skip_next();
    let before = borrowing.found_at;
    if let Err(e) = copy() {
        eprintln!("heyo: could not ask for the selected text: {e}");
        return String::new();
    }
    let asked = Instant::now();
    while unsafe { GetClipboardSequenceNumber() } == before {
        if asked.elapsed() >= ANSWER {
            return String::new();
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(SETTLE);
    let text = Open::new().and_then(|clipboard| clipboard.text()).unwrap_or_default();
    crate::clipboard::skip_next();
    if !saved.put_back() {
        eprintln!("heyo: could not put the clipboard back as it was");
    }
    text
}

/// Whether the clipboard's owner asks that this not be kept: the format
/// password managers put beside a secret, or Windows' own way of saying
/// "not for the clipboard history".
fn secret(marked: u32, history: u32) -> bool {
    let has = |format: u32| format != 0 && unsafe { IsClipboardFormatAvailable(format) } != 0;
    if has(marked) {
        return true;
    }
    // There with the value 0: keep it out. (Any other value allows it.)
    has(history) && Open::new().and_then(|clipboard| clipboard.bytes(history)).is_some_and(|bytes| bytes.len() >= 4 && bytes[..4] == [0; 4])
}

/// Reports each clipboard change until the session ends. Windows numbers
/// them, so a glance at the number a few times a second is all it takes.
pub fn watch(mut on_change: impl FnMut(Change)) -> Result<(), String> {
    let marked = format("ExcludeClipboardContentFromMonitorProcessing");
    let history = format("CanIncludeInClipboardHistory");
    let mut seen = unsafe { GetClipboardSequenceNumber() };
    loop {
        std::thread::sleep(Duration::from_millis(250));
        if BORROWED.load(Ordering::SeqCst) {
            continue;
        }
        let number = unsafe { GetClipboardSequenceNumber() };
        if number == seen {
            continue;
        }
        seen = number;
        // As Heyo left it after borrowing it: nothing was copied.
        if number == LEFT_AT.load(Ordering::SeqCst) {
            continue;
        }
        let has = |format: u32| unsafe { IsClipboardFormatAvailable(format) != 0 };
        on_change(Change { has_text: has(TEXT), has_image: has(BITMAP), secret: secret(marked, history), source: super::windows::active_class() });
    }
}
