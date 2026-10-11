//! Everything that talks to the desktop: other applications' windows, input
//! into them, focus, notifications. The rest of Heyo says what it wants
//! done; how depends on the session (X11, Wayland) and lives here.
//!
//! Each capability is a module of plain functions: what the rest of Heyo
//! uses. They hold what is the same everywhere and ask the session's
//! adapter for the rest (`desktop::Desktop`, chosen once; see there). What
//! an adapter cannot do, its function says (an error, or "nothing") and
//! `capabilities` reports, so features can be hidden rather than fail.

use std::sync::OnceLock;

pub mod capture;
pub mod clipboard_watch;
pub mod common;
pub mod desktop;
pub mod focus;
pub mod hotkeys;
pub mod input;
pub mod keys;
pub mod overlay;
pub mod system;
pub mod typed;
pub mod windows;
#[cfg(target_os = "linux")]
mod gnome;
#[cfg(windows)]
pub(crate) mod win;
#[cfg(any(windows, test))]
mod win_rules;
#[cfg(target_os = "linux")]
mod x11;

/// The kind of desktop session Heyo is running in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Session {
    X11,
    Wayland,
    /// No display at all, or one this code does not know.
    Unknown,
}

/// Decides the session from the environment, as desktops set it.
fn detect(session_type: Option<&str>, wayland_display: bool, display: bool) -> Session {
    match session_type {
        Some("wayland") => Session::Wayland,
        Some("x11") => Session::X11,
        // Not set (started from a terminal multiplexer, say): go by the sockets.
        _ if wayland_display => Session::Wayland,
        _ if display => Session::X11,
        _ => Session::Unknown,
    }
}

pub fn session() -> Session {
    static SESSION: OnceLock<Session> = OnceLock::new();
    *SESSION.get_or_init(|| {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        detect(var("XDG_SESSION_TYPE").as_deref(), var("WAYLAND_DISPLAY").is_some(), var("DISPLAY").is_some())
    })
}

/// How well the session supports something.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum Support {
    Full,
    /// Works, with less: slower, coarser, or needing an extra tool.
    Limited,
    None,
}

/// A key that can be held to talk.
#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct HoldKey {
    #[ts(type = "string")]
    value: &'static str,
    #[ts(type = "string")]
    title: &'static str,
}

/// What the running session can do, so features that cannot work are
/// hidden or explained instead of failing when used.
#[derive(serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Capabilities {
    #[ts(type = "\"x11\" | \"wayland\" | \"windows\" | \"unknown\"")]
    pub session: &'static str,
    /// Listing, switching and moving other applications' windows.
    pub windows: Support,
    /// Sending the paste keystroke to the focused application.
    pub paste: Support,
    /// Reading the text selected in another application.
    pub selection: Support,
    /// Noticing copies as they happen (limited: polled, text only).
    pub clipboard_watch: Support,
    /// Global hotkeys set from Heyo.
    pub hotkeys: Support,
    /// A single key held to talk.
    pub hold_key: Support,
    /// Reading the screen (limited: through the desktop's screenshot tool).
    pub capture: Support,
    /// Covering the whole screen for a selection (limited: an ordinary
    /// fullscreen window, which the desktop may animate).
    pub overlay: Support,
    /// Expanding snippets as their keyword is typed in other applications.
    pub snippet_expansion: Support,
    /// The keys offered for hold-to-talk.
    pub hold_keys: Vec<HoldKey>,
    /// Heyo's GNOME Shell extension: "none" where it has no use, else
    /// "missing", "outdated" (an older one than Heyo carries; it still
    /// serves if it is running), "pending" (in place; loaded at the next
    /// login) or "active".
    #[ts(type = "\"none\" | \"missing\" | \"outdated\" | \"pending\" | \"active\"")]
    pub extension: &'static str,
}

/// Installs Heyo's GNOME Shell extension for this user.
pub fn install_extension() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    return gnome::extension::install();
    #[cfg(not(target_os = "linux"))]
    Err(unsupported("The GNOME Shell extension"))
}

pub fn capabilities() -> Capabilities {
    let desktop = desktop::desktop();
    let supported = desktop.supported();
    let offered = keys::available();
    Capabilities {
        session: match session() {
            _ if cfg!(windows) => "windows",
            Session::X11 => "x11",
            Session::Wayland => "wayland",
            Session::Unknown => "unknown",
        },
        windows: supported.windows,
        paste: supported.paste,
        selection: supported.selection,
        clipboard_watch: supported.clipboard_watch,
        hotkeys: if hotkeys::supported() { Support::Full } else { Support::None },
        hold_key: supported.hold_key,
        capture: supported.capture,
        overlay: supported.overlay,
        snippet_expansion: if typed::supported() { supported.typing } else { Support::None },
        hold_keys: offered.into_iter().map(|(value, title)| HoldKey { value, title }).collect(),
        #[cfg(target_os = "linux")]
        extension: gnome::extension::state().name(),
        #[cfg(not(target_os = "linux"))]
        extension: "none",
    }
}

/// Whether a root search item can work in this session. Commands that
/// cannot are left out of the search instead of failing when chosen.
pub fn item_available(id: &str) -> bool {
    let needs_windows = id.starts_with(crate::root::WINDOW_PREFIX) || id == "cmd:@heyo:switch-windows";
    if id == crate::root::SNIPPET_EXPANSION_ID {
        return typed::supported();
    }
    !needs_windows || desktop::desktop().supported().windows != Support::None
}

/// The error for something the session cannot do.
pub(crate) fn unsupported(what: &str) -> String {
    match session() {
        Session::Wayland => format!("{what} is not supported on Wayland yet."),
        _ => format!("{what} is not supported in this session."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_declared_session_type_wins_over_the_sockets() {
        // A Wayland session has an X server too (XWayland).
        assert_eq!(detect(Some("wayland"), true, true), Session::Wayland);
        assert_eq!(detect(Some("x11"), false, true), Session::X11);
        assert_eq!(detect(None, true, true), Session::Wayland);
        assert_eq!(detect(Some("tty"), false, true), Session::X11);
        assert_eq!(detect(None, false, false), Session::Unknown);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn each_desktop_says_what_it_can_do() {
        use desktop::Desktop;
        // No adapter: only what generic tools do.
        let generic = desktop::Generic.supported();
        assert_eq!(generic.windows, Support::None);
        assert_eq!(generic.hold_key, Support::None);
        assert_eq!(generic.capture, Support::Limited);
        // With the Shell extension, most of it comes back.
        let gnome = gnome::Gnome.supported();
        assert_eq!(gnome.windows, Support::Full);
        assert_eq!(gnome.paste, Support::Full);
        assert_eq!(gnome.hold_key, Support::Limited);
        let x11 = x11::X11.supported();
        assert_eq!(x11.windows, Support::Full);
        assert_eq!(x11.overlay, Support::Full);
        // Typing can only be followed where an application may listen to
        // the keyboard: X11.
        assert_eq!(x11.typing, Support::Full);
        assert!(x11::X11.typing().is_some());
        assert_eq!(gnome.typing, Support::None);
        assert!(gnome::Gnome.typing().is_none());
        assert!(desktop::Generic.typing().is_none());
    }

    /// What an adapter does not do says so, and reaches for nothing else.
    #[test]
    fn a_desktop_without_an_adapter_refuses_rather_than_guesses() {
        struct Bare;
        impl desktop::Desktop for Bare {
            fn supported(&self) -> desktop::Supported {
                desktop::Supported::NOTHING
            }
        }
        use desktop::Desktop;
        assert!(Bare.windows(None).is_err());
        assert!(Bare.activate(1).is_err());
        assert!(Bare.paste(false).is_err());
        assert!(Bare.grab(capture::Area { x: 0, y: 0, width: 1, height: 1, scale: 1.0 }).is_err());
        assert!(Bare.watch_clipboard(&mut |_| {}).is_err());
        assert!(Bare.hotkeys().is_none());
        assert_eq!(Bare.selected_text(), "");
    }

    #[test]
    fn the_rest_of_heyo_reaches_the_desktop_through_the_modules() {
        let fake = desktop::fake::Fake::with_windows(&[(1, "firefox"), (2, "Code"), (3, "firefox")]);
        fake.during(|| {
            assert_eq!(windows::active_class().as_deref(), Some("firefox"));
            assert_eq!(windows::of_classes(&["firefox".into()]), [1, 3]);
            assert_eq!(windows::before(1), Some(2));
            windows::activate(3).unwrap();
            assert_eq!(windows::active_external(), Some(3));
            // The window one came from goes last.
            assert_eq!(windows::list(Some(3)).unwrap().iter().map(|w| w.id).collect::<Vec<_>>(), [1, 2, 3]);
            input::paste_keystroke(std::time::Duration::ZERO);
            assert!(item_available("cmd:@heyo:switch-windows"));
            // The clipboard is the desktop's, with no application to ask.
            assert_eq!(clipboard_watch::read_text(), None);
            clipboard_watch::write_text("copied").unwrap();
            assert_eq!(clipboard_watch::read_text().as_deref(), Some("copied"));
            clipboard_watch::write_html("<b>bold</b>", "bold").unwrap();
            assert_eq!(clipboard_watch::read_text().as_deref(), Some("bold"));
        });
        assert_eq!(fake.asked(), ["activate 3", "paste", "copy copied", "copy bold as <b>bold</b>"]);
        // A terminal in front gets the terminal's paste.
        let fake = desktop::fake::Fake::with_windows(&[(1, "org.gnome.Ptyxis")]);
        fake.during(|| input::paste_keystroke(std::time::Duration::ZERO));
        assert_eq!(fake.asked(), ["paste terminal"]);
    }
}
