//! Watching one key being held, through a low-level keyboard hook.
//!
//! Windows calls such a hook for every key, before any application sees
//! it, and waits for the answer: so the hook lives on a thread that does
//! nothing else, only notes what happened and passes the key on at once.
//! Nothing is swallowed; the key keeps its meaning everywhere. What the
//! presses and releases amount to is decided elsewhere (`keys::Watch`), on
//! the thread that asked. There is a hook only while a key is configured,
//! and it goes with its thread (and, should Heyo die, with the process).

use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PeekMessageW, PostQuitMessage, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, LLKHF_UP, MSG, PM_NOREMOVE, WH_KEYBOARD_LL, WM_QUIT, WM_USER,
};

use crate::platform::keys::{Hold, Key, Watch};

/// The virtual-key code of each key (`keys::KEYS`).
const CODES: &[(&str, u32)] = &[
    ("alt_r", 0xA5),
    ("alt_l", 0xA4),
    ("control_r", 0xA3),
    ("control_l", 0xA2),
    ("shift_r", 0xA1),
    ("super_r", 0x5C),
    ("menu", 0x5D),
    ("caps_lock", 0x14),
    ("f13", 0x7C),
];

const LEFT_CONTROL: u32 = 0xA2;
/// The scan code of the Left Ctrl that Windows makes up before every Right
/// Alt on layouts where that key is AltGr. It is no key anyone pressed.
const ALTGR_CONTROL: u32 = 0x21D;

/// The keys that can be watched: all of them.
pub fn keys() -> Vec<&'static str> {
    CODES.iter().map(|(name, _)| *name).collect()
}

/// Whether Windows has the key down right now.
fn is_down(code: u32) -> bool {
    unsafe { GetAsyncKeyState(code as i32) < 0 }
}

/// Which keys are down, one bit per virtual-key code. (Codes below 8 are
/// mouse buttons.)
pub fn keys_down() -> [u8; 32] {
    let mut keys = [0u8; 32];
    for code in 8..255u32 {
        if is_down(code) {
            keys[code as usize / 8] |= 1 << (code % 8);
        }
    }
    keys
}

/// A key the keyboard itself pressed or let go.
struct Event {
    code: u32,
    pressed: bool,
}

thread_local! {
    /// Where the hook on this thread reports to.
    static EVENTS: RefCell<Option<Sender<Event>>> = const { RefCell::new(None) };
}

/// Called by Windows for every key, on the thread that set the hook. It
/// must answer quickly, never fail, and always hand the key on.
unsafe extern "system" fn on_key(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam != 0 {
        let key = &*(lparam as *const KBDLLHOOKSTRUCT);
        // Keys "pressed" by programs are not the user's doing: Heyo's own
        // paste lets go of held modifiers for a moment.
        let real = key.flags & LLKHF_INJECTED == 0 && !(key.vkCode == LEFT_CONTROL && key.scanCode == ALTGR_CONTROL);
        if real {
            let event = Event { code: key.vkCode, pressed: key.flags & LLKHF_UP == 0 };
            let heard = EVENTS.try_with(|events| events.try_borrow().is_ok_and(|events| events.as_ref().is_some_and(|events| events.send(event).is_ok())));
            // Nobody is listening any more: end this thread, and the hook with it.
            if !heard.unwrap_or(false) {
                PostQuitMessage(0);
            }
        }
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

/// The hook and the thread it lives on. Dropping it removes both.
struct Hook {
    thread: u32,
    events: Receiver<Event>,
    done: Option<std::thread::JoinHandle<()>>,
}

impl Hook {
    fn set() -> Result<Hook, String> {
        let (report, events) = channel::<Event>();
        let (started, start) = channel::<Result<u32, String>>();
        let done = std::thread::Builder::new()
            .name("hold-key".into())
            .spawn(move || unsafe {
                let mut message: MSG = std::mem::zeroed();
                // Gives the thread its message queue, so that it can be told to stop.
                PeekMessageW(&mut message, null_mut(), WM_USER, WM_USER, PM_NOREMOVE);
                let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_key), GetModuleHandleW(null()), 0);
                if hook.is_null() {
                    let _ = started.send(Err("Windows did not let the keyboard be watched for the hold-to-talk key.".into()));
                    return;
                }
                EVENTS.with(|events| *events.borrow_mut() = Some(report));
                let _ = started.send(Ok(GetCurrentThreadId()));
                // The hook is called from in here. Nothing else happens on
                // this thread, so a key never waits for Heyo.
                while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {}
                UnhookWindowsHookEx(hook);
                EVENTS.with(|events| *events.borrow_mut() = None);
            })
            .map_err(|e| format!("could not start the hold-to-talk watcher: {e}"))?;
        match start.recv() {
            Ok(Ok(thread)) => Ok(Hook { thread, events, done: Some(done) }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("the hold-to-talk watcher stopped as it started".into()),
        }
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        unsafe { PostThreadMessageW(self.thread, WM_QUIT, 0, 0) };
        if let Some(done) = self.done.take() {
            let _ = done.join();
        }
    }
}

/// Whether a key other than the watched one is down. `down` is what the
/// events say; Windows is asked to confirm, so that one release that went
/// unseen (while a UAC prompt had the keyboard, say) cannot disable the
/// key for good.
fn others_held(down: &mut [bool; 256]) -> bool {
    let mut any = false;
    for (code, held) in down.iter_mut().enumerate().filter(|(_, held)| **held) {
        *held = is_down(code as u32);
        any |= *held;
    }
    any
}

/// Watches for as long as the session lasts. `key` is asked for the
/// configured key (it can change at any time); `on` hears what happens.
pub fn watch(key: &dyn Fn() -> String, on: &dyn Fn(Hold)) -> Result<(), String> {
    /// How often the setting is looked at while nothing happens.
    const IDLE: Duration = Duration::from_millis(500);
    /// How often Windows is asked whether the key is still down, while recording.
    const CHECK: Duration = Duration::from_millis(60);
    /// The key shown up for this long has been let go, whatever the events said.
    const GONE: Duration = Duration::from_millis(400);

    let mut configured = String::new();
    let mut code: Option<u32> = None;
    let mut hook: Option<Hook> = None;
    let mut watch = Watch::default();
    // Other keys that are down, as far as the events say.
    let mut down = [false; 256];
    // While recording: when Windows last agreed that the key is down.
    let mut seen_down = Instant::now();

    loop {
        let setting = key();
        if setting != configured {
            code = CODES.iter().find(|(name, _)| *name == setting).map(|(_, code)| *code);
            if !setting.is_empty() && code.is_none() {
                eprintln!("heyo: {setting} is not a key that can be held to talk on Windows");
            }
            eprintln!("heyo: push-to-talk key: {}", if setting.is_empty() { "none" } else { &setting });
            configured = setting;
            // Whatever the old key was doing is over.
            if let Some(hold) = watch.lost() {
                on(hold);
            }
            down = [false; 256];
        }
        let Some(code) = code else {
            // No key, no hook: the keyboard is none of Heyo's business.
            hook = None;
            std::thread::sleep(IDLE);
            continue;
        };
        if hook.is_none() {
            hook = Some(Hook::set()?);
        }
        let Some(Hook { events, .. }) = &hook else { continue };

        let now = Instant::now();
        if let Some(hold) = watch.tick(now) {
            seen_down = now;
            eprintln!("heyo: push-to-talk: recording");
            on(hold);
        }
        // A release can go unseen. Windows' own picture of the keyboard is
        // the second opinion: when it has shown the key up for a while,
        // that is that. (Only "for a while": a paste lifts modifiers.)
        if watch.recording() {
            if is_down(code) {
                seen_down = now;
            } else if now.saturating_duration_since(seen_down) >= GONE {
                eprintln!("heyo: push-to-talk: key no longer held; finishing");
                if let Some(hold) = watch.lost() {
                    on(hold);
                }
            }
        }

        let wait = match watch.due() {
            Some(due) => due.saturating_duration_since(Instant::now()),
            None if watch.recording() => CHECK,
            None => IDLE,
        };
        let event = match events.recv_timeout(wait) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                // The hook's thread ended; start over with a new one, unhurriedly.
                eprintln!("heyo: the hold-to-talk watcher stopped; starting it again");
                hook = None;
                if let Some(hold) = watch.lost() {
                    on(hold);
                }
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
        };
        let talk = event.code == code;
        if !talk {
            down[event.code.min(255) as usize] = event.pressed;
        }
        let others = talk && event.pressed && others_held(&mut down);
        if let Some(hold) = watch.key(Instant::now(), if talk { Key::Talk } else { Key::Other }, event.pressed, others) {
            eprintln!("heyo: push-to-talk: {}", if hold == Hold::Released { "released; finishing" } else { "another key; cancelled" });
            on(hold);
        }
    }
}
