//! The seam between Heyo and the desktop it runs on.
//!
//! `Desktop` is everything Heyo asks of a desktop session. Each session has
//! one adapter (`x11`, `gnome`, `win`, and `Generic` where there is none),
//! chosen once when first needed; tests put a fake in its place. What an
//! adapter cannot do it leaves at the default here, which says so: nothing
//! falls through to another desktop's tools by accident.
//!
//! The modules beside this one (`windows`, `input`, `capture`, …) are the
//! interface the rest of Heyo uses; they hold what is the same on every
//! desktop and ask the adapter for the rest.

use std::sync::OnceLock;

use super::capture::{Area, Shot};
use super::clipboard_watch::Change;
use super::keys::Hold;
use super::windows::{Layout, Rect, Window, WindowInfo};
use super::{unsupported, Support};

/// How well a desktop supports each thing Heyo does with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Supported {
    /// Listing, switching and moving other applications' windows.
    pub windows: Support,
    /// Sending the paste keystroke to the focused application.
    pub paste: Support,
    /// Reading the text selected in another application.
    pub selection: Support,
    /// Noticing copies as they happen (limited: polled, text only).
    pub clipboard_watch: Support,
    /// A single key held to talk.
    pub hold_key: Support,
    /// Reading the screen (limited: through the desktop's screenshot tool).
    pub capture: Support,
    /// Covering the whole screen for a selection (limited: an ordinary
    /// fullscreen window, which the desktop may animate).
    pub overlay: Support,
    /// Following what is typed in other applications, for snippets that
    /// expand as their keyword is typed.
    pub typing: Support,
    /// Writing text into another application piece by piece and correcting
    /// it where it stands by erasing and pasting: for dictated words that go
    /// in as they are spoken, where they cannot be composed.
    pub rewriting: Support,
    /// Composing text in another application as an input method does: for
    /// dictated words that go in as they are spoken.
    pub composing: Support,
}

impl Supported {
    pub const NOTHING: Supported = Supported {
        windows: Support::None,
        paste: Support::None,
        selection: Support::None,
        clipboard_watch: Support::None,
        hold_key: Support::None,
        capture: Support::None,
        overlay: Support::None,
        typing: Support::None,
        rewriting: Support::None,
        composing: Support::None,
    };
}

/// The standing the desktop itself is asked to give one of Heyo's windows
/// (`Desktop::stand_own`).
// Read only where a desktop is asked: on Linux.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy)]
pub struct OwnStanding {
    /// Above the other windows.
    pub above: bool,
    /// On every workspace.
    pub everywhere: bool,
    /// Whether it may have the keyboard: false, and the keyboard stays
    /// with the window that had it, or goes back to it at once.
    pub keyboard: bool,
    /// Where it is put, if the desktop is to put it somewhere.
    pub at: Option<OwnPlace>,
}

/// Where the desktop is asked to put a window: the rule is
/// `platform::place`'s, and the desktop's adapter knows its units.
// Read only where a desktop is asked: on Linux.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy)]
pub enum OwnPlace {
    /// At the bottom centre of the primary monitor, a tenth of its height
    /// up: a window of that size in logical pixels.
    BottomCentre { size: (f64, f64) },
    /// Over all of a monitor, given as a capture of it is: its area as the
    /// toolkit has it.
    Cover { area: Area },
    /// With its top left corner at a place of the screen, as a capture
    /// gives places (the toolkit's pixels): a window of that many pixels.
    At { at: (i32, i32), size: (u32, u32) },
}

pub trait Desktop: Send + Sync {
    fn supported(&self) -> Supported;

    // --- Other applications' windows ---------------------------------------

    /// Which keys are held down right now, as a 256-bit map by keycode.
    fn keys_down(&self) -> Option<[u8; 32]> {
        None
    }

    /// The active window, unless it is one of Heyo's own.
    fn active_window(&self) -> Option<Window> {
        None
    }

    /// The class of the active window, e.g. "firefox".
    fn active_class(&self) -> Option<String> {
        None
    }

    /// Application windows, most recently used first; `current` goes last.
    fn windows(&self, _current: Option<Window>) -> Result<Vec<WindowInfo>, String> {
        Err(unsupported("Listing windows"))
    }

    /// The windows that can be seen at present (not minimized, not on
    /// another workspace), the topmost first. Where the desktop cannot tell,
    /// all of them, most recently used first, which is nearly the same.
    fn windows_shown(&self) -> Result<Vec<WindowInfo>, String> {
        self.windows(None)
    }

    /// The application window used most recently before `window`.
    fn window_before(&self, _window: Window) -> Option<Window> {
        None
    }

    /// The windows whose class is one of `classes` (lowercase), most
    /// recently used first.
    fn windows_of(&self, _classes: &[String]) -> Vec<Window> {
        Vec::new()
    }

    fn activate(&self, _window: Window) -> Result<(), String> {
        Err(unsupported("Switching windows"))
    }

    fn close(&self, _window: Window) -> Result<(), String> {
        Err(unsupported("Closing windows"))
    }

    fn arrange(&self, _window: Window, _layout: Layout, _monitors: &[Rect]) -> Result<(), String> {
        Err(unsupported("Moving windows"))
    }

    // --- Input into the application in front -----------------------------------

    /// Presses the paste shortcut there (a terminal's, if it is one).
    fn paste(&self, _terminal: bool) -> Result<(), String> {
        Err(unsupported("Pasting"))
    }

    /// The text selected there, or nothing.
    fn selected_text(&self) -> String {
        String::new()
    }

    // --- The clipboard ------------------------------------------------------------

    /// The clipboard's text, or `None` if it has none. The default is the
    /// toolkit's way, which on Wayland fails unless Heyo is in front.
    fn clipboard_text(&self) -> Option<String> {
        super::clipboard_watch::toolkit_text()
    }

    /// The clipboard's image as RGBA where it can be read without Heyo
    /// being in front. `None`: read it the toolkit's way.
    fn clipboard_image(&self) -> Option<image::RgbaImage> {
        None
    }

    /// Puts text on the clipboard.
    fn set_clipboard_text(&self, text: &str) -> Result<(), String> {
        super::clipboard_watch::toolkit_set_text(text)
    }

    /// Puts rich text on the clipboard, with `text` for the applications
    /// that take none.
    fn set_clipboard_html(&self, html: &str, text: &str) -> Result<(), String> {
        super::clipboard_watch::toolkit_set_html(html, text)
    }

    /// Reports every clipboard change for as long as the session lasts, or
    /// says at once that it cannot.
    fn watch_clipboard(&self, _on_change: &mut dyn FnMut(Change)) -> Result<(), String> {
        Err(unsupported("Watching the clipboard"))
    }

    // --- The screen -----------------------------------------------------------------

    fn grab(&self, _area: Area) -> Result<Shot, String> {
        Err(unsupported("Capturing the screen"))
    }

    // --- A key held to talk ---------------------------------------------------------

    /// The keys (of `keys::KEYS`) that can be watched.
    fn hold_keys(&self) -> Vec<&'static str> {
        Vec::new()
    }

    fn watch_hold(&self, _key: &dyn Fn() -> String, _on: &dyn Fn(Hold)) -> Result<(), String> {
        Err(unsupported("A hold-to-talk key"))
    }

    // --- Heyo's own windows ---------------------------------------------------------

    /// Brings Heyo's window of that title to the front, where the window
    /// cannot do so itself. False when the desktop knows no such window
    /// (yet: a window is one a moment after it is asked to be shown); true
    /// also where there is nothing for the desktop to do.
    #[cfg(target_os = "linux")]
    fn activate_own(&self, _title: &str) -> bool {
        true
    }

    /// Whether one of Heyo's windows has the focus, where the desktop knows
    /// better than the toolkit. `None`: believe the toolkit.
    fn own_focused(&self) -> Option<bool> {
        None
    }

    /// Has the desktop itself give one of Heyo's windows, known by its
    /// title, the standing a window cannot say of itself there: from now
    /// on, for the window of that title that is there and each that comes.
    /// False where the desktop does no such thing, which is everywhere a
    /// window can say it of itself.
    fn stand_own(&self, _title: &str, _standing: &OwnStanding) -> bool {
        false
    }

    /// Where the monitor the pointer is on starts, in physical pixels as the
    /// toolkit gives monitors' positions. `None`: ask the toolkit, which on
    /// Wayland knows the pointer only while it is over one of Heyo's windows.
    fn pointer_monitor(&self) -> Option<(i32, i32)> {
        None
    }

    // --- The computer ---------------------------------------------------------------

    /// Does something to the computer; see [`super::system::run`].
    fn system(&self, action: super::system::Action) -> Result<Option<String>, String> {
        super::system::freedesktop(action)
    }

    /// Opens a file, folder or URL, with `application` when given.
    fn open(&self, target: &str, application: Option<&str>) -> Result<(), String> {
        super::system::freedesktop_open(target, application)
    }

    fn trash(&self, paths: &[String]) -> Result<(), String> {
        super::system::freedesktop_trash(paths)
    }

    /// The id of the application that opens `target` by default.
    fn default_application(&self, target: &str) -> Option<String> {
        super::system::freedesktop_default_application(target)
    }

    /// The speakers and microphones. A desktop without a way of its own is
    /// asked through the freedesktop tools, which only Linux has.
    fn audio_devices(&self) -> Result<Vec<super::system::AudioDevice>, String> {
        #[cfg(target_os = "linux")]
        return super::system::freedesktop_audio_devices();
        #[cfg(not(target_os = "linux"))]
        Err(unsupported("Choosing audio devices"))
    }

    fn use_audio_device(&self, _kind: super::system::AudioKind, _name: &str) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        return super::system::freedesktop_use_audio_device(_kind, _name);
        #[cfg(not(target_os = "linux"))]
        Err(unsupported("Choosing audio devices"))
    }

    /// The user's processes, the busiest first.
    fn processes(&self) -> Result<Vec<super::system::ProcessInfo>, String> {
        #[cfg(target_os = "linux")]
        return super::system::freedesktop_processes();
        #[cfg(not(target_os = "linux"))]
        Err(unsupported("Listing processes"))
    }

    fn end_process(&self, _pid: u32, _force: bool) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        return super::system::freedesktop_end_process(_pid, _force);
        #[cfg(not(target_os = "linux"))]
        Err(unsupported("Ending processes"))
    }

    // --- Global hotkeys -------------------------------------------------------------

    /// What sets and keeps the global hotkeys here, if anything does.
    fn hotkeys(&self) -> Option<&dyn Hotkeys> {
        None
    }

    // --- What is typed --------------------------------------------------------------

    /// What follows the typing in other applications here and replaces
    /// what was typed, if anything can.
    fn typing(&self) -> Option<&dyn super::typed::Typing> {
        None
    }

    // --- Composing text -------------------------------------------------------------

    /// Whether the session's input method is one Heyo composes through,
    /// and is running.
    fn can_compose(&self) -> bool {
        false
    }

    /// See [`super::composing::compose`].
    fn compose(&self, _own_keys: &[&str], _on: super::composing::Hearing) -> Result<Box<dyn super::composing::Composing>, String> {
        Err(unsupported("Composing text in another application"))
    }

    /// See [`super::composing::recover`].
    fn recover_input_method(&self) {}
}

/// Global hotkeys: each runs Heyo with some arguments.
pub trait Hotkeys: Send + Sync {
    /// The accelerator (as `<Control>space`) of what runs Heyo with these arguments.
    fn get(&self, target: &super::hotkeys::Target) -> Option<String>;
    /// Sets it; an empty accelerator removes the hotkey.
    fn set(&self, target: &super::hotkeys::Target, accelerator: &str) -> Result<(), String>;
    /// Accelerators of all `--open <id>` hotkeys, by item id.
    fn items(&self) -> std::collections::HashMap<String, String>;
    /// Begins acting on the hotkeys, where Heyo itself must.
    fn start(&self, _app: &tauri::AppHandle) {}
}

/// A session Heyo has no adapter for: what generic tools can do, no more.
pub struct Generic;

impl Desktop for Generic {
    fn supported(&self) -> Supported {
        // On Linux: wtype or ydotool, the primary selection, polling, and
        // the desktop's own screenshot tool. Elsewhere nothing.
        if cfg!(target_os = "linux") {
            Supported { paste: Support::Limited, selection: Support::Limited, clipboard_watch: Support::Limited, capture: Support::Limited, overlay: Support::Limited, ..Supported::NOTHING }
        } else {
            Supported::NOTHING
        }
    }

    #[cfg(target_os = "linux")]
    fn paste(&self, terminal: bool) -> Result<(), String> {
        super::input::paste_with_tools(terminal)
    }

    #[cfg(target_os = "linux")]
    fn selected_text(&self) -> String {
        super::input::primary_selection()
    }

    #[cfg(target_os = "linux")]
    fn grab(&self, area: Area) -> Result<Shot, String> {
        // A Wayland compositor that lets its screen be read outright
        // (Sway and the other wlroots ones): asked, in the output's own
        // pixels. Else the desktop's screenshot tool, where there is one.
        if super::session() == super::Session::Wayland && super::wlr::offered() {
            return super::wlr::grab(area);
        }
        super::capture::grab_with_tool(area)
    }

    #[cfg(target_os = "linux")]
    fn pointer_monitor(&self) -> Option<(i32, i32)> {
        if super::session() == super::Session::Wayland { super::wlr::pointed() } else { None }
    }

    #[cfg(target_os = "linux")]
    fn hotkeys(&self) -> Option<&dyn Hotkeys> {
        super::hotkeys::gsettings()
    }
}

/// The adapter for the session Heyo was started in.
fn choose() -> Box<dyn Desktop> {
    #[cfg(windows)]
    return Box::new(super::win::Windows);
    #[cfg(target_os = "linux")]
    match super::session() {
        super::Session::X11 => return Box::new(super::x11::X11),
        // An X server is usually there on Wayland too (XWayland), but it
        // knows only the applications still using X11.
        super::Session::Wayland if super::gnome::available() => return Box::new(super::gnome::Gnome),
        _ => {}
    }
    #[allow(unreachable_code)]
    Box::new(Generic)
}

/// The desktop Heyo is running on.
pub fn desktop() -> &'static dyn Desktop {
    #[cfg(test)]
    if let Some(fake) = fake::current() {
        return fake;
    }
    static CHOSEN: OnceLock<Box<dyn Desktop>> = OnceLock::new();
    CHOSEN.get_or_init(choose).as_ref()
}

/// A desktop for tests: it has the windows it is given and remembers what
/// was asked of it.
#[cfg(test)]
pub mod fake {
    use std::cell::Cell;
    use std::sync::Mutex;

    use super::super::composing::{Composing, Hearing, Stir};
    use super::super::typed::{Lost, Typed, Typing, Writing};
    use super::*;

    thread_local!(static CURRENT: Cell<Option<&'static Fake>> = const { Cell::new(None) });

    pub(super) fn current() -> Option<&'static dyn Desktop> {
        CURRENT.with(|current| current.get()).map(|fake| fake as &dyn Desktop)
    }

    #[derive(Default)]
    pub struct Fake {
        /// Most recently used first; the first is in front.
        pub windows: Mutex<Vec<WindowInfo>>,
        /// "activate 3", "paste terminal", … in the order asked.
        pub asked: Mutex<Vec<String>>,
        pub selection: Mutex<String>,
        /// The text on the clipboard.
        pub clipboard: Mutex<Option<String>>,
        /// Why the paste shortcut cannot be pressed, if it cannot.
        pub paste_fails: Mutex<Option<String>>,
        /// What it supports, for a test of a desktop that cannot do what
        /// this one usually can (windows, pasting, the selection).
        pub supported: Mutex<Option<Supported>>,
        /// A window of Heyo's own is in front, so none of these is active.
        pub heyo_in_front: Mutex<bool>,
        /// The text before the cursor in the window in front: what was
        /// pasted there and written piece by piece, and what the test's
        /// user typed.
        pub field: Mutex<String>,
        /// The user has touched the window in front (typed, clicked) since
        /// writing into it began.
        pub touched: Mutex<bool>,
        /// What becomes of the pieces written next instead of their going
        /// in, the first first.
        pub losing: Mutex<std::collections::VecDeque<Lost>>,
        /// The session has an input method to compose through,
        pub input_method: Mutex<bool>,
        /// and why nothing can be composed in the window in front, if not.
        pub no_field: Mutex<Option<String>>,
        /// What is shown in the window in front as being composed, after
        /// its text.
        pub composing: Mutex<String>,
        /// Who hears what happens in the field that is composed in.
        hearing: Mutex<Option<Hearing>>,
    }

    impl Fake {
        /// A desktop with these windows (id, class), the first in front.
        pub fn with_windows(windows: &[(Window, &str)]) -> &'static Fake {
            let fake = Fake::default();
            *fake.windows.lock().unwrap() =
                windows.iter().map(|(id, class)| WindowInfo { id: *id, title: format!("{class} {id}"), class: (*class).to_owned(), active: false, bounds: None }).collect();
            Box::leak(Box::new(fake))
        }

        /// Runs `test` with this as the desktop (on this thread).
        pub fn during<T>(&'static self, test: impl FnOnce() -> T) -> T {
            CURRENT.with(|current| current.set(Some(self)));
            let result = test();
            CURRENT.with(|current| current.set(None));
            result
        }

        pub fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }

        fn note(&self, what: String) {
            self.asked.lock().unwrap().push(what);
        }

        /// The user types into the window in front.
        pub fn user_types(&self, text: &str) {
            self.field.lock().unwrap().push_str(text);
            *self.touched.lock().unwrap() = true;
        }

        /// What the window in front holds before its cursor.
        pub fn field(&self) -> String {
            self.field.lock().unwrap().clone()
        }

        /// Erases and writes as an application does at Backspace and paste.
        fn edit(&self, erase: usize, text: &str) {
            let mut field = self.field.lock().unwrap();
            for _ in 0..erase {
                field.pop();
            }
            field.push_str(text);
        }

        /// Something happens in the field that is composed in, as its
        /// input method would hear of it. Whoever composes there hears it
        /// first, as with a real one.
        pub fn stirs(&'static self, stir: Stir) {
            // The field lets go of what was being composed.
            if matches!(stir, Stir::Reset | Stir::Left | Stir::Gone) {
                self.composing.lock().unwrap().clear();
            }
            let hearing = self.hearing.lock().unwrap().take();
            if let Some(mut hearing) = hearing {
                hearing(stir, &mut Pen { fake: self, ends: false });
                *self.hearing.lock().unwrap() = Some(hearing);
            }
        }

        /// The user presses a key in the field that is composed in, which
        /// types `text`.
        pub fn user_presses(&'static self, text: &str) {
            self.stirs(Stir::Key);
            assert_eq!(*self.composing.lock().unwrap(), "", "a key reached the field while text was being composed");
            self.user_types(text);
        }
    }

    /// Composes in the fake's window in front.
    struct Pen {
        fake: &'static Fake,
        /// Composing ends with this one.
        ends: bool,
    }

    impl Composing for Pen {
        fn show(&mut self, text: &str) -> Result<(), String> {
            self.fake.note(format!("compose {text}"));
            text.clone_into(&mut self.fake.composing.lock().unwrap());
            Ok(())
        }

        fn commit(&mut self, text: &str) -> Result<(), String> {
            self.fake.note(format!("commit {text}"));
            self.fake.field.lock().unwrap().push_str(text);
            Ok(())
        }
    }

    impl Drop for Pen {
        fn drop(&mut self) {
            if self.ends {
                self.fake.note("composing ends".into());
                *self.fake.hearing.lock().unwrap() = None;
            }
        }
    }

    impl Typing for Fake {
        fn watch(&self, _wanted: &dyn Fn() -> bool, _on: &mut dyn FnMut(Typed)) -> Result<(), String> {
            Err(unsupported("Following what is typed"))
        }

        fn replace(&self, erase: usize, text: &str, terminal: bool) -> Result<(), String> {
            self.note(format!("replace {erase} with {text}{}", if terminal { " terminal" } else { "" }));
            *self.clipboard.lock().unwrap() = Some(text.to_owned());
            self.edit(erase, text);
            Ok(())
        }

        fn write(&self, hotkeys: &[String], own_presses: std::sync::Arc<std::sync::atomic::AtomicU32>) -> Result<Box<dyn Writing>, String> {
            let fake = CURRENT.with(|current| current.get()).ok_or("no desktop")?;
            fake.note(format!("write{}", hotkeys.iter().map(|hotkey| format!(" {hotkey}")).collect::<String>()));
            *fake.touched.lock().unwrap() = false;
            Ok(Box::new(Hand { fake, own_presses, known: 0 }))
        }

        fn composing(&self) -> bool {
            false
        }
    }

    /// Writes into the fake's window in front, until its user touches it.
    struct Hand {
        fake: &'static Fake,
        /// Presses on Heyo's own windows, and how many of them were noted.
        own_presses: std::sync::Arc<std::sync::atomic::AtomicU32>,
        known: u32,
    }

    impl Writing for Hand {
        fn replace(&mut self, erase: usize, text: &str) -> Result<(), Lost> {
            let own = self.own_presses.load(std::sync::atomic::Ordering::SeqCst);
            if own != self.known {
                self.known = own;
                self.fake.note(format!("own presses {own}"));
            }
            if *self.fake.touched.lock().unwrap() {
                return Err(Lost::untouched("a key was pressed"));
            }
            let lost = self.fake.losing.lock().unwrap().pop_front();
            let (erased, put) = lost.as_ref().map_or((erase, true), |lost| (lost.erased, lost.put));
            self.fake.note(format!("rewrite {erased} with {}", if put { text } else { "nothing" }));
            if put && !text.is_empty() {
                *self.fake.clipboard.lock().unwrap() = Some(text.to_owned());
            }
            self.fake.edit(erased, if put { text } else { "" });
            match lost {
                Some(lost) => {
                    // Whatever went wrong, nothing more is written: unless
                    // the piece was only put off.
                    *self.fake.touched.lock().unwrap() = !lost.later;
                    Err(lost)
                }
                None => Ok(()),
            }
        }
    }

    impl Desktop for Fake {
        fn supported(&self) -> Supported {
            let composing = if *self.input_method.lock().unwrap() { Support::Full } else { Support::None };
            self.supported.lock().unwrap().unwrap_or(Supported { windows: Support::Full, paste: Support::Full, selection: Support::Full, rewriting: Support::Full, composing, ..Supported::NOTHING })
        }

        fn can_compose(&self) -> bool {
            *self.input_method.lock().unwrap()
        }

        fn compose(&self, own_keys: &[&str], on: Hearing) -> Result<Box<dyn Composing>, String> {
            let fake = CURRENT.with(|current| current.get()).ok_or("no desktop")?;
            if let Some(why) = fake.no_field.lock().unwrap().clone() {
                return Err(why);
            }
            fake.note(format!("composing begins{}", own_keys.iter().map(|key| format!(" {key}")).collect::<String>()));
            *fake.hearing.lock().unwrap() = Some(on);
            Ok(Box::new(Pen { fake, ends: true }))
        }

        fn active_window(&self) -> Option<Window> {
            if *self.heyo_in_front.lock().unwrap() {
                return None;
            }
            self.windows.lock().unwrap().first().map(|w| w.id)
        }

        fn active_class(&self) -> Option<String> {
            self.windows.lock().unwrap().first().map(|w| w.class.clone())
        }

        fn windows(&self, current: Option<Window>) -> Result<Vec<WindowInfo>, String> {
            let mut windows = self.windows.lock().unwrap().clone();
            windows.sort_by_key(|w| Some(w.id) == current);
            Ok(windows)
        }

        fn window_before(&self, window: Window) -> Option<Window> {
            self.windows.lock().unwrap().iter().map(|w| w.id).find(|id| *id != window)
        }

        fn windows_of(&self, classes: &[String]) -> Vec<Window> {
            self.windows.lock().unwrap().iter().filter(|w| classes.contains(&w.class.to_lowercase())).map(|w| w.id).collect()
        }

        /// The window comes to the front, as on a real desktop.
        fn activate(&self, window: Window) -> Result<(), String> {
            self.note(format!("activate {window}"));
            let mut windows = self.windows.lock().unwrap();
            let at = windows.iter().position(|w| w.id == window).ok_or("the window is gone")?;
            let front = windows.remove(at);
            windows.insert(0, front);
            Ok(())
        }

        fn close(&self, window: Window) -> Result<(), String> {
            self.note(format!("close {window}"));
            self.windows.lock().unwrap().retain(|w| w.id != window);
            Ok(())
        }

        fn arrange(&self, window: Window, layout: Layout, _monitors: &[Rect]) -> Result<(), String> {
            self.note(format!("arrange {window} {layout:?}"));
            Ok(())
        }

        /// What is on the clipboard goes into the window in front.
        fn paste(&self, terminal: bool) -> Result<(), String> {
            self.note(if terminal { "paste terminal".into() } else { "paste".into() });
            if let Some(why) = self.paste_fails.lock().unwrap().clone() {
                return Err(why);
            }
            let pasted = self.clipboard.lock().unwrap().clone().unwrap_or_default();
            self.edit(0, &pasted);
            Ok(())
        }

        fn selected_text(&self) -> String {
            self.selection.lock().unwrap().clone()
        }

        fn clipboard_text(&self) -> Option<String> {
            self.clipboard.lock().unwrap().clone()
        }

        fn set_clipboard_text(&self, text: &str) -> Result<(), String> {
            self.note(format!("copy {text}"));
            *self.clipboard.lock().unwrap() = Some(text.to_owned());
            Ok(())
        }

        fn set_clipboard_html(&self, html: &str, text: &str) -> Result<(), String> {
            self.note(format!("copy {text} as {html}"));
            *self.clipboard.lock().unwrap() = Some(text.to_owned());
            Ok(())
        }

        fn system(&self, action: crate::platform::system::Action) -> Result<Option<String>, String> {
            self.note(format!("system {action:?}"));
            Ok(None)
        }

        fn open(&self, target: &str, application: Option<&str>) -> Result<(), String> {
            self.note(format!("open {target} with {}", application.unwrap_or("the default")));
            Ok(())
        }

        fn trash(&self, paths: &[String]) -> Result<(), String> {
            self.note(format!("trash {}", paths.join(" ")));
            Ok(())
        }

        fn default_application(&self, _target: &str) -> Option<String> {
            None
        }

        fn typing(&self) -> Option<&dyn Typing> {
            Some(self)
        }

        fn audio_devices(&self) -> Result<Vec<crate::platform::system::AudioDevice>, String> {
            self.note("audio devices".into());
            Ok(Vec::new())
        }

        fn use_audio_device(&self, kind: crate::platform::system::AudioKind, name: &str) -> Result<(), String> {
            self.note(format!("use {kind:?} {name}"));
            Ok(())
        }

        fn processes(&self) -> Result<Vec<crate::platform::system::ProcessInfo>, String> {
            self.note("processes".into());
            Ok(Vec::new())
        }

        fn end_process(&self, pid: u32, _force: bool) -> Result<(), String> {
            self.note(format!("end {pid}"));
            Ok(())
        }
    }
}
