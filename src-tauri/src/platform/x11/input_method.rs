//! Whether an input method may be composing: turning what the keys say
//! into other text, as for Chinese or Japanese. While one is, the keys
//! pressed are not what ends up in the document, and a snippet's keyword
//! "typed" that way was not typed at all.
//!
//! There is no one place to ask. What can be asked is which input source is
//! chosen: of GNOME (its settings), of IBus (`ibus engine`), of Fcitx 5
//! (over D-Bus). A source that is a plain keyboard layout composes nothing.
//! Anything else, and every case where the answer could not be had, counts
//! as composing: the snippet is then not expanded, which costs little.
//!
//! This is asked only when a keyword has just been typed, not at every key.
//!
//! Limits: an input method Heyo does not know how to ask (SCIM, uim, Fcitx
//! 4, …) always counts as composing. One that is not announced in the
//! environment and is none of the above is not noticed at all.

use std::process::Command;

/// Whom to ask.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Method {
    /// No input method is set up.
    None,
    /// GNOME chooses the source, also IBus's.
    Gnome,
    Ibus,
    Fcitx,
    /// One that cannot be asked.
    Other,
}

/// Decides from the environment as desktops set it: the desktop's name
/// (`XDG_CURRENT_DESKTOP`) and the input-method modules named for the
/// toolkits (`GTK_IM_MODULE`, `QT_IM_MODULE`, `XMODIFIERS`).
pub(super) fn method(desktop: &str, modules: &[&str]) -> Method {
    let modules: Vec<String> = modules.iter().map(|module| module.trim().to_lowercase()).collect();
    let named = |name: &str| modules.iter().any(|module| module.contains(name));
    // Values that mean the toolkit's own handling of dead keys and Compose.
    const PLAIN: &[&str] = &["", "none", "@im=none", "@im=local", "xim", "simple", "gtk-im-context-simple", "compose"];
    if named("fcitx") {
        Method::Fcitx
    } else if desktop.to_lowercase().contains("gnome") {
        Method::Gnome
    } else if named("ibus") {
        Method::Ibus
    } else if modules.iter().any(|module| !PLAIN.contains(&module.as_str())) {
        Method::Other
    } else {
        Method::None
    }
}

/// The kinds of the input sources in a list as `gsettings get` prints it:
/// `[('xkb', 'us'), ('ibus', 'libpinyin')]`, or `@a(ss) []` for none.
fn kinds(printed: &str) -> Vec<&str> {
    printed.split("('").skip(1).filter_map(|rest| rest.split('\'').next()).collect()
}

/// From GNOME's settings: the sources set up, the same most recently used
/// first, and whether each window has a source of its own. `None`: cannot tell.
pub(super) fn gnome_composing(sources: &str, recent: &str, per_window: &str) -> Option<bool> {
    let all = kinds(sources);
    // Nothing but keyboard layouts to choose from.
    if all.iter().all(|kind| *kind == "xkb") && sources.contains('[') {
        return Some(false);
    }
    // With a source per window the list does not say which this window has.
    if per_window.trim() != "false" {
        return None;
    }
    Some(*kinds(recent).first()? != "xkb")
}

/// From `ibus engine`: `xkb:us::eng` is a keyboard layout.
pub(super) fn ibus_composing(engine: &str) -> Option<bool> {
    let engine = engine.trim();
    (!engine.is_empty() && !engine.contains(char::is_whitespace)).then(|| !engine.starts_with("xkb:"))
}

/// From Fcitx 5's current input method: `keyboard-us` is a keyboard layout.
pub(super) fn fcitx_composing(input_method: &str) -> Option<bool> {
    let input_method = input_method.trim();
    (!input_method.is_empty()).then(|| !input_method.starts_with("keyboard-"))
}

/// What a tool prints, if it ran and succeeded.
fn printed(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).stdin(std::process::Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn ask_gnome() -> Option<bool> {
    const SCHEMA: &str = "org.gnome.desktop.input-sources";
    let get = |key: &str| printed("gsettings", &["get", SCHEMA, key]);
    let sources = get("sources")?;
    // Usually that is all it takes: asking costs time somebody is typing in.
    if let Some(answer) = gnome_composing(&sources, "", "") {
        return Some(answer);
    }
    gnome_composing(&sources, &get("mru-sources")?, &get("per-window")?)
}

fn ask_fcitx() -> Option<bool> {
    let bus = zbus::blocking::Connection::session().ok()?;
    let reply = bus.call_method(Some("org.fcitx.Fcitx5"), "/controller", Some("org.fcitx.Fcitx.Controller1"), "CurrentInputMethod", &()).ok()?;
    fcitx_composing(&reply.body().deserialize::<String>().ok()?)
}

/// Whether an input method may be composing at this moment.
pub fn composing() -> bool {
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    let (desktop, gtk, qt, x) = (var("XDG_CURRENT_DESKTOP"), var("GTK_IM_MODULE"), var("QT_IM_MODULE"), var("XMODIFIERS"));
    let answer = match method(&desktop, &[&gtk, &qt, &x]) {
        Method::None => Some(false),
        Method::Gnome => ask_gnome(),
        Method::Ibus => printed("ibus", &["engine"]).and_then(|engine| ibus_composing(&engine)),
        Method::Fcitx => ask_fcitx(),
        Method::Other => None,
    };
    answer != Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_says_whom_to_ask() {
        assert_eq!(method("", &["", "", ""]), Method::None);
        assert_eq!(method("XFCE", &["", "", "@im=none"]), Method::None);
        assert_eq!(method("i3", &["xim", "", "@im=local"]), Method::None);
        // GNOME starts IBus for every session and chooses the source itself.
        assert_eq!(method("ubuntu:GNOME", &["ibus", "ibus", "@im=ibus"]), Method::Gnome);
        assert_eq!(method("GNOME", &["", "", ""]), Method::Gnome);
        assert_eq!(method("KDE", &["ibus", "ibus", "@im=ibus"]), Method::Ibus);
        // Fcitx is asked wherever it runs, on GNOME too.
        assert_eq!(method("ubuntu:GNOME", &["fcitx", "fcitx", "@im=fcitx"]), Method::Fcitx);
        assert_eq!(method("KDE", &["", "fcitx5", ""]), Method::Fcitx);
        assert_eq!(method("XFCE", &["", "", "@im=SCIM"]), Method::Other);
        assert_eq!(method("XFCE", &["uim", "", ""]), Method::Other);
    }

    #[test]
    fn on_gnome_the_source_used_last_is_the_one_in_use() {
        let both = "[('xkb', 'us'), ('ibus', 'libpinyin')]";
        assert_eq!(gnome_composing(both, "[('xkb', 'us'), ('ibus', 'libpinyin')]", "false"), Some(false));
        assert_eq!(gnome_composing(both, "[('ibus', 'libpinyin'), ('xkb', 'us')]", "false"), Some(true));
        // Never switched: there is no telling which is in use.
        assert_eq!(gnome_composing(both, "@a(ss) []", "false"), None);
        // A source per window: the list is not about this window.
        assert_eq!(gnome_composing(both, "[('xkb', 'us'), ('ibus', 'libpinyin')]", "true"), None);
        assert_eq!(gnome_composing(both, "[('xkb', 'us')]", ""), None);
    }

    #[test]
    fn only_keyboard_layouts_to_choose_from_compose_nothing() {
        assert_eq!(gnome_composing("[('xkb', 'us'), ('xkb', 'de')]", "@a(ss) []", "true"), Some(false));
        assert_eq!(gnome_composing("[('xkb', 'us')]", "[('ibus', 'anthy')]", "false"), Some(false));
        // None set up: GNOME uses the system's layout.
        assert_eq!(gnome_composing("@a(ss) []", "@a(ss) []", "false"), Some(false));
        // Not an answer at all.
        assert_eq!(gnome_composing("", "", ""), None);
    }

    #[test]
    fn a_plain_layout_is_told_from_an_input_method() {
        assert_eq!(ibus_composing("xkb:us::eng\n"), Some(false));
        assert_eq!(ibus_composing("libpinyin\n"), Some(true));
        assert_eq!(ibus_composing("anthy"), Some(true));
        assert_eq!(ibus_composing(""), None);
        assert_eq!(ibus_composing("No engine is set.\n"), None);
        assert_eq!(fcitx_composing("keyboard-us"), Some(false));
        assert_eq!(fcitx_composing("keyboard-de-nodeadkeys"), Some(false));
        assert_eq!(fcitx_composing("pinyin"), Some(true));
        assert_eq!(fcitx_composing("mozc"), Some(true));
        assert_eq!(fcitx_composing(""), None);
    }
}
