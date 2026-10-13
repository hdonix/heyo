//! The fonts Heyo carries, for what Slint draws.
//!
//! A web view finds a font for every character by itself; Slint's text
//! stack (Parley, with Fontique choosing the fonts) asks the system, and
//! what the system answers for Chinese depends on the system: an English
//! Windows hands out a font without the simplified characters, and 发, 这
//! and 议 come out as boxes. So Heyo brings its own: Inter for Latin, as
//! the web interface has, and Noto Sans SC for Chinese. They are in the
//! program itself, registered when Slint starts and put first in what
//! `sans-serif` and `system-ui` mean, which is what Slint sets text in
//! when no family is named. The system's fonts stay behind them, for what
//! these two have not: Korean, emoji, rare characters.
//!
//! Noto Sans SC is not the file Google ships: its line is given Inter's
//! height and its characters are at 94%, so that a row is as tall and its
//! text as large in Chinese as in English (`scripts/fonts/fit-chinese.py`
//! made it, and says why). A newer download is to go through that script.

use std::sync::Arc;

use slint::fontique_011::fontique::{Blob, Collection, FamilyId, GenericFamily};

/// The fonts, in the order a character is looked for in them. Each file's
/// licence is beside it in `src-tauri/fonts/`.
const CARRIED: &[(&str, &[u8])] = &[
    ("Inter", include_bytes!("../../fonts/Inter-Variable.ttf")),
    ("Noto Sans SC", include_bytes!("../../fonts/NotoSansSC-Variable.otf")),
];

/// The generic families Slint sets text in when none is named
/// (`FALLBACK_FAMILIES` in i-slint-common), and the one a `.slint` file
/// would name for an interface.
const GENERIC: &[GenericFamily] = &[GenericFamily::SansSerif, GenericFamily::SystemUi, GenericFamily::UiSansSerif];

/// Registers the carried fonts with a collection and puts them first in
/// its generic families, with what was there behind them. Answers the
/// fonts that were found in their files, by the names in `CARRIED`.
fn put_first(collection: &mut Collection) -> Vec<&'static str> {
    let (mut families, mut found) = (Vec::<FamilyId>::new(), Vec::new());
    for (name, data) in CARRIED {
        // The bytes are the program's own: nothing is read or copied.
        let registered = collection.register_fonts(Blob::new(Arc::new(*data)), None);
        if !registered.is_empty() {
            found.push(*name);
        }
        for (family, _) in registered {
            if !families.contains(&family) {
                families.push(family);
            }
        }
    }
    for generic in GENERIC {
        let behind: Vec<FamilyId> = collection.generic_families(*generic).filter(|family| !families.contains(family)).collect();
        collection.set_generic_families(*generic, families.iter().copied().chain(behind));
    }
    found
}

/// Gives Slint the carried fonts. To be called on Slint's thread once its
/// backend is chosen: the collection is the one all its text is set from.
pub(crate) fn register() -> Vec<&'static str> {
    put_first(&mut slint::fontique_011::shared_collection())
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::fontique_011::fontique::{Attributes, CollectionOptions, FallbackKey, FontWeight, QueryFamily, QueryStatus, Script, SourceCache};

    /// The family a character is set in, found the way Parley finds it:
    /// the first font, of the families text is set in and then of the
    /// system's fallbacks for the script, that has the character.
    fn family_for(collection: &mut Collection, character: char, script: &str, weight: f32) -> Option<String> {
        let mut cache = SourceCache::default();
        let mut found = None;
        {
            let mut query = collection.query(&mut cache);
            // What Slint sets text in when no family is named.
            query.set_families([QueryFamily::Generic(GenericFamily::SansSerif), QueryFamily::Generic(GenericFamily::SystemUi)]);
            query.set_fallbacks(FallbackKey::new(Script::from_bytes(script.as_bytes().try_into().expect("a script is named by four letters")), None));
            query.set_attributes(Attributes { weight: FontWeight::new(weight), ..Attributes::default() });
            query.matches_with(|font| match font.charmap().and_then(|characters| characters.map(character)) {
                Some(_) => {
                    found = Some(font.family.0);
                    QueryStatus::Stop
                }
                None => QueryStatus::Continue,
            });
        }
        found.and_then(|family| collection.family_name(family).map(str::to_string))
    }

    fn with_the_systems_fonts() -> Collection {
        Collection::new(CollectionOptions { shared: false, system_fonts: true })
    }

    #[test]
    fn both_fonts_are_found_in_their_files() {
        assert_eq!(put_first(&mut with_the_systems_fonts()), ["Inter", "Noto Sans SC"]);
    }

    /// Latin is set in Inter and Chinese in Noto Sans SC, whatever the
    /// system would have answered, at each weight the interface uses.
    #[test]
    fn text_is_set_in_the_carried_fonts() {
        let mut collection = with_the_systems_fonts();
        put_first(&mut collection);
        for weight in [400.0, 500.0, 600.0] {
            for latin in ['a', 'Q', '7', '…', '—'] {
                assert_eq!(family_for(&mut collection, latin, "Latn", weight).as_deref(), Some("Inter"), "{latin} at {weight}");
            }
            // The three an English Windows had no glyph for, the
            // punctuation Chinese is written with, and a traditional form.
            for han in ['发', '这', '议', '汉', '，', '。', '髮'] {
                assert_eq!(family_for(&mut collection, han, "Hani", weight).as_deref(), Some("Noto Sans SC"), "{han} at {weight}");
            }
        }
    }

    /// What the carried fonts have not is still looked for in the system's.
    #[test]
    fn the_systems_fonts_stay_behind_them() {
        let mut collection = with_the_systems_fonts();
        let before = family_for(&mut collection, '한', "Hang", 400.0);
        put_first(&mut collection);
        let after = family_for(&mut collection, '한', "Hang", 400.0);
        assert!(after.as_deref() != Some("Inter") && after.as_deref() != Some("Noto Sans SC"), "Korean is in neither font, yet {after:?} was chosen");
        assert_eq!(after, before, "the system's choice for Korean changed");
    }
}
