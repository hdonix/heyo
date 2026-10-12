//! Hotkeys in the settings window: a key press as a GTK accelerator
//! (what the host binds), and an accelerator as the key caps it is shown as.
//!
//! This is `src/hotkey.tsx` again, for a window that is not a web view: the
//! same keys by the same names, read off the keyboard the same way, by
//! where a key is and not by what it types. winit names a key's place with
//! the names the web has for it (`KeyA`, `Digit1`, `Minus`), so the table
//! is the same table, and a test holds it to the one in `hotkey.tsx`.

/// Keys that are neither letters, digits nor function keys: where the key
/// is (`KeyboardEvent.code`) and what GTK calls it.
pub(crate) const CODE_NAMES: &[(&str, &str)] = &[
    ("Space", "space"),
    ("Enter", "Return"),
    ("Tab", "Tab"),
    ("Backspace", "BackSpace"),
    ("Delete", "Delete"),
    ("Insert", "Insert"),
    ("Home", "Home"),
    ("End", "End"),
    ("PageUp", "Page_Up"),
    ("PageDown", "Page_Down"),
    ("ArrowUp", "Up"),
    ("ArrowDown", "Down"),
    ("ArrowLeft", "Left"),
    ("ArrowRight", "Right"),
    ("Backquote", "grave"),
    ("Minus", "minus"),
    ("Equal", "equal"),
    ("BracketLeft", "bracketleft"),
    ("BracketRight", "bracketright"),
    ("Backslash", "backslash"),
    ("Semicolon", "semicolon"),
    ("Quote", "apostrophe"),
    ("Comma", "comma"),
    ("Period", "period"),
    ("Slash", "slash"),
];

/// The modifiers held with a key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Held {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub superkey: bool,
}

impl Held {
    /// As they are written before the key of an accelerator.
    fn written(self) -> String {
        [(self.control, "<Control>"), (self.shift, "<Shift>"), (self.alt, "<Alt>"), (self.superkey, "<Super>")].iter().filter(|(held, _)| *held).map(|(_, name)| *name).collect()
    }

    /// As the key caps of a field that is recording.
    pub(crate) fn caps(self) -> Vec<String> {
        parts(&self.written()).into_iter().filter(|part| !part.is_empty()).collect()
    }
}

fn function_key(code: &str) -> bool {
    code.strip_prefix('F').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// What GTK calls the key at `code`, if it is one a hotkey can end in.
fn key_name(code: &str) -> Option<String> {
    if let Some(letter) = code.strip_prefix("Key").filter(|rest| rest.len() == 1 && rest.bytes().all(|b| b.is_ascii_uppercase())) {
        return Some(letter.to_ascii_lowercase());
    }
    if let Some(digit) = code.strip_prefix("Digit").filter(|rest| rest.len() == 1 && rest.bytes().all(|b| b.is_ascii_digit())) {
        return Some(digit.to_string());
    }
    if function_key(code) {
        return Some(code.to_string());
    }
    CODE_NAMES.iter().find(|(known, _)| *known == code).map(|(_, name)| name.to_string())
}

/// A GTK accelerator for the key at `code` pressed with `held`, or `None`
/// for a bare modifier or a key that needs a modifier.
pub(crate) fn accelerator(held: Held, code: &str) -> Option<String> {
    let key = key_name(code)?;
    let modifiers = held.written();
    if modifiers.is_empty() && !function_key(&key) {
        return None;
    }
    Some(modifiers + &key)
}

/// An accelerator as key caps: `<Control><Shift>v` is Ctrl, Shift, V.
pub(crate) fn parts(accelerator: &str) -> Vec<String> {
    let mut caps = Vec::new();
    let mut rest = accelerator;
    while let Some(inner) = rest.strip_prefix('<') {
        let Some(end) = inner.find('>') else { break };
        let name = &inner[..end];
        caps.push(match name {
            "Control" | "Primary" | "Ctrl" => "Ctrl".to_string(),
            "Meta" => "Super".to_string(),
            other => other.to_string(),
        });
        rest = &inner[end + 1..];
    }
    let mut letters = rest.chars();
    caps.push(match (rest, letters.next(), letters.next()) {
        ("space", ..) => "Space".to_string(),
        (_, Some(only), None) => only.to_uppercase().collect(),
        _ => rest.replace('_', " "),
    });
    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table here is the one the palette's recorder has.
    #[test]
    fn the_keys_are_those_of_the_web_recorder() {
        let source = include_str!("../../../src/hotkey.tsx");
        let table = source.split_once("const CODE_NAMES: Record<string, string> = {").and_then(|(_, rest)| rest.split_once("};")).expect("CODE_NAMES is no longer in src/hotkey.tsx").0;
        let theirs: Vec<(String, String)> = table
            .lines()
            .filter_map(|line| line.trim().trim_end_matches(',').split_once(": "))
            .map(|(code, name)| (code.to_string(), name.trim_matches('"').to_string()))
            .collect();
        let ours: Vec<(String, String)> = CODE_NAMES.iter().map(|(code, name)| (code.to_string(), name.to_string())).collect();
        assert_eq!(ours, theirs);
    }

    #[test]
    fn a_key_press_is_an_accelerator() {
        let (none, control) = (Held::default(), Held { control: true, ..Held::default() });
        assert_eq!(accelerator(control, "KeyV").as_deref(), Some("<Control>v"));
        assert_eq!(accelerator(Held { control: true, shift: true, alt: true, superkey: true }, "Digit1").as_deref(), Some("<Control><Shift><Alt><Super>1"));
        assert_eq!(accelerator(Held { superkey: true, ..none }, "Space").as_deref(), Some("<Super>space"));
        assert_eq!(accelerator(Held { alt: true, ..none }, "PageDown").as_deref(), Some("<Alt>Page_Down"));
        // A function key is a hotkey by itself; any other key needs a modifier.
        assert_eq!(accelerator(none, "F9").as_deref(), Some("F9"));
        assert_eq!(accelerator(none, "KeyV"), None);
        assert_eq!(accelerator(none, "Space"), None);
        // A modifier alone, and keys there is no name for.
        assert_eq!(accelerator(control, "ControlLeft"), None);
        assert_eq!(accelerator(control, "Escape"), None);
        assert_eq!(accelerator(control, "Fn"), None);
    }

    #[test]
    fn an_accelerator_is_key_caps() {
        assert_eq!(parts("<Control><Shift>v"), ["Ctrl", "Shift", "V"]);
        assert_eq!(parts("<Primary><Meta>space"), ["Ctrl", "Super", "Space"]);
        assert_eq!(parts("<Alt>Page_Down"), ["Alt", "Page Down"]);
        assert_eq!(parts("<Super>F9"), ["Super", "F9"]);
        assert_eq!(parts("F9"), ["F9"]);
        assert_eq!(Held { control: true, superkey: true, ..Held::default() }.caps(), ["Ctrl", "Super"]);
        assert_eq!(Held::default().caps(), Vec::<String>::new());
    }
}
