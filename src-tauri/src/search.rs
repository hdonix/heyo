//! Fuzzy ranking over the item index.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Something the root search can find: an application or a command.
pub struct Entry {
    pub title: String,
    /// Its English title, where the one shown is in another language: it
    /// is found by either, and they weigh the same.
    pub english: Option<String>,
    /// Secondary text matched with less weight (generic names, keywords).
    pub keywords: Vec<String>,
}

/// Fuzzy match scores of the entries that match `query`, as
/// `(index, score)` in index order.
pub fn scores(entries: &[Entry], query: &str) -> Vec<(usize, u32)> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query.trim(), CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();
    let mut score = |text: &str| pattern.score(Utf32Str::new(text, &mut buf), &mut matcher);

    entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            // A small title bonus ranks "Terminal" above an app that merely
            // lists "terminal" as a keyword, without letting a scattered
            // title match beat an exact keyword.
            let title = score(&entry.title).max(entry.english.as_deref().and_then(&mut score)).map(|s| s + s / 4);
            let secondary = entry.keywords.iter().filter_map(|text| score(text)).max();
            title.max(secondary).map(|s| (i, s))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, english: Option<&str>, keywords: &[&str]) -> Entry {
        Entry { title: title.into(), english: english.map(str::to_owned), keywords: keywords.iter().map(|keyword| keyword.to_string()).collect() }
    }

    #[test]
    fn an_entry_is_found_by_the_title_shown_and_by_its_english_one_alike() {
        let english = entry("Clipboard History", None, &["paste", "copy"]);
        let chinese = entry("剪贴板历史", Some("Clipboard History"), &["paste", "copy", "粘贴", "复制"]);
        let entries = [english, chinese];
        // What is typed in English weighs the same whatever language the
        // title is shown in.
        for query in ["clip", "clipboard history", "hist", "paste"] {
            let found = scores(&entries, query);
            assert_eq!((found.len(), found[0].1), (2, found[1].1), "{query}");
        }
        // The title shown and its language's keywords find it too.
        for query in ["剪贴板", "历史", "粘贴"] {
            assert_eq!(scores(&entries, query).iter().map(|(i, _)| *i).collect::<Vec<_>>(), [1], "{query}");
        }
    }
}
