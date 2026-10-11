//! Global hotkeys on Windows: registered with the system by a thread that
//! waits for them, and kept in `hotkeys.json` beside the settings.
//!
//! Accelerators are written as elsewhere in Heyo (`<Control><Alt>c`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use tauri::AppHandle;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_APP, WM_HOTKEY};

/// Accelerator → the arguments Heyo is "run" with when it is pressed.
type Hotkeys = HashMap<String, Vec<String>>;

static HOTKEYS: Mutex<Option<Hotkeys>> = Mutex::new(None);
/// The thread that holds the registrations, once it runs.
static THREAD: AtomicU32 = AtomicU32::new(0);
/// Asks that thread to register the hotkeys anew.
const RELOAD: u32 = WM_APP + 1;

fn file() -> PathBuf {
    crate::paths::config().join("hotkeys.json")
}

fn with<T>(f: impl FnOnce(&mut Hotkeys) -> T) -> T {
    let mut guard = HOTKEYS.lock().unwrap();
    let hotkeys = guard.get_or_insert_with(|| {
        std::fs::read_to_string(file())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            // Until one is chosen: Alt+Space shows Heyo.
            .unwrap_or_else(|| HashMap::from([("<Alt>space".to_owned(), Vec::new())]))
    });
    f(hotkeys)
}

fn save(hotkeys: &Hotkeys) -> Result<(), String> {
    let path = file();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, serde_json::to_string_pretty(hotkeys).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// The modifiers and the virtual key code of an accelerator.
fn parse(accelerator: &str) -> Option<(u32, u32)> {
    let mut modifiers = MOD_NOREPEAT;
    let mut rest = accelerator;
    while let Some(start) = rest.strip_prefix('<') {
        let end = start.find('>')?;
        modifiers |= match start[..end].to_lowercase().as_str() {
            "control" | "ctrl" | "primary" => MOD_CONTROL,
            "alt" | "mod1" => MOD_ALT,
            "shift" => MOD_SHIFT,
            "super" | "mod4" | "meta" => MOD_WIN,
            _ => return None,
        };
        rest = &start[end + 1..];
    }
    let name = rest.to_lowercase();
    let code = match name.as_str() {
        "space" => 0x20,
        "return" | "enter" => 0x0D,
        "tab" => 0x09,
        "escape" => 0x1B,
        "backspace" => 0x08,
        "delete" => 0x2E,
        "insert" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "page_up" | "prior" => 0x21,
        "page_down" | "next" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "semicolon" => 0xBA,
        "equal" | "plus" => 0xBB,
        "comma" => 0xBC,
        "minus" => 0xBD,
        "period" => 0xBE,
        "slash" => 0xBF,
        "grave" => 0xC0,
        "bracketleft" => 0xDB,
        "backslash" => 0xDC,
        "bracketright" => 0xDD,
        "apostrophe" => 0xDE,
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.as_str()) {
                (Some(c), "") if c.is_ascii_alphanumeric() => c.to_ascii_uppercase() as u32,
                (Some('f'), number) => 0x6F + number.parse::<u32>().ok().filter(|n| (1..=24).contains(n))?,
                _ => return None,
            }
        }
    };
    Some((modifiers, code))
}

/// Hotkeys registered with Windows by Heyo itself.
pub struct Registered;

impl crate::platform::desktop::Hotkeys for Registered {
    fn get(&self, target: &crate::platform::hotkeys::Target) -> Option<String> {
        get(&target.args)
    }

    fn set(&self, target: &crate::platform::hotkeys::Target, accelerator: &str) -> Result<(), String> {
        set(&target.args, accelerator)
    }

    fn items(&self) -> HashMap<String, String> {
        item_hotkeys()
    }

    fn start(&self, app: &AppHandle) {
        start(app.clone());
    }
}

/// Registers the hotkeys and acts on them, for as long as Heyo runs.
fn start(app: AppHandle) {
    std::thread::spawn(move || unsafe {
        THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
        let mut registered: Vec<Vec<String>> = Vec::new();
        let register = |registered: &mut Vec<Vec<String>>| {
            for id in 0..registered.len() {
                UnregisterHotKey(std::ptr::null_mut(), id as i32);
            }
            registered.clear();
            for (accelerator, args) in with(|hotkeys| hotkeys.clone()) {
                let Some((modifiers, code)) = parse(&accelerator) else { continue };
                if RegisterHotKey(std::ptr::null_mut(), registered.len() as i32, modifiers, code) == 0 {
                    eprintln!("heyo: the hotkey {accelerator} is taken by another program");
                } else {
                    eprintln!("heyo: hotkey {accelerator}: {}", if args.is_empty() { "show or hide".to_owned() } else { args.join(" ") });
                }
                // Kept either way, so that ids stay the places in the list.
                registered.push(args);
            }
        };
        register(&mut registered);
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            if message.message == RELOAD {
                register(&mut registered);
            } else if message.message == WM_HOTKEY {
                let Some(args) = registered.get(message.wParam) else { continue };
                eprintln!("heyo: hotkey pressed: {}", if args.is_empty() { "show or hide".to_owned() } else { args.join(" ") });
                // As if Heyo had been run again with these arguments.
                let args: Vec<String> = std::iter::once("heyo".to_owned()).chain(args.iter().cloned()).collect();
                let app = app.clone();
                let _ = app.clone().run_on_main_thread(move || {
                    if !crate::handle_args(&app, &args) {
                        crate::palette::toggle(&app);
                    }
                });
            }
        }
    });
}

fn reload() {
    let thread = THREAD.load(Ordering::SeqCst);
    if thread != 0 {
        unsafe { PostThreadMessageW(thread, RELOAD, 0, 0) };
    }
}

/// The accelerator that runs Heyo with these arguments.
fn get(args: &[String]) -> Option<String> {
    with(|hotkeys| hotkeys.iter().find(|(_, a)| a.as_slice() == args).map(|(accelerator, _)| accelerator.clone()))
}

/// Accelerators of all `--open <id>` hotkeys, by item id.
fn item_hotkeys() -> HashMap<String, String> {
    with(|hotkeys| hotkeys.iter().filter(|(_, args)| args.len() == 2 && args[0] == "--open").map(|(accelerator, args)| (args[1].clone(), accelerator.clone())).collect())
}

/// Sets the accelerator for running Heyo with these arguments; an empty one
/// removes the hotkey.
fn set(args: &[String], accelerator: &str) -> Result<(), String> {
    if !accelerator.is_empty() && parse(accelerator).is_none() {
        return Err(format!("{accelerator} cannot be a hotkey on Windows."));
    }
    let saved = with(|hotkeys| {
        if !accelerator.is_empty() && hotkeys.get(accelerator).is_some_and(|other| other.as_slice() != args) {
            return Err(format!("{accelerator} is already used by another Heyo hotkey."));
        }
        hotkeys.retain(|_, a| a.as_slice() != args);
        if !accelerator.is_empty() {
            hotkeys.insert(accelerator.to_owned(), args.to_vec());
        }
        save(hotkeys)
    });
    reload();
    saved
}
