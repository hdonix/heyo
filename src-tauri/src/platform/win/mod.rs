//! Windows: other applications' windows, typing into them and reading
//! what is selected there, the clipboard, the screen, a key held to talk
//! and global hotkeys, through the Win32 API.

pub mod autostart;
mod capture;
mod clipboard;
mod hold;
pub mod hotkeys;
pub(crate) mod icons;
mod selection;

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows_sys::Win32::System::Threading::{GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindow, IsWindowVisible, IsZoomed, PostMessageW, SetForegroundWindow, SetWindowPos, ShowWindow, GWL_EXSTYLE, GW_OWNER, SWP_NOACTIVATE, SWP_NOZORDER,
    SW_MAXIMIZE, SW_RESTORE, WM_CLOSE, WS_EX_TOOLWINDOW,
};

use crate::platform::capture::{Area, Shot};
use crate::platform::clipboard_watch::Change;
use crate::platform::keys::Hold;
use crate::platform::win_rules::{program_name, with_borders};
use crate::platform::windows::{carried, next_display, target, Layout, Rect, Window, WindowInfo};

/// COM on this thread, for as long as this lives. The shell's functions
/// want it "apartment threaded"; whoever only asks other programs things
/// (UI Automation) is better off without an apartment.
pub(crate) struct Com;

impl Com {
    pub(crate) fn begin(for_the_shell: bool) -> Option<Com> {
        use ::windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED};
        let threading = if for_the_shell { COINIT_APARTMENTTHREADED } else { COINIT_MULTITHREADED };
        // Refused only where the thread already has COM another way; then
        // it is not this one's to end either.
        unsafe { CoInitializeEx(None, threading) }.is_ok().then_some(Com)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe { ::windows::Win32::System::Com::CoUninitialize() };
    }
}

/// A windowed program on Windows has nowhere to say anything: what Heyo
/// (and the helper programs it starts) writes to standard error goes to
/// `heyo.log` in its cache directory instead, started afresh when large.
pub fn log_to_file() {
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE};
    let path = crate::paths::cache().join("heyo.log");
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let large = std::fs::metadata(&path).is_ok_and(|m| m.len() > 1_000_000);
    let Ok(file) = std::fs::OpenOptions::new().create(true).append(!large).write(true).truncate(large).open(&path) else { return };
    unsafe { SetStdHandle(STD_ERROR_HANDLE, file.into_raw_handle() as _) };
}

/// Whether the window in front is one of Heyo's.
pub fn own_in_front() -> bool {
    let window = unsafe { GetForegroundWindow() };
    !window.is_null() && process_of(window) == unsafe { GetCurrentProcessId() }
}

fn handle(window: Window) -> HWND {
    window as usize as HWND
}

fn id(window: HWND) -> Window {
    window as usize as Window
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn process_of(window: HWND) -> u32 {
    let mut process = 0;
    unsafe { GetWindowThreadProcessId(window, &mut process) };
    process
}

/// The name of the program a window belongs to, lowercase, without ".exe":
/// what stands in for a window class here.
fn program_of(window: HWND) -> String {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_of(window));
        if process.is_null() {
            return String::new();
        }
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        let found = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);
        CloseHandle(process);
        if found == 0 {
            return String::new();
        }
        program_name(&String::from_utf16_lossy(&buffer[..(length as usize).min(buffer.len())]))
    }
}

fn title_of(window: HWND) -> String {
    unsafe {
        let length = GetWindowTextLengthW(window);
        if length <= 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; length as usize + 1];
        let copied = GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32);
        String::from_utf16_lossy(&buffer[..copied.max(0) as usize])
    }
}

/// Other applications' windows as the taskbar would show them, front to back.
fn application_windows() -> Vec<HWND> {
    unsafe extern "system" fn collect(window: HWND, list: LPARAM) -> BOOL {
        let list = &mut *(list as *mut Vec<HWND>);
        let shown = IsWindowVisible(window) != 0
            && GetWindow(window, GW_OWNER).is_null()
            && GetWindowLongW(window, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW == 0
            && GetWindowTextLengthW(window) > 0
            && process_of(window) != GetCurrentProcessId();
        if shown {
            list.push(window);
        }
        1
    }
    let mut list: Vec<HWND> = Vec::new();
    unsafe { EnumWindows(Some(collect), &mut list as *mut _ as LPARAM) };
    // The shell's own full-screen "window".
    list.retain(|window| title_of(*window) != "Program Manager");
    list
}

pub mod windows {
    use super::*;

    pub fn active_external() -> Option<Window> {
        let window = unsafe { GetForegroundWindow() };
        (!window.is_null() && process_of(window) != unsafe { GetCurrentProcessId() }).then(|| id(window))
    }

    pub fn active_class() -> Option<String> {
        active_external().map(|window| program_of(handle(window))).filter(|name| !name.is_empty())
    }

    pub fn list(current: Option<Window>) -> Result<Vec<WindowInfo>, String> {
        let mut windows: Vec<WindowInfo> = application_windows()
            .into_iter()
            .map(|window| WindowInfo { id: id(window), title: title_of(window), class: program_of(window), active: Some(id(window)) == current, bounds: bounds_of(window) })
            .collect();
        windows.sort_by_key(|w| w.active);
        Ok(windows)
    }

    pub fn before(window: Window) -> Option<Window> {
        application_windows().into_iter().map(id).find(|other| *other != window)
    }

    pub fn of_classes(classes: &[String]) -> Vec<Window> {
        application_windows().into_iter().filter(|window| classes.contains(&program_of(*window))).map(id).collect()
    }

    pub fn activate(window: Window) -> Result<(), String> {
        let window = handle(window);
        unsafe {
            if IsIconic(window) != 0 {
                ShowWindow(window, SW_RESTORE);
            }
            // Allowed because Heyo is in front when it asks.
            if SetForegroundWindow(window) == 0 {
                return Err("Windows did not let the window come to the front.".into());
            }
        }
        Ok(())
    }

    pub fn close(window: Window) -> Result<(), String> {
        match unsafe { PostMessageW(handle(window), WM_CLOSE, 0, 0) } {
            0 => Err("the window is gone".into()),
            _ => Ok(()),
        }
    }

    fn rect(r: RECT) -> Rect {
        Rect { x: r.left, y: r.top, width: r.right - r.left, height: r.bottom - r.top }
    }

    /// A window's rectangle as Windows counts it: on Windows 10 and later
    /// that includes borders nobody sees, there to be grabbed for resizing.
    fn outer(window: HWND) -> Option<Rect> {
        let mut frame: RECT = unsafe { std::mem::zeroed() };
        (unsafe { GetWindowRect(window, &mut frame) } != 0).then(|| rect(frame))
    }

    /// The rectangle a window is seen to have, when the compositor says.
    fn seen(window: HWND) -> Option<Rect> {
        let mut frame: RECT = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<RECT>() as u32;
        (unsafe { DwmGetWindowAttribute(window, DWMWA_EXTENDED_FRAME_BOUNDS as u32, &mut frame as *mut RECT as *mut core::ffi::c_void, size) } == 0).then(|| rect(frame))
    }

    /// Where a window is seen to be, in physical pixels.
    fn bounds_of(window: HWND) -> Option<Rect> {
        let _physical = capture::PhysicalPixels::new();
        seen(window).or_else(|| outer(window))
    }

    /// A monitor, and what of it the taskbar leaves free.
    struct Display {
        monitor: HMONITOR,
        whole: Rect,
        work: Rect,
    }

    fn displays() -> Vec<Display> {
        unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, list: LPARAM) -> BOOL {
            let list = &mut *(list as *mut Vec<Display>);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                list.push(Display { monitor, whole: rect(info.rcMonitor), work: rect(info.rcWork) });
            }
            1
        }
        let mut list: Vec<Display> = Vec::new();
        unsafe { EnumDisplayMonitors(null_mut(), null(), Some(collect), &mut list as *mut _ as LPARAM) };
        list
    }

    /// Moves a window into a layout on its monitor, or onto the next one
    /// (Windows knows the monitors and their free areas itself).
    pub fn apply(window: Window, layout: Layout, _monitors: &[Rect]) -> Result<(), String> {
        let window = handle(window);
        // Positions are asked and given in physical pixels throughout.
        let _physical = capture::PhysicalPixels::new();
        unsafe {
            if IsWindow(window) == 0 {
                return Err("the window is gone".into());
            }
            if layout == Layout::Maximize {
                ShowWindow(window, SW_MAXIMIZE);
                return Ok(());
            }
            let displays = displays();
            let on = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
            let current = displays.iter().position(|display| display.monitor == on).ok_or("Windows did not say which display the window is on.")?;
            // A maximized or minimized window is neither moved nor resized.
            let was_maximized = IsZoomed(window) != 0;
            if was_maximized || IsIconic(window) != 0 {
                ShowWindow(window, SW_RESTORE);
            }
            // Its frame as restored, not the maximized one.
            let frame = outer(window).ok_or("the window is gone")?;
            let visible = seen(window).unwrap_or(frame);
            let to = match layout {
                Layout::NextDisplay => {
                    let monitors: Vec<Rect> = displays.iter().map(|display| display.whole).collect();
                    let next = next_display(&monitors, current).ok_or("There is only one display.")?;
                    carried(visible, displays[current].work, displays[next].work)
                }
                _ => target(layout, displays[current].work, visible),
            };
            let to = with_borders(to, frame, visible);
            if SetWindowPos(window, null_mut(), to.x, to.y, to.width.max(1), to.height.max(1), SWP_NOZORDER | SWP_NOACTIVATE) == 0 {
                // A window of a program run as administrator, for one.
                return Err("Windows did not let the window be moved.".into());
            }
            // What filled one display fills the next.
            if layout == Layout::NextDisplay && was_maximized {
                ShowWindow(window, SW_MAXIMIZE);
            }
        }
        Ok(())
    }
}

/// Presses and lets go of one key, as the keyboard's own would.
fn tap(code: u16) -> Result<(), String> {
    let keys = [key(code, false), key(code, true)];
    let sent = unsafe { SendInput(keys.len() as u32, keys.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == keys.len() { Ok(()) } else { Err("Windows did not accept the keystroke.".into()) }
}

/// Whether applications are shown dark (Settings → Personalisation → Colours).
fn toggle_dark() -> Result<Option<String>, String> {
    use crate::platform::system::tool;
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    let light = tool("reg", &["query", KEY, "/v", "AppsUseLightTheme"])?.contains("0x1");
    for value in ["AppsUseLightTheme", "SystemUsesLightTheme"] {
        tool("reg", &["add", KEY, "/v", value, "/t", "REG_DWORD", "/d", if light { "0" } else { "1" }, "/f"])?;
    }
    Ok(Some(if light { "Dark appearance" } else { "Light appearance" }.into()))
}

fn system(action: crate::platform::system::Action) -> Result<Option<String>, String> {
    use crate::platform::system::{tool, Action};
    // The volume keys: Windows shows the level itself.
    const VOLUME_MUTE: u16 = 0xAD;
    const VOLUME_DOWN: u16 = 0xAE;
    const VOLUME_UP: u16 = 0xAF;
    let done = |_| None;
    match action {
        Action::LockScreen => tool("rundll32.exe", &["user32.dll,LockWorkStation"]).map(done),
        Action::Sleep => tool("rundll32.exe", &["powrprof.dll,SetSuspendState", "0,1,0"]).map(done),
        Action::Restart => tool("shutdown", &["/r", "/t", "0"]).map(done),
        Action::ShutDown => tool("shutdown", &["/s", "/t", "0"]).map(done),
        Action::LogOut => tool("shutdown", &["/l"]).map(done),
        Action::EmptyTrash => tool("powershell", &["-NoProfile", "-Command", "Clear-RecycleBin -Force -ErrorAction SilentlyContinue"])
            .map(|_| Some("Recycle Bin emptied".into())),
        Action::ToggleDarkMode => toggle_dark(),
        Action::VolumeUp => tap(VOLUME_UP).map(|()| None),
        Action::VolumeDown => tap(VOLUME_DOWN).map(|()| None),
        Action::ToggleMute => tap(VOLUME_MUTE).map(|()| None),
    }
}

/// Moves files to the Recycle Bin, which only the shell knows how to do.
fn recycle(paths: &[String]) -> Result<(), String> {
    // The path travels in the environment, where nothing has to be quoted.
    const SCRIPT: &str = "Add-Type -AssemblyName Microsoft.VisualBasic; $p = $env:HEYO_TRASH; if (Test-Path -LiteralPath $p -PathType Container) { [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($p, 'OnlyErrorDialogs', 'SendToRecycleBin') } else { [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($p, 'OnlyErrorDialogs', 'SendToRecycleBin') }";
    for path in paths {
        let output = crate::platform::common::quiet("powershell")
            .args(["-NoProfile", "-Command", SCRIPT])
            .env("HEYO_TRASH", path)
            .output()
            .map_err(|e| format!("could not run PowerShell: {e}"))?;
        if !output.status.success() {
            return Err(format!("could not move {path} to the Recycle Bin: {}", String::from_utf8_lossy(&output.stderr).trim()));
        }
    }
    Ok(())
}

fn key(code: u16, up: bool) -> INPUT {
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: code, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 } } }
}

/// Presses Ctrl and a letter in the application in front.
fn control(letter: u16) -> Result<(), String> {
    // Let go of what the hotkey that started this may still hold down, so
    // that the application sees Ctrl+V and not Ctrl+Alt+V.
    let keys = [key(VK_MENU, true), key(VK_SHIFT, true), key(VK_CONTROL, false), key(letter, false), key(letter, true), key(VK_CONTROL, true)];
    let sent = unsafe { SendInput(keys.len() as u32, keys.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == keys.len() { Ok(()) } else { Err("Windows did not accept the keystroke.".into()) }
}

/// Presses Ctrl+V in the application in front.
pub fn paste() -> Result<(), String> {
    control(0x56)
}

/// Presses Ctrl+C there.
fn copy_keystroke() -> Result<(), String> {
    control(0x43)
}

/// Windows as a `Desktop`.
pub struct Windows;

impl crate::platform::desktop::Desktop for Windows {
    fn system(&self, action: crate::platform::system::Action) -> Result<Option<String>, String> {
        system(action)
    }

    fn open(&self, target: &str, application: Option<&str>) -> Result<(), String> {
        match application {
            Some(program) => crate::platform::system::start(program, &[target]),
            None => crate::platform::common::open(target),
        }
    }

    fn trash(&self, paths: &[String]) -> Result<(), String> {
        recycle(paths)
    }

    fn default_application(&self, _target: &str) -> Option<String> {
        None
    }

    fn supported(&self) -> crate::platform::desktop::Supported {
        use crate::platform::Support;
        crate::platform::desktop::Supported {
            windows: Support::Full,
            paste: Support::Full,
            // Asked of the application, or copied where it will not say:
            // slower than a selection that is simply there to read, and
            // not every application does either.
            selection: Support::Limited,
            clipboard_watch: Support::Full,
            hold_key: Support::Full,
            capture: Support::Full,
            // An ordinary fullscreen window.
            overlay: Support::Limited,
            // Not written: see `platform::typed`.
            typing: Support::None,
            // It needs the user's own keys heard: see `platform::typed`.
            rewriting: Support::None,
        }
    }

    fn keys_down(&self) -> Option<[u8; 32]> {
        Some(hold::keys_down())
    }

    fn active_window(&self) -> Option<Window> {
        windows::active_external()
    }

    fn active_class(&self) -> Option<String> {
        windows::active_class()
    }

    fn windows(&self, current: Option<Window>) -> Result<Vec<WindowInfo>, String> {
        windows::list(current)
    }

    fn window_before(&self, window: Window) -> Option<Window> {
        windows::before(window)
    }

    fn windows_of(&self, classes: &[String]) -> Vec<Window> {
        windows::of_classes(classes)
    }

    fn activate(&self, window: Window) -> Result<(), String> {
        windows::activate(window)
    }

    fn close(&self, window: Window) -> Result<(), String> {
        windows::close(window)
    }

    fn arrange(&self, window: Window, layout: Layout, monitors: &[Rect]) -> Result<(), String> {
        windows::apply(window, layout, monitors)
    }

    /// Ctrl+V, which terminals here take too.
    fn paste(&self, _terminal: bool) -> Result<(), String> {
        paste()
    }

    fn selected_text(&self) -> String {
        selection::selected_text()
    }

    fn watch_clipboard(&self, on_change: &mut dyn FnMut(Change)) -> Result<(), String> {
        clipboard::watch(on_change)
    }

    fn grab(&self, area: Area) -> Result<Shot, String> {
        capture::grab(area)
    }

    fn hold_keys(&self) -> Vec<&'static str> {
        hold::keys()
    }

    fn watch_hold(&self, key: &dyn Fn() -> String, on: &dyn Fn(Hold)) -> Result<(), String> {
        hold::watch(key, on)
    }

    fn own_focused(&self) -> Option<bool> {
        Some(own_in_front())
    }

    fn hotkeys(&self) -> Option<&dyn crate::platform::desktop::Hotkeys> {
        Some(&hotkeys::Registered)
    }
}
