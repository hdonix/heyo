//! Putting together what a query's words found and what its meaning found.
//!
//! The words are certain: every one of them is in every entry they find, and
//! those come the most recent first. The meaning is a guess, and always has
//! an answer, however far off. So which of the two a query is about is
//! decided first (`weight`): a piece of an address or a number is looked
//! for by its letters alone. For the rest the two orders are merged by
//! rank (reciprocal rank fusion), since a place in a list is the one thing
//! "most recent" and "nearest in meaning" can be compared by.

use std::collections::{HashMap, HashSet};

use super::vectors::Index;

/// What decides how much the meaning counts; in one place, to be set again
/// when there are real searches to set it by (`clipboard/eval.rs`).
///
/// `floor` and `count` were set on the cases that come with the source.
/// The two weights could not be: in those cases the words either find the
/// entry first or find nothing, so any weights do as well. They are the
/// cautious choice. At a half, an entry the meaning alone found comes after
/// some sixty that the words found, which is to say after all of them, and
/// only what both found moves up; at one, the two lists take turns.
#[derive(Debug, Clone, PartialEq)]
pub struct Tuning {
    /// How much the meaning counts against the words (which count 1) for a
    /// query of a word or two.
    pub short: f32,
    /// The same for a query that reads like a description.
    pub long: f32,
    /// An entry less like the query than this is not what was meant. (A
    /// query nothing answers still gets its guesses: no value tells those
    /// from right answers. This only cuts the guesses short.)
    pub floor: f32,
    /// How many entries the meaning may bring.
    pub count: usize,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning { short: 0.5, long: 1.0, floor: 0.25, count: 10 }
    }
}

/// The usual constant of reciprocal rank fusion: how fast a place further
/// down a list counts for less.
const RANK: f32 = 60.0;

/// Whether a word is looked for by its letters: a piece of an address, a
/// path, a number, an identifier, a command's option.
fn exact(term: &str) -> bool {
    let punctuation = term.chars().filter(|c| c.is_ascii_punctuation() && !matches!(c, '-' | '\'' | '?' | '!' | ',')).count();
    let digits = term.chars().filter(char::is_ascii_digit).count();
    punctuation > 0 || digits >= 3 || term.starts_with('-') || !term.chars().any(char::is_alphabetic)
}

/// How much the meaning of a query counts against its words (which count
/// 1), given the words left once a time, a source and a kind are taken out
/// of it. `None`: not at all, and the model need not be asked.
pub fn weight(terms: &[String], tuning: &Tuning) -> Option<f32> {
    if terms.is_empty() || terms.iter().any(|term| exact(term)) {
        return None;
    }
    let characters: usize = terms.iter().map(|term| term.chars().count()).sum();
    let chinese = terms.iter().flat_map(|term| term.chars()).filter(|c| !c.is_ascii() && c.is_alphabetic()).count();
    // The beginning of a word being typed means nothing yet.
    if characters < if chinese > 0 { 2 } else { 4 } {
        return None;
    }
    let long = terms.len() >= 3 || chinese >= 5 || characters >= 16;
    Some(if long { tuning.long } else { tuning.short })
}

/// What of a query's words is put to the model, if anything is.
pub fn asked(terms: &[String], tuning: &Tuning) -> Option<String> {
    weight(terms, tuning).map(|_| terms.join(" "))
}

/// The entries of both lists in one order: `words` as the word search found
/// them, `meaning` nearest first, each place counting for less the further
/// down it is, and the places in `meaning` for `weight` times as much.
/// With each entry, whether the meaning alone brought it.
pub fn fuse(words: &[u64], meaning: &[u64], weight: f32) -> Vec<(u64, bool)> {
    scored(words, meaning, weight).into_iter().map(|(id, _, alone)| (id, alone)).collect()
}

/// `fuse`, with each entry's score: what its places in the two lists add up to.
pub fn scored(words: &[u64], meaning: &[u64], weight: f32) -> Vec<(u64, f32, bool)> {
    let mut scores: HashMap<u64, f32> = HashMap::new();
    let mut order: Vec<u64> = Vec::new();
    for (list, counts) in [(words, 1.0), (meaning, weight)] {
        for (place, id) in list.iter().enumerate() {
            let score = scores.entry(*id).or_insert_with(|| {
                order.push(*id);
                0.0
            });
            *score += counts / (RANK + 1.0 + place as f32);
        }
    }
    let by_words: HashSet<u64> = words.iter().copied().collect();
    // Stable: of two that score alike, the one the words found comes first.
    order.sort_by(|a, b| scores[b].total_cmp(&scores[a]));
    order.into_iter().map(|id| (id, scores[&id], !by_words.contains(&id))).collect()
}

/// The entries near enough to the query (a text in `language`) to be what was meant.
pub fn near(index: &mut Index, vector: &[f32], language: usize, allowed: Option<&HashSet<u64>>, tuning: &Tuning) -> Vec<u64> {
    let found = index.nearest(vector, language, allowed, tuning.count);
    found.into_iter().filter(|(_, score)| *score >= tuning.floor).map(|(id, _)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn what_is_looked_for_by_its_letters() {
        let tuning = Tuning::default();
        for query in ["heyo/issues", "0.3.17", "ref=77", "=>", "-p", "deploy@10.0.3.17", "example.com", "issue 4242", "user_id", "a::b", "s", "ss", "票", ""] {
            assert_eq!(weight(&terms(query), &tuning), None, "{query}");
        }
        for query in ["发票", "invoice", "meeting notes", "re-run", "what's up"] {
            assert_eq!(weight(&terms(query), &tuning), Some(tuning.short), "{query}");
        }
        for query in ["the ssh command for staging", "那个服务器的登录命令", "how to reset password"] {
            assert_eq!(weight(&terms(query), &tuning), Some(tuning.long), "{query}");
        }
        assert_eq!(asked(&terms("登录 命令"), &tuning).as_deref(), Some("登录 命令"));
        assert_eq!(asked(&[], &tuning), None);
    }

    #[test]
    fn two_orders_become_one() {
        // In both: first. Then the lists take turns, the words' entry first.
        assert_eq!(fuse(&[1, 2, 3], &[9, 2, 8], 1.0), [(2, false), (1, false), (9, true), (3, false), (8, true)]);
        // The meaning counting for little: what it alone found comes after
        // all that the words found, in its own order.
        assert_eq!(fuse(&[1, 2, 3], &[9, 3, 8], 0.01), [(1, false), (2, false), (3, false), (9, true), (8, true)]);
        // Counting for much, it leads.
        assert_eq!(fuse(&[1, 2], &[9, 8], 3.0), [(9, true), (8, true), (1, false), (2, false)]);
        // Either alone is itself.
        assert_eq!(fuse(&[1, 2], &[], 1.0), [(1, false), (2, false)]);
        assert_eq!(fuse(&[], &[5, 4], 1.0), [(5, true), (4, true)]);
        assert!(fuse(&[], &[], 1.0).is_empty());
    }
}
