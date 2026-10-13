//! The language Heyo speaks. What it says is written in English where it
//! is said, and the English is the key a translation is found by:
//!
//! - `t("Open at Login")` is that text in the language chosen;
//! - `tf("Hotkey for {0}", &[&title])` with its placeholders filled;
//! - `tc("verb", "Open")` where the same English means two things;
//! - `tn("{n} entry", "{n} entries", n, &[])` for a number of things;
//! - `marked("General")` in a table that is shown later, with `translate`.
//!
//! The translations are `locales/<language>/LC_MESSAGES/heyo.po`, gettext's
//! files, which `build.rs` reads into the table below: they are in the
//! program, and the language changes while it runs (`choose`). `pnpm i18n`
//! gathers the strings out of the sources (`scripts/i18n.mjs`), which is
//! why each is a literal in the call that marks it.
//!
//! One set of files serves the whole program. Slint's markup reads them
//! itself (`@tr`, bundled by `build.rs`); the web view and the runtime are
//! handed the chosen language's (`translations`) and say `t` the same way
//! (`shared/i18n.ts`). Placeholders are Slint's everywhere: `{}` the next
//! argument, `{0}` one by its place, `{n}` the number of a plural.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Serialize;
use ts_rs::TS;

// LANGUAGES: each translation's code (its directory), its own name for
// itself and its plural rule. MESSAGES: (context, English, each language's
// forms in that order), sorted; a message a language lacks has no forms.
include!(concat!(env!("OUT_DIR"), "/locales.rs"));

/// The setting that leaves the language to the system, and the code of the
/// language the sources are written in.
pub(crate) const SYSTEM: &str = "system";
pub(crate) const SOURCE: &str = "en";

/// Which language is spoken: 0 for the sources', else one more than its
/// place in `LANGUAGES`.
static CHOSEN: AtomicUsize = AtomicUsize::new(0);

/// The languages there are besides the sources': code and name.
pub(crate) fn languages() -> impl Iterator<Item = (&'static str, &'static str)> {
    LANGUAGES.iter().map(|(code, name, _)| (*code, *name))
}

fn chosen() -> usize {
    CHOSEN.load(Ordering::Relaxed)
}

/// The code of the language spoken now.
pub(crate) fn language() -> &'static str {
    code(chosen())
}

fn code(chosen: usize) -> &'static str {
    chosen.checked_sub(1).map_or(SOURCE, |place| LANGUAGES[place].0)
}

/// Speaks the language the setting names from now on: one by its code, or
/// the system's. True when that is another than before.
pub(crate) fn choose(setting: &str) -> bool {
    let chosen = resolve(setting, sys_locale::get_locales());
    CHOSEN.swap(chosen, Ordering::Relaxed) != chosen
}

/// A locale as a directory of `locales/` is named, and its language alone.
fn parts(locale: &str) -> (String, String) {
    let locale = locale.split(['.', '@']).next().unwrap_or_default().replace('-', "_");
    let language = locale.split('_').next().unwrap_or_default().to_lowercase();
    (locale, language)
}

/// The language a setting comes to. The system's is the first of the
/// locales it prefers that Heyo speaks: by its whole name, or else by its
/// language.
fn resolve(setting: &str, system: impl IntoIterator<Item = String>) -> usize {
    let spoken = |wanted: &str| {
        let (locale, language) = parts(wanted);
        let place = |matches: &dyn Fn(&str) -> bool| LANGUAGES.iter().position(|(code, ..)| matches(code)).map(|place| place + 1);
        place(&|code| code.eq_ignore_ascii_case(&locale)).or_else(|| (language == SOURCE).then_some(0)).or_else(|| place(&|code| parts(code).1 == language))
    };
    if setting != SYSTEM {
        return spoken(setting).unwrap_or(0);
    }
    system.into_iter().find_map(|locale| spoken(&locale)).unwrap_or(0)
}

fn forms(chosen: usize, context: &str, text: &str) -> &'static [&'static str] {
    let Some(place) = chosen.checked_sub(1) else { return &[] };
    MESSAGES.binary_search_by(|(its_context, its_text, _)| (*its_context, *its_text).cmp(&(context, text))).map_or(&[], |found| MESSAGES[found].2[place])
}

fn said(chosen: usize, context: &str, text: &'static str) -> &'static str {
    forms(chosen, context, text).first().copied().unwrap_or(text)
}

/// That text in the language spoken.
pub(crate) fn t(text: &'static str) -> &'static str {
    said(chosen(), "", text)
}

/// That text in the language spoken, where the English alone does not say
/// which of its meanings it is.
// Not called yet, nor are the two below: what is translated so far has no
// such text. The tests hold them to what they say.
#[allow(dead_code)]
pub(crate) fn tc(context: &str, text: &'static str) -> &'static str {
    said(chosen(), context, text)
}

/// That text with its placeholders filled.
#[allow(dead_code)]
pub(crate) fn tf(text: &'static str, with: &[&dyn Display]) -> String {
    fill(said(chosen(), "", text), with, None)
}

/// The text for `n` things, `{n}` being the number.
#[allow(dead_code)]
pub(crate) fn tn(one: &'static str, many: &'static str, n: u64, with: &[&dyn Display]) -> String {
    fill(counted(chosen(), one, many, n), with, Some(n))
}

fn counted(chosen: usize, one: &'static str, many: &'static str, n: u64) -> &'static str {
    let translated = forms(chosen, "", one);
    let rule = chosen.checked_sub(1).map_or(SOURCE_PLURAL, |place| LANGUAGES[place].2);
    translated.get(plural(rule, n)).copied().unwrap_or(if n == 1 { one } else { many })
}

/// Marks a text for translating where it is only kept: a table's entry,
/// shown later through `translate`.
pub(crate) const fn marked(text: &'static str) -> &'static str {
    text
}

/// A text that was `marked`, in the language spoken.
pub(crate) fn translate(text: &'static str) -> &'static str {
    said(chosen(), "", text)
}

/// Fills a text's placeholders: `{}` is the next of `with`, `{0}` the one
/// at that place, `{n}` the number. One it has nothing for stays as it is.
fn fill(text: &str, with: &[&dyn Display], n: Option<u64>) -> String {
    let mut filled = String::with_capacity(text.len());
    let (mut rest, mut next) = (text, 0);
    while let Some((before, after)) = rest.split_once('{') {
        filled.push_str(before);
        let Some((name, after)) = after.split_once('}') else {
            filled.push('{');
            rest = after;
            continue;
        };
        let place = if name.is_empty() {
            next += 1;
            Some(next - 1)
        } else {
            name.parse().ok()
        };
        match (place.and_then(|place| with.get(place)), n) {
            (Some(value), _) => filled.push_str(&value.to_string()),
            (None, Some(n)) if name == "n" => filled.push_str(&n.to_string()),
            _ => filled.extend(["{", name, "}"]),
        }
        rest = after;
    }
    filled + rest
}

// --- Plurals -------------------------------------------------------------------

/// English's rule, as a `.po` file would write it.
const SOURCE_PLURAL: &str = "n != 1";

/// Which of a language's plural forms `n` things take, by the rule its
/// file gives (`Plural-Forms`, an expression of C's). The first for a rule
/// that cannot be read.
fn plural(rule: &str, n: u64) -> usize {
    let mut rule = Rule { rest: rule.as_bytes(), n };
    rule.choice().filter(|_| rule.rest.iter().all(u8::is_ascii_whitespace)).unwrap_or(0) as usize
}

struct Rule<'a> {
    rest: &'a [u8],
    n: u64,
}

impl Rule<'_> {
    /// Takes `sign` if the rule goes on with it.
    fn takes(&mut self, sign: &str) -> bool {
        self.rest = self.rest.trim_ascii_start();
        let taken = self.rest.starts_with(sign.as_bytes());
        if taken {
            self.rest = &self.rest[sign.len()..];
        }
        taken
    }

    /// One level of operators that bind alike, over the level below. A
    /// sign that begins another ("<" and "<=") comes after it.
    fn level(&mut self, signs: &[(&str, fn(u64, u64) -> Option<u64>)], below: fn(&mut Self) -> Option<u64>) -> Option<u64> {
        let mut value = below(self)?;
        while let Some((_, apply)) = signs.iter().find(|(sign, _)| self.takes(sign)) {
            value = apply(value, below(self)?)?;
        }
        Some(value)
    }

    fn choice(&mut self) -> Option<u64> {
        let condition = self.either()?;
        if !self.takes("?") {
            return Some(condition);
        }
        let then = self.choice()?;
        let otherwise = self.takes(":").then(|| self.choice())??;
        Some(if condition != 0 { then } else { otherwise })
    }

    fn either(&mut self) -> Option<u64> {
        self.level(&[("||", |a, b| Some((a != 0 || b != 0) as u64))], Self::both)
    }

    fn both(&mut self) -> Option<u64> {
        self.level(&[("&&", |a, b| Some((a != 0 && b != 0) as u64))], Self::equal)
    }

    fn equal(&mut self) -> Option<u64> {
        self.level(&[("==", |a, b| Some((a == b) as u64)), ("!=", |a, b| Some((a != b) as u64))], Self::ordered)
    }

    fn ordered(&mut self) -> Option<u64> {
        self.level(&[("<=", |a, b| Some((a <= b) as u64)), (">=", |a, b| Some((a >= b) as u64)), ("<", |a, b| Some((a < b) as u64)), (">", |a, b| Some((a > b) as u64))], Self::sum)
    }

    fn sum(&mut self) -> Option<u64> {
        self.level(&[("+", |a, b| a.checked_add(b)), ("-", |a, b| a.checked_sub(b))], Self::product)
    }

    fn product(&mut self) -> Option<u64> {
        self.level(&[("*", |a, b| a.checked_mul(b)), ("/", |a, b| a.checked_div(b)), ("%", |a, b| a.checked_rem(b))], Self::one)
    }

    fn one(&mut self) -> Option<u64> {
        if self.takes("!") {
            return Some((self.one()? == 0) as u64);
        }
        if self.takes("(") {
            let inner = self.choice()?;
            return self.takes(")").then_some(inner);
        }
        if self.takes("n") {
            return Some(self.n);
        }
        let digits = self.rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
        let (number, rest) = self.rest.split_at(digits);
        self.rest = rest;
        std::str::from_utf8(number).ok()?.parse().ok()
    }
}

// --- For the web view and the runtime ----------------------------------------------

/// How many numbers a plural's forms are written out for (`Translations`).
const PLURALS_TOLD: u64 = 200;

/// A language as the web view and the runtime are handed it
/// (`shared/i18n.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub(crate) struct Translations {
    /// Its code: `en`, `zh_CN`.
    language: String,
    /// Which plural form each number of things takes, from none to 199.
    /// A number from 200 on takes the form of 100 and its last two digits,
    /// which holds for every rule gettext knows (a test holds each
    /// language Heyo speaks to it).
    plural: Vec<u8>,
    /// Each translated message's forms, by its English: after its context
    /// and U+0004 where it has one, as gettext's own files key it.
    messages: BTreeMap<String, Vec<String>>,
}

/// The language spoken now, for the web view and the runtime.
pub(crate) fn translations() -> Translations {
    spoken(chosen())
}

fn spoken(chosen: usize) -> Translations {
    let rule = chosen.checked_sub(1).map_or(SOURCE_PLURAL, |place| LANGUAGES[place].2);
    let messages = chosen.checked_sub(1).into_iter().flat_map(|place| MESSAGES.iter().filter(move |(.., forms)| !forms[place].is_empty()).map(move |(context, text, forms)| (if context.is_empty() { text.to_string() } else { format!("{context}\u{4}{text}") }, forms[place].iter().map(|form| form.to_string()).collect())));
    Translations { language: code(chosen).into(), plural: (0..PLURALS_TOLD).map(|n| plural(rule, n) as u8).collect(), messages: messages.collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where a number's plural form is in what `Translations` writes out,
    /// as `shared/i18n.ts` looks it up.
    fn told(n: u64) -> u64 {
        if n < PLURALS_TOLD { n } else { 100 + n % 100 }
    }

    fn place(language: &str) -> usize {
        LANGUAGES.iter().position(|(code, ..)| *code == language).expect("the language is in locales/") + 1
    }

    #[test]
    fn a_text_is_said_in_the_language_chosen_and_in_english_where_it_has_no_translation() {
        let chinese = place("zh_CN");
        assert_eq!(said(chinese, "", "General"), "通用");
        assert_eq!(said(0, "", "General"), "General");
        assert_eq!(said(chinese, "", "A text nobody translated"), "A text nobody translated");
        // A context is part of the key: the text alone is another message.
        assert_eq!(said(chinese, "no such context", "General"), "General");
    }

    #[test]
    fn the_table_is_in_the_order_it_is_searched_in() {
        assert!(MESSAGES.windows(2).all(|pair| (pair[0].0, pair[0].1) < (pair[1].0, pair[1].1)));
        assert!(MESSAGES.iter().all(|(.., forms)| forms.len() == LANGUAGES.len()));
    }

    #[test]
    fn placeholders_are_filled_by_place_in_order_and_by_the_number() {
        assert_eq!(fill("Hotkey for {0}", &[&"Clipboard History"], None), "Hotkey for Clipboard History");
        assert_eq!(fill("{} of {}", &[&1, &2], None), "1 of 2");
        assert_eq!(fill("{1}的{0}", &[&"a", &"b"], None), "b的a");
        assert_eq!(fill("{n} entries from {0}", &[&"Firefox"], Some(3)), "3 entries from Firefox");
        // What there is nothing for is left to be seen.
        assert_eq!(fill("{2} and {n} and {", &[&"a"], None), "{2} and {n} and {");
    }

    #[test]
    fn a_number_of_things_takes_the_form_its_language_has_for_it() {
        assert_eq!((counted(0, "{n} entry", "{n} entries", 1), counted(0, "{n} entry", "{n} entries", 0)), ("{n} entry", "{n} entries"));
        assert_eq!(fill(counted(0, "{n} entry", "{n} entries", 2), &[], Some(2)), "2 entries");
    }

    #[test]
    fn a_plural_rule_is_worked_out_as_c_would() {
        let forms = |rule: &str, numbers: &[u64]| numbers.iter().map(|n| plural(rule, *n)).collect::<Vec<_>>();
        assert_eq!(forms("n != 1", &[0, 1, 2]), [1, 0, 1]);
        assert_eq!(forms("0", &[0, 1, 2]), [0, 0, 0]);
        assert_eq!(forms("n > 1", &[0, 1, 2]), [0, 0, 1]);
        // Russian, Polish and Arabic, as gettext's manual gives them.
        let russian = "n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2";
        assert_eq!(forms(russian, &[1, 2, 5, 11, 21, 22, 25, 111, 112]), [0, 1, 2, 2, 0, 1, 2, 2, 2]);
        let polish = "n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2";
        assert_eq!(forms(polish, &[1, 2, 5, 12, 22, 101]), [0, 1, 2, 2, 1, 2]);
        let arabic = "n==0 ? 0 : n==1 ? 1 : n==2 ? 2 : n%100>=3 && n%100<=10 ? 3 : n%100>=11 ? 4 : 5";
        assert_eq!(forms(arabic, &[0, 1, 2, 3, 10, 11, 99, 100, 102, 103]), [0, 1, 2, 3, 3, 4, 4, 5, 5, 3]);
        // One that cannot be read chooses the first form.
        assert_eq!(forms("n !", &[5]), [0]);
        assert_eq!(forms("n / 0", &[5]), [0]);
    }

    #[test]
    fn every_language_has_a_rule_that_reads_and_that_the_windows_can_be_told() {
        for (code, _, rule) in LANGUAGES.iter().copied().chain([(SOURCE, "English", SOURCE_PLURAL)]) {
            let mut read = Rule { rest: rule.as_bytes(), n: 7 };
            assert!(read.choice().is_some() && read.rest.is_empty(), "{code}: its Plural-Forms cannot be read: {rule}");
            assert!((0..100_000).all(|n| plural(rule, n) == plural(rule, told(n))), "{code}: its plural forms do not come round every hundred");
        }
    }

    #[test]
    fn the_system_chooses_by_the_first_of_its_locales_heyo_speaks() {
        let system = |locales: &[&str]| resolve(SYSTEM, locales.iter().map(|locale| locale.to_string()));
        let chinese = place("zh_CN");
        assert_eq!(system(&["zh-CN", "en-US"]), chinese);
        assert_eq!(system(&["zh_CN.UTF-8"]), chinese);
        assert_eq!(system(&["en-US", "zh-CN"]), 0);
        assert_eq!(system(&["fr-FR", "zh-CN"]), chinese);
        // A language Heyo speaks as another region does.
        assert_eq!(system(&["zh-SG"]), chinese);
        assert_eq!(system(&["C"]), 0);
        assert_eq!(system(&[]), 0);
        // A setting names its language, whatever the system prefers.
        assert_eq!(resolve("zh_CN", vec!["en-US".to_string()]), chinese);
        assert_eq!(resolve(SOURCE, vec!["zh-CN".to_string()]), 0);
        assert_eq!(resolve("tlh", Vec::new()), 0);
    }

    #[test]
    fn the_windows_are_handed_the_language_spoken() {
        let english = spoken(0);
        assert_eq!((english.language.as_str(), english.messages.len(), &english.plural[..3]), ("en", 0, &[1, 0, 1][..]));
        let chinese = spoken(place("zh_CN"));
        assert_eq!((chinese.language.as_str(), chinese.messages["General"].as_slice(), chinese.plural.iter().max()), ("zh_CN", &["通用".to_string()][..], Some(&0)));
    }
}
