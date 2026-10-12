//! What a desktop must be told for the pill to float over everything and
//! never have the keyboard. This is all of the pill that differs by
//! platform: the window itself and what it shows are the same everywhere.
//!
//! - X11: the window goes past the window manager (override-redirect). It
//!   is then above every window the manager stacks, a fullscreen one too,
//!   on every workspace, and is never given the focus, by appearing or by
//!   a click. Slint makes a window anew when it shows one it had hidden,
//!   without what is said here, so the pill is hidden and shown by winit,
//!   which keeps the window (`show`, `hide`).
//! - Windows: not in the taskbar or the window switcher, and never
//!   activated: not when it is first shown (winit sees to that once), not
//!   when it is shown again (winit would activate it then, so the window
//!   is shown and hidden here, by its handle), and not by a click (a style
//!   winit has no name for, set on the handle once the window is made).
//!   It is above the taskbar as every topmost window is.
//! - Wayland: a window can say none of this of itself, and winit cannot
//!   hide one there. Slint hides and shows it (`kept`), making it anew
//!   each time; where it is put and whether it is given the keyboard is
//!   the compositor's to decide.

use slint::winit_030::winit::window::{Window, WindowAttributes};

/// What winit is told of the pill's window before it makes it.
#[cfg(target_os = "linux")]
pub(in super::super) fn attributes(attributes: WindowAttributes) -> WindowAttributes {
    use slint::winit_030::winit::platform::x11::WindowAttributesExtX11;
    attributes.with_override_redirect(true)
}

#[cfg(windows)]
pub(in super::super) fn attributes(attributes: WindowAttributes) -> WindowAttributes {
    use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
    attributes.with_skip_taskbar(true).with_active(false)
}

#[cfg(not(any(target_os = "linux", windows)))]
pub(in super::super) fn attributes(attributes: WindowAttributes) -> WindowAttributes {
    attributes
}

#[cfg(windows)]
fn handle(window: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as windows_sys::Win32::Foundation::HWND),
        _ => None,
    }
}

/// Whether the window, once made, is kept and only put on and taken off
/// the screen (`show`, `hide`). Where it is not, Slint shows and hides it.
pub(super) fn kept() -> bool {
    !super::super::thread::wayland()
}

/// What can only be said of the window once it is there.
pub(super) fn made(window: &Window) {
    #[cfg(windows)]
    if let Some(window) = handle(window) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW};
        unsafe {
            let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
            SetWindowLongPtrW(window, GWL_EXSTYLE, style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize);
        }
    }
    #[cfg(not(windows))]
    let _ = window;
}

/// Puts the window on screen, above the others, without the keyboard.
pub(super) fn show(window: &Window) {
    #[cfg(windows)]
    if let Some(window) = handle(window) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE};
        unsafe {
            ShowWindow(window, SW_SHOWNOACTIVATE);
            SetWindowPos(window, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
    #[cfg(not(windows))]
    window.set_visible(true);
}

/// Takes it off the screen; the window stays, for the next time.
pub(super) fn hide(window: &Window) {
    #[cfg(windows)]
    if let Some(window) = handle(window) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
        unsafe {
            ShowWindow(window, SW_HIDE);
        }
    }
    #[cfg(not(windows))]
    window.set_visible(false);
}
