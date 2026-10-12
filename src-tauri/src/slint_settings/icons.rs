//! The symbols the settings window draws for a Raycast icon name.
//!
//! The host names icons as extensions do, by Raycast's names
//! (`clipboard-16`), and the web interface draws each as a Lucide icon.
//! This window draws the same ones: the icons are in `ui/icons/lucide.tsv`
//! and the names that differ in `ui/icons/raycast.tsv`, both written from
//! what the web interface uses by `ui/icons/make.mjs`.

use std::collections::HashMap;
use std::sync::OnceLock;

const LUCIDE: &str = include_str!("ui/icons/lucide.tsv");
const RAYCAST: &str = include_str!("ui/icons/raycast.tsv");

fn table(text: &'static str) -> HashMap<&'static str, &'static str> {
    text.lines().filter_map(|line| line.split_once('\t')).collect()
}

/// The SVG of the symbol drawn for `name`, if it names one: a Raycast icon
/// name, with or without its `-16`, as `lucideIcon` of `src/raycast/Image.tsx`
/// finds it.
pub(crate) fn svg(name: &str) -> Option<String> {
    static TABLES: OnceLock<(HashMap<&str, &str>, HashMap<&str, &str>)> = OnceLock::new();
    let (lucide, raycast) = TABLES.get_or_init(|| (table(LUCIDE), table(RAYCAST)));
    let base = name.strip_suffix("-16").unwrap_or(name);
    let unfilled = base.strip_suffix("-filled").unwrap_or(base);
    let drawn = lucide.get(*raycast.get(base).or_else(|| raycast.get(unfilled)).unwrap_or(&unfilled))?;
    Some(format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{drawn}</svg>"##))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_raycast_name_is_a_lucide_icon() {
        // By its own name, by another, filled or not.
        assert!(svg("keyboard-16").unwrap().contains("<rect"));
        assert_eq!(svg("cog-16"), svg("cog"));
        assert_eq!(svg("app-window-grid-3x3-16"), svg("grid-3x3"));
        assert_eq!(svg("microphone-16"), svg("mic"));
        assert_eq!(svg("star-filled-16"), svg("star"));
        assert_eq!(svg("no-such-icon-16"), None);
        assert_eq!(svg("/usr/share/icons/firefox.png"), None);
    }

    /// Every symbol the window itself names is there.
    #[test]
    fn the_windows_own_symbols_are_there() {
        for (_, _, symbol) in super::super::model::PAGES {
            assert!(svg(symbol).is_some(), "{symbol}");
        }
        for symbol in ["app-window-grid-3x3-16", "plug-16", "app-window-16", "store-16"] {
            assert!(svg(symbol).is_some(), "{symbol}");
        }
    }
}
