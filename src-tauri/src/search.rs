//! Fuzzy ranking over the item index.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Something the root search can find: an application or a command.
pub struct Entry {
    pub title: String,
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
            let title = score(&entry.title).map(|s| s + s / 4);
            let secondary = entry.keywords.iter().filter_map(|text| score(text)).max();
            title.max(secondary).map(|s| (i, s))
        })
        .collect()
}
