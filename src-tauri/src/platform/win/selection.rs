//! The text selected in another application.
//!
//! Windows has nothing like X11's primary selection. An application that
//! is accessible says what is selected in it when asked (UI Automation's
//! text pattern: browsers, editors, Office and terminals answer); one that
//! is not can only be made to copy, after which the clipboard is put back
//! as it was. Neither is forced: a terminal is never sent Ctrl+C (which
//! would stop what runs in it), and nothing is copied while Heyo itself is
//! in front, for the keystroke would go to Heyo.

use std::sync::mpsc::channel;
use std::time::Duration;

use windows::Win32::Foundation::HWND as ComWindow;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern, TreeScope_Subtree, UIA_IsTextPatternAvailablePropertyId, UIA_TextPatternId,
};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use super::{application_windows, copy_keystroke, process_of, program_of};
use crate::platform::win_rules::console;

/// How long an application gets to say what is selected in it. One that
/// is busy, or has a great many controls to look through, is not waited for.
const PATIENCE: Duration = Duration::from_millis(800);
/// More text than this is not a selection anyone wants handed around.
const LONGEST: i32 = 1_000_000;

/// Whom to ask.
#[derive(Clone, Copy)]
enum Ask {
    /// Whatever has the keyboard.
    Focused,
    /// The first text in this window that can say what is selected in it.
    /// (A window that is not in front has no focus to go by.)
    Window(usize),
}

/// What the element, or the nearest thing around it that holds text, has selected.
unsafe fn selection_of(automation: &IUIAutomation, element: IUIAutomationElement) -> windows::core::Result<String> {
    let walker = automation.ControlViewWalker()?;
    let mut element = element;
    // A caret inside a paragraph inside a document: the document is what answers.
    let mut pattern = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId);
    for _ in 0..4 {
        if pattern.is_ok() {
            break;
        }
        element = walker.GetParentElement(&element)?;
        pattern = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId);
    }
    let ranges = pattern?.GetSelection()?;
    let mut text = String::new();
    for index in 0..ranges.Length()? {
        text.push_str(&ranges.GetElement(index)?.GetText(LONGEST)?.to_string());
    }
    Ok(text)
}

/// Asks through UI Automation, on this thread. `None`: nothing there
/// answers such questions.
fn read(ask: Ask) -> Option<String> {
    let _com = super::Com::begin(false)?;
    let answer = (|| unsafe {
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let element = match ask {
            Ask::Focused => automation.GetFocusedElement()?,
            Ask::Window(window) => {
                let holds_text = automation.CreatePropertyCondition(UIA_IsTextPatternAvailablePropertyId, &VARIANT::from(true))?;
                automation.ElementFromHandle(ComWindow(window as *mut core::ffi::c_void))?.FindFirst(TreeScope_Subtree, &holds_text)?
            }
        };
        // Heyo's own text fields are not "another application".
        if element.CurrentProcessId()? as u32 == GetCurrentProcessId() {
            return Err(windows::core::Error::empty());
        }
        selection_of(&automation, element)
    })();
    // Everything of COM's is gone by here, before COM itself goes.
    answer.ok()
}

/// Asks on a thread of its own, and does not wait long: the answer comes
/// from the other application, which may be busy.
fn ask(whom: Ask) -> Option<String> {
    let (answer, answered) = channel();
    std::thread::Builder::new()
        .name("selection".into())
        .spawn(move || {
            let _ = answer.send(read(whom));
        })
        .ok()?;
    answered.recv_timeout(PATIENCE).ok().flatten()
}

pub fn selected_text() -> String {
    let front = unsafe { GetForegroundWindow() };
    if front.is_null() {
        return String::new();
    }
    if process_of(front) == unsafe { GetCurrentProcessId() } {
        // From the palette: the application the user came from is the
        // next window down, and still knows what was selected in it.
        return application_windows().first().and_then(|window| ask(Ask::Window(*window as usize))).unwrap_or_default();
    }
    match ask(Ask::Focused) {
        // Said, even if it is "nothing".
        Some(text) => text,
        None if console(&program_of(front)) => String::new(),
        None => super::clipboard::text_copied_by(copy_keystroke),
    }
}
