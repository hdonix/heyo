//! The user's own words for dictation: names, jargon and product names the
//! recognizer does not know how to write.
//!
//! The recognizer (SenseVoice) cannot be given words to listen for, so the
//! vocabulary is applied to what it wrote: the host corrects each piece the
//! helper says it heard, and tells the model that polishes the transcript
//! (`prompt`). A listed term is found however it was cased and spaced
//! ("type script", "Typescript" → "TypeScript"), and a `heard as -> written
//! as` line replaces one wording with another ("jason" → "JSON", "杰森" →
//! "JSON").
//!
//! What this cannot do: tell that a word was misheard. A Chinese name
//! written with the wrong characters of the same sound is only fixed if
//! those wrong characters are listed as "heard as"; and since Chinese has no
//! spaces, a "heard as" in Chinese is replaced wherever its characters
//! occur, also inside a longer word.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Vocabulary {
    /// How terms are to be written.
    pub terms: Vec<String>,
    /// What is heard → how it is to be written.
    pub replacements: Vec<(String, String)>,
}

/// White space as JavaScript's `trim` and `\s` know it, which the setting
/// was first read by: Unicode's, and the byte-order mark.
fn blank(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// Reads the setting: one entry per line, `Term` or `heard as -> Term`
/// (`→` and `=>` do as well).
pub(crate) fn parse(lines: &[String]) -> Vocabulary {
    let mut vocabulary = Vocabulary::default();
    for line in lines {
        // The first arrow of any kind divides what is heard from what is written.
        let arrow = ["->", "=>", "→"].iter().filter_map(|arrow| line.find(arrow).map(|at| (at, arrow.len()))).min();
        let (heard, written) = match arrow {
            Some((at, length)) => (line[..at].trim_matches(blank), line[at + length..].trim_matches(blank)),
            None => ("", line.trim_matches(blank)),
        };
        if written.is_empty() {
            continue;
        }
        if !heard.is_empty() {
            vocabulary.replacements.push((heard.to_owned(), written.to_owned()));
        }
        // What it is written as is itself a term: "json" becomes "JSON" too.
        if !vocabulary.terms.iter().any(|term| term == written) {
            vocabulary.terms.push(written.to_owned());
        }
    }
    vocabulary
}

fn space(c: char) -> bool {
    blank(c) || matches!(c, '-' | '‐' | '‑')
}

/// Joins the parts of a name written as one word: "node.js", "snake_case".
fn joiner(c: char) -> bool {
    matches!(c, '.' | '_')
}

/// A letter or digit of a script that puts spaces between words. Chinese,
/// Japanese and Korean text does not, so there a word may start anywhere.
fn wordy(c: char) -> bool {
    static WORDY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[[\p{L}\p{N}]--[\p{Han}\p{Hiragana}\p{Katakana}\p{Hangul}]]$").unwrap());
    if c.is_ascii() {
        return c.is_ascii_alphanumeric();
    }
    WORDY.is_match(c.encode_utf8(&mut [0; 4]))
}

/// What a wording is recognized by: its letters, lowercase, without spaces,
/// hyphens or the dots and underscores inside it.
fn key(text: &str) -> String {
    text.chars().filter(|&c| !space(c) && !joiner(c)).flat_map(char::to_lowercase).collect()
}

/// The correction pass for a vocabulary. Building it is the costly part and
/// is done once a recording; correcting a sentence then takes time that
/// depends on the sentence, not on how many terms there are.
#[derive(Debug, Default)]
pub(crate) struct Corrector {
    /// A wording's key → how it is written.
    written: HashMap<String, String>,
    /// The lengths of the keys in characters, longest first: "Visual
    /// Studio Code" before "Visual Studio".
    lengths: Vec<usize>,
    firsts: HashSet<char>,
}

impl Corrector {
    pub(crate) fn new(vocabulary: &Vocabulary) -> Corrector {
        let mut written = HashMap::new();
        for term in &vocabulary.terms {
            let k = key(term);
            if !k.is_empty() {
                written.entry(k).or_insert_with(|| term.clone());
            }
        }
        // A replacement wins over a term that reads the same.
        for (heard, term) in &vocabulary.replacements {
            let k = key(heard);
            if !k.is_empty() {
                written.insert(k, term.clone());
            }
        }
        let mut lengths: Vec<usize> = written.keys().map(|k| k.chars().count()).collect::<HashSet<_>>().into_iter().collect();
        lengths.sort_unstable_by(|a, b| b.cmp(a));
        let firsts = written.keys().filter_map(|k| k.chars().next()).collect();
        Corrector { written, lengths, firsts }
    }

    /// The text with the vocabulary's words written their way, and
    /// everything else exactly as it was.
    pub(crate) fn correct(&self, text: &str) -> String {
        if self.written.is_empty() {
            return text.to_owned();
        }
        let chars: Vec<char> = text.chars().collect();
        let word: Vec<bool> = chars.iter().map(|&c| wordy(c)).collect();
        let wordy_at = |i: Option<usize>| i.and_then(|i| word.get(i)).copied().unwrap_or(false);

        // The text as its key would be, and where each piece of it came from.
        let mut skeleton = String::new();
        let mut letters: Vec<char> = Vec::new();
        // For each character of `skeleton`, where it begins in it…
        let mut starts: Vec<usize> = Vec::new();
        // …and the index in `chars` it stands for.
        let mut from: Vec<usize> = Vec::new();
        for (i, &c) in chars.iter().enumerate() {
            if space(c) || (joiner(c) && wordy_at(i.checked_sub(1)) && wordy_at(Some(i + 1))) {
                continue;
            }
            for lower in c.to_lowercase() {
                starts.push(skeleton.len());
                skeleton.push(lower);
                letters.push(lower);
                from.push(i);
            }
        }
        starts.push(skeleton.len());

        let mut out = String::new();
        // Up to where `chars` has been written out.
        let mut done = 0;
        let mut at = 0;
        while at < letters.len() {
            let start = from[at];
            // Not in the middle of a character, nor of a word.
            let begins = (at == 0 || from[at - 1] != start) && self.firsts.contains(&letters[at]) && !(word[start] && wordy_at(start.checked_sub(1)));
            let mut matched = 0;
            if begins {
                for &length in &self.lengths {
                    if at + length > letters.len() {
                        continue;
                    }
                    let end = from[at + length - 1];
                    if at + length < letters.len() && from[at + length] == end {
                        continue;
                    }
                    if word[end] && wordy_at(Some(end + 1)) {
                        continue;
                    }
                    let Some(term) = self.written.get(&skeleton[starts[at]..starts[at + length]]) else { continue };
                    out.extend(&chars[done..start]);
                    out.push_str(term);
                    done = end + 1;
                    matched = length;
                    break;
                }
            }
            at += matched.max(1);
        }
        out.extend(&chars[done..]);
        out
    }
}

/// The vocabulary as the polishing model is told it; empty without one.
pub(crate) fn prompt(vocabulary: &Vocabulary, limit: usize) -> String {
    if vocabulary.terms.is_empty() {
        return String::new();
    }
    let mut heard: HashMap<&str, Vec<&str>> = HashMap::new();
    for (from, term) in &vocabulary.replacements {
        heard.entry(term.as_str()).or_default().push(from.as_str());
    }
    let lines: Vec<String> = vocabulary
        .terms
        .iter()
        .take(limit)
        .map(|term| match heard.get(term.as_str()) {
            Some(misheard) => format!("- {term} (may appear as: {})", misheard.join(", ")),
            None => format!("- {term}"),
        })
        .collect();
    format!("\n\nThe speaker uses the names and terms below. Where the transcript has one of them, or something that sounds like one where it would make sense, write it exactly as listed. Do not insert them where they were not said.\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| line.to_string()).collect()
    }

    fn correct(vocabulary: &[&str], text: &str) -> String {
        Corrector::new(&parse(&lines(vocabulary))).correct(text)
    }

    fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn terms_are_read_with_what_they_are_heard_as() {
        assert_eq!(
            parse(&lines(&["TypeScript", "  Heyo  ", "", "jason -> JSON", "杰森 → JSON", "k8s=>Kubernetes"])),
            Vocabulary { terms: lines(&["TypeScript", "Heyo", "JSON", "Kubernetes"]), replacements: pairs(&[("jason", "JSON"), ("杰森", "JSON"), ("k8s", "Kubernetes")]) }
        );
    }

    #[test]
    fn what_says_nothing_is_dropped() {
        assert_eq!(parse(&lines(&["   ", "heard ->", "->"])), Vocabulary::default());
        assert_eq!(parse(&[]), Vocabulary::default());
        // Only a way of writing it: a term.
        assert_eq!(parse(&lines(&["-> Heyo"])).terms, ["Heyo"]);
    }

    const ENGLISH: &[&str] = &["TypeScript", "Heyo", "Visual Studio Code", "Visual Studio", "Node.js", "Go", "jason -> JSON", "cube control -> kubectl"];

    #[test]
    fn a_term_is_written_as_listed_however_it_was_cased_and_spaced() {
        assert_eq!(correct(ENGLISH, "I write type script every day."), "I write TypeScript every day.");
        assert_eq!(correct(ENGLISH, "Typescript, typescript and TYPE-SCRIPT."), "TypeScript, TypeScript and TypeScript.");
        assert_eq!(correct(ENGLISH, "open heyo"), "open Heyo");
        assert_eq!(correct(ENGLISH, "Use node js or node.js or NodeJS."), "Use Node.js or Node.js or Node.js.");
    }

    #[test]
    fn the_longest_term_is_taken() {
        assert_eq!(correct(ENGLISH, "visual studio code and visual studio"), "Visual Studio Code and Visual Studio");
    }

    #[test]
    fn what_was_heard_is_replaced_with_what_was_meant() {
        assert_eq!(correct(ENGLISH, "Send me the Jason file."), "Send me the JSON file.");
        assert_eq!(correct(ENGLISH, "Run Cube Control get pods."), "Run kubectl get pods.");
        // What it is written as is found too.
        assert_eq!(correct(ENGLISH, "a json file"), "a JSON file");
    }

    #[test]
    fn nothing_is_replaced_inside_longer_words() {
        assert_eq!(correct(ENGLISH, "Going to Chicago, long ago."), "Going to Chicago, long ago.");
        assert_eq!(correct(ENGLISH, "heyoing and theyo"), "heyoing and theyo");
        assert_eq!(correct(ENGLISH, "typescripts"), "typescripts");
        assert_eq!(correct(ENGLISH, "Jasons"), "Jasons");
        assert_eq!(correct(ENGLISH, "go2market"), "go2market");
    }

    #[test]
    fn words_are_not_joined_across_punctuation() {
        assert_eq!(correct(ENGLISH, "I like that type. Script it."), "I like that type. Script it.");
        assert_eq!(correct(ENGLISH, "type, script"), "type, script");
        // But a sentence may end right after a term.
        assert_eq!(correct(ENGLISH, "It is type script."), "It is TypeScript.");
    }

    #[test]
    fn everything_else_is_left_exactly_as_it_was() {
        let text = "  Nothing here —  to  fix…\n";
        assert_eq!(correct(ENGLISH, text), text);
        assert_eq!(correct(&[], "type script"), "type script");
        assert_eq!(correct(ENGLISH, ""), "");
        // A sentence as the helper says it, with the space before it.
        assert_eq!(correct(ENGLISH, " Open heyo."), " Open Heyo.");
    }

    const MIXED: &[&str] = &["TypeScript", "B站", "小红书", "杰森 -> JSON", "哈克 -> Heyo", "GitHub"];

    #[test]
    fn a_term_is_found_in_text_without_spaces() {
        assert_eq!(correct(MIXED, "我用typescript写代码。"), "我用TypeScript写代码。");
        assert_eq!(correct(MIXED, "代码在 git hub 上，用 type script 写的。"), "代码在 GitHub 上，用 TypeScript 写的。");
    }

    #[test]
    fn a_mixed_term_that_came_out_in_pieces_is_joined() {
        assert_eq!(correct(MIXED, "我在 b 站看视频"), "我在 B站看视频");
        assert_eq!(correct(MIXED, "我在b站和小 红 书都有账号"), "我在B站和小红书都有账号");
    }

    #[test]
    fn chinese_heard_for_a_name_is_replaced() {
        assert_eq!(correct(MIXED, "把杰森文件发给我，然后打开哈克。"), "把JSON文件发给我，然后打开Heyo。");
    }

    #[test]
    fn latin_terms_are_not_replaced_inside_longer_latin_words_in_chinese_text() {
        assert_eq!(correct(MIXED, "这个typescripts不是词"), "这个typescripts不是词");
        assert_eq!(correct(MIXED, "用mygithub账号"), "用mygithub账号");
    }

    #[test]
    fn a_homophone_that_is_not_listed_is_not_known() {
        // Same sound, other characters: nothing here knows they sound alike.
        assert_eq!(correct(MIXED, "把结森文件发给我"), "把结森文件发给我");
    }

    #[test]
    fn characters_outside_the_basic_plane_and_ones_that_grow_when_lowercased_are_coped_with() {
        assert_eq!(correct(&["TypeScript"], "😀 type script 😀"), "😀 TypeScript 😀");
        assert_eq!(correct(&["TypeScript"], "İ type script"), "İ TypeScript");
        assert_eq!(correct(&["𠮷野家"], "去𠮷 野家吃饭"), "去𠮷野家吃饭");
    }

    #[test]
    fn a_few_hundred_terms_are_quick() {
        let mut vocabulary: Vec<String> = (0..400).map(|i| format!("Term{i} Name{}", i % 7)).collect();
        vocabulary.extend(lines(&["TypeScript", "杰森 -> JSON"]));
        let started = std::time::Instant::now();
        let fix = Corrector::new(&parse(&vocabulary));
        let built = started.elapsed();
        let sentence = "今天我们用 type script 写了一个新的功能，然后把杰森文件发给了大家，term 399 name 0 也在里面。 ".repeat(8);
        let before = std::time::Instant::now();
        let mut out = String::new();
        for _ in 0..100 {
            out = fix.correct(&sentence);
        }
        let each = before.elapsed() / 100;
        assert!(out.contains("TypeScript") && out.contains("JSON") && out.contains("Term399 Name0"), "{out}");
        // A partial result is corrected every 300 ms; this must be nothing beside that.
        assert!(built.as_millis() < 100, "{built:?}");
        assert!(each.as_millis() < 5, "{each:?}");
    }

    #[test]
    fn the_model_is_told_the_terms_and_what_they_may_be_heard_as() {
        let told = prompt(&parse(&lines(&["TypeScript", "jason -> JSON", "杰森 -> JSON"])), 400);
        assert!(told.contains("- TypeScript\n"), "{told}");
        assert!(told.contains("- JSON (may appear as: jason, 杰森)"), "{told}");
        // Nothing without a vocabulary, and not too much with a long one.
        assert_eq!(prompt(&Vocabulary::default(), 400), "");
        let long = prompt(&parse(&(0..1000).map(|i| format!("Word{i}")).collect::<Vec<_>>()), 50);
        assert_eq!(long.lines().filter(|line| line.starts_with("- ")).count(), 50);
    }
}
