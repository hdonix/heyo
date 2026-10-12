//! Giving up on Slint without giving up on Heyo.
//!
//! Heyo is built to abort on a panic (`panic = "abort"` in the release
//! profile), so a panic on Slint's thread cannot be caught: after the panic
//! hook returns, the process is gone, and with it the palette, the
//! clipboard history and a dictation in progress. So the hook does not
//! return on that thread. It says that Slint is gone, takes Slint's window
//! off the screen, and keeps the thread where it is for as long as Heyo
//! runs; `open` then answers that it cannot, and the settings are the web
//! ones for the rest of the run.
//!
//! This is sound only because Slint's thread holds nothing of the host's
//! when it stops: it never runs the host's commands (`window::Live::ask`).
//! What it does hold is Slint's own, which nothing else uses again. A
//! thread Slint starts for itself (for assistive technology, for the
//! desktop's settings) is not covered: a panic there still ends Heyo.
//!
//! Taking the window away is the one thing here that differs by platform,
//! because a window belongs to the connection or thread that made it:
//!
//! - X11: the window is winit's own connection's. Shutting that socket
//!   down makes the server drop the client and all it had on screen, and
//!   stops the server queueing events for a thread that will never read.
//! - Windows: a window is its thread's, and a thread with windows must
//!   answer their messages, or programs that broadcast one wait on it. The
//!   thread's windows are handed to the system's own procedure and
//!   destroyed, and the thread then does nothing but answer messages.
//! - Wayland: not handled. The surface stays as it was last drawn.

use std::sync::atomic::{AtomicI64, AtomicU8, Ordering};
use std::sync::Once;

use slint::winit_030::winit;

/// The name of Slint's thread, by which the hook knows it.
pub(super) const THREAD: &str = "heyo-slint";

/// Where Slint is in this run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum State {
    /// Not asked for yet.
    Idle,
    Starting,
    Running,
    /// It failed or panicked: not to be asked again.
    Gone,
}

const STATES: [State; 4] = [State::Idle, State::Starting, State::Running, State::Gone];

static STATE: AtomicU8 = AtomicU8::new(0);

/// What is needed to take the window away from another place than winit:
/// the socket of its X11 connection, or its handle on Windows. -1 for none.
static NATIVE: AtomicI64 = AtomicI64::new(-1);

pub(super) fn state() -> State {
    STATES[STATE.load(Ordering::SeqCst) as usize]
}

/// Moves from one state to the next; false if Slint was not in `from`.
pub(super) fn moved(from: State, to: State) -> bool {
    STATE.compare_exchange(from as u8, to as u8, Ordering::SeqCst, Ordering::SeqCst).is_ok()
}

pub(super) fn gone() {
    STATE.store(State::Gone as u8, Ordering::SeqCst);
}

/// Why Slint is gone, for the user to be told. Written where there is time
/// to: a thread that panicked only tries.
static REASON: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub(super) fn gone_because(reason: &str) {
    gone();
    if let Ok(mut why) = REASON.try_lock() {
        if why.is_empty() {
            *why = reason.to_string();
        }
    }
}

pub(super) fn reason() -> String {
    let why = REASON.lock().map(|why| why.clone()).unwrap_or_default();
    if why.is_empty() { "it stopped".into() } else { why }
}

/// Notes what `take_away` will need, while the window can still be asked.
pub(super) fn remember(window: &winit::window::Window) {
    if let Some(native) = native(window) {
        NATIVE.store(native, Ordering::SeqCst);
    }
}

#[cfg(target_os = "linux")]
fn native(window: &winit::window::Window) -> Option<i64> {
    use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
    // Xlib's connection number is the socket. (On Wayland there is none.)
    let RawDisplayHandle::Xlib(handle) = window.display_handle().ok()?.as_raw() else { return None };
    let xlib = x11_dl::xlib::Xlib::open().ok()?;
    Some(unsafe { (xlib.XConnectionNumber)(handle.display?.as_ptr().cast()) } as i64)
}

#[cfg(windows)]
fn native(window: &winit::window::Window) -> Option<i64> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as i64),
        _ => None,
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn native(_window: &winit::window::Window) -> Option<i64> {
    None
}

/// Takes Slint's window off the screen and keeps this thread, which is
/// Slint's and has panicked, from ever going on.
#[cfg(target_os = "linux")]
fn rest() -> ! {
    let socket = NATIVE.load(Ordering::SeqCst);
    if socket >= 0 {
        // Not closed: the number must not come to mean another file while
        // Xlib still has it written down.
        unsafe { libc::shutdown(socket as i32, libc::SHUT_RDWR) };
    }
    loop {
        std::thread::park();
    }
}

#[cfg(windows)]
fn rest() -> ! {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DefWindowProcW, DestroyWindow, DispatchMessageW, EnumThreadWindows, FindWindowExW, GetMessageW, GetWindowThreadProcessId, SetWindowLongPtrW, GWLP_WNDPROC, HWND_MESSAGE, MSG};

    /// winit's procedure would run winit, which is where the panic was.
    unsafe fn disown(window: HWND) {
        SetWindowLongPtrW(window, GWLP_WNDPROC, DefWindowProcW as *const () as isize);
    }
    unsafe extern "system" fn close(window: HWND, _: LPARAM) -> BOOL {
        disown(window);
        DestroyWindow(window);
        1
    }
    unsafe {
        let thread = GetCurrentThreadId();
        // The windows one sees, then the ones only messages are sent to.
        EnumThreadWindows(thread, Some(close), 0);
        let mut hidden: HWND = std::ptr::null_mut();
        loop {
            hidden = FindWindowExW(HWND_MESSAGE, hidden, std::ptr::null(), std::ptr::null());
            if hidden.is_null() {
                break;
            }
            if GetWindowThreadProcessId(hidden, std::ptr::null_mut()) == thread {
                disown(hidden);
            }
        }
        let mut message: MSG = std::mem::zeroed();
        loop {
            if GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                DispatchMessageW(&message);
            } else {
                std::thread::park();
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn rest() -> ! {
    loop {
        std::thread::park();
    }
}

/// Sees to it that a panic on Slint's thread ends Slint and not Heyo.
pub(super) fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let before = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic| {
            before(panic);
            if std::thread::current().name() == Some(THREAD) {
                gone_because("what draws it failed; Heyo's log has the details");
                eprintln!("heyo: Slint's thread panicked. Its windows are taken away; the settings and the dictation pill are not shown again until Heyo is restarted");
                rest();
            }
        }));
    });
}

/// For testing that the screenshot overlay cannot trap the user: with
/// `HEYO_SLINT_PANIC_IN_DRAG` set, Slint's thread panics at the tenth
/// movement of a drag on it; with `HEYO_SLINT_STALL_IN_DRAG`, it stops
/// there for a minute instead.
pub(super) fn test_in_drag(movements: u32) {
    if movements != 10 {
        return;
    }
    if std::env::var_os("HEYO_SLINT_PANIC_IN_DRAG").is_some() {
        panic!("HEYO_SLINT_PANIC_IN_DRAG asked for this panic");
    }
    if std::env::var_os("HEYO_SLINT_STALL_IN_DRAG").is_some() {
        eprintln!("heyo: HEYO_SLINT_STALL_IN_DRAG asked Slint's thread to stop for a minute");
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

/// For testing that Slint cannot start at all: with `HEYO_SLINT_NO_START`
/// set, it does not.
pub(super) fn test_no_start() -> Result<(), String> {
    if std::env::var_os("HEYO_SLINT_NO_START").is_some() { Err("HEYO_SLINT_NO_START asked for this".into()) } else { Ok(()) }
}

/// For testing the above: with `HEYO_SLINT_PANIC_AFTER` set to a number of
/// milliseconds, Slint's thread panics that long after its loop starts.
pub(super) fn test_panic() {
    let Some(after) = std::env::var("HEYO_SLINT_PANIC_AFTER").ok().and_then(|ms| ms.parse().ok()) else { return };
    slint::Timer::single_shot(std::time::Duration::from_millis(after), || panic!("HEYO_SLINT_PANIC_AFTER asked for this panic"));
}
