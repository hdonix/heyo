//! What a recording has put into the application, and what goes there next,
//! where its words are pasted (where they are composed instead, see
//! `composed`).
//!
//! With "type as you speak", the sentence being spoken is in the
//! application while it is still being recognized, and is corrected where
//! it stands when the recognizer changes its mind: [`OnScreen`] is told what
//! was heard ([`Event`]) and answers with what to do ([`Step`]). It
//! remembers and answers and touches nothing; `dictation` carries the steps
//! out and reports back when one could not be.
//!
//! # What is never done
//!
//! Correcting means erasing in someone else's text field, and Heyo may
//! erase only what it wrote itself in this recording and what is still the
//! last thing before the cursor. So a [`Step::Replace`] erases nothing but
//! characters this module knows it put there, counted as Backspace counts
//! them, and only [`plain`] text is ever written to be erased again: text in
//! which every application takes one Backspace for one character. Anything
//! else (a combining mark, an emoji made of several, a line end) is written
//! once and never corrected.
//!
//! The moment a step is reported lost ([`Event::Lost`]: the user typed or
//! clicked, another window came to the front, the application did not take
//! the text) nothing is erased any more for the rest of the recording. What
//! is in the application stays, and each sentence that finishes is pasted
//! at the cursor, wherever that is by then: less the beginning of it that
//! was already written.
//!
//! A step that was only put off ([`Event::PutOff`]: the desktop had the
//! keyboard, as it has while a hotkey is held) changes nothing: what is
//! heard next is written from where the text stands, and catches up.
//!
//! # At the end
//!
//! - Polished text takes the place of what was written as it was heard, if
//!   all of that can still be erased; otherwise the text stays as heard and
//!   [`OnScreen::shortfall`] says so.
//! - A cancelled recording is taken back the same way: cancelling discards
//!   the recording, and what it wrote goes with it, if it can.
//!
//! Where the words do not go in as they are spoken (the setting, the
//! session, a terminal) the same module says what was always done: each
//! finished sentence is pasted, or the polished text once at the end. So
//! it does for the rest of a recording whose words were composed in the
//! application up to then (`composed`) and can be no longer
//! ([`OnScreen::resuming`]).

/// The most characters erased at the end, for the polished text or a
/// cancelled recording. A GTK text view takes two thousand Backspaces in a
/// quarter of a second; an application several times slower is still done
/// before the wait gets long. Beyond that the text stays as it is.
pub(crate) const MOST_ERASED: usize = 2000;

/// What was heard, and what became of the last step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Event<'a> {
    /// The sentence in progress, as far as it is recognized.
    Partial(&'a str),
    /// A finished sentence, as it is written after those before it.
    Sentence(&'a str),
    /// The recording is over: `text` is what should stand, which is other
    /// than what was heard if it was `polished`.
    Result { text: &'a str, polished: bool },
    /// The recording was discarded.
    Cancelled,
    /// The last `Step::Replace` was not carried out as asked: `erased`
    /// characters were erased, and the text was `put` (sent on its way, and
    /// taken to have arrived) or not.
    Lost { erased: usize, put: bool },
    /// The last `Step::Replace` could not be carried out at that moment
    /// and nothing was touched: what is heard next may be written.
    PutOff,
}

/// What to do in the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    /// Erase `erase` characters before the cursor and write `insert` in
    /// their place. If that cannot be done as asked, `Event::Lost` is to be
    /// said before any other event.
    Replace { erase: usize, insert: String },
    /// Paste at the cursor, wherever it is.
    Paste(String),
}

/// What could not be done at the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shortfall {
    /// The text stays as it was heard.
    NotPolished(&'static str),
    /// What a cancelled recording wrote stays.
    NotTakenBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum How {
    /// The words are written as they are recognized and corrected in place.
    Revising,
    /// Each finished sentence is pasted; nothing is erased.
    Appending,
    /// Nothing is written until the end, when the polished text is pasted.
    AtTheEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    AsHeard,
    Polished,
    Cancelled,
}

/// What an application holds of a recording whose words reached it another
/// way so far (`composed`), for the rest to be pasted after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Standing {
    /// The finished sentences so far, as heard.
    pub(crate) said: String,
    /// How much of `said` is in the application (bytes),
    pub(crate) put: usize,
    /// and what stands there of the text after that.
    pub(crate) there: String,
}

/// The last replacement, in case it is reported lost.
#[derive(Debug)]
struct Undo {
    before: String,
    erase: usize,
    insert: String,
}

#[derive(Debug)]
pub(crate) struct OnScreen {
    how: How,
    /// The finished sentences so far, as heard.
    said: String,
    /// While revising: what this recording has written and can still erase,
    /// all of it before the cursor.
    screen: String,
    /// While appending: how much of `said` is dealt with (bytes),
    put: usize,
    /// and what stands in the application of the sentence after that.
    stale: String,
    undo: Option<Undo>,
    ending: Option<Ending>,
    shortfall: Option<Shortfall>,
}

/// Whether every application erases this character with one Backspace, and
/// only it: a character that stands alone. Left out, among others, are
/// control characters, combining marks and joiners, the pieces emoji are put
/// together from (variation selectors, skin tones, flags), and conjoining
/// Hangul letters. What is not listed is not plain: when in doubt, the text
/// is not corrected.
fn stands_alone(c: char) -> bool {
    matches!(u32::from(c),
        0x20..=0x7e                         // ASCII
        | 0xa1..=0xac | 0xae..=0x24f        // Latin, without the soft hyphen
        | 0x370..=0x377 | 0x37a..=0x37f | 0x384..=0x3ff     // Greek
        | 0x400..=0x482 | 0x48a..=0x52f     // Cyrillic, without its combining marks
        | 0x2010..=0x2027 | 0x2030..=0x205e // dashes, quotation marks, …
        | 0x20a0..=0x20c0                   // currencies
        | 0x2100..=0x214f | 0x2190..=0x23ff | 0x2460..=0x27bf | 0x2b00..=0x2bff     // letterlike, arrows, symbols
        | 0x3000..=0x3029 | 0x3030..=0x303f // CJK punctuation, without its tone marks
        | 0x3041..=0x3096 | 0x309b..=0x30ff // kana, without the combining voice marks
        | 0x3105..=0x312f | 0x31a0..=0x31bf // Bopomofo
        | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x323af  // ideographs
        | 0xac00..=0xd7a3                   // Hangul syllables
        | 0xff01..=0xff60 | 0xffe0..=0xffe6 // full-width forms
        | 0x1f300..=0x1f3fa | 0x1f400..=0x1f64f | 0x1f680..=0x1f6ff | 0x1f900..=0x1faff    // emoji of one character
    )
}

/// Whether text can be written to be erased again.
pub(crate) fn plain(text: &str) -> bool {
    text.chars().all(stands_alone)
}

/// How many bytes two texts begin with in common, whole characters only.
fn common(a: &str, b: &str) -> usize {
    a.char_indices().zip(b.chars()).find(|((_, x), y)| x != y).map_or(a.len().min(b.len()), |((at, _), _)| at)
}

/// How much of `owed` (bytes) the application can be taken to hold, where
/// `there` stands in its place and nothing may be erased: what the two
/// begin with in common. Where they part in the middle of a word, the word
/// is owed whole: after "watch" that should have been "walk", "walk" reads
/// better than "lk". (In writing without spaces between words they part
/// where they part.)
pub(super) fn held(there: &str, owed: &str) -> usize {
    let same = common(there, owed);
    let of_a_word = |c: char| c.is_alphanumeric() && u32::from(c) < 0x2e80;
    let parted_in_a_word = same < there.len() && owed[same..].starts_with(of_a_word) && owed[..same].ends_with(of_a_word);
    match parted_in_a_word {
        true => owed[..same].char_indices().rev().take_while(|(_, c)| of_a_word(*c)).last().map_or(same, |(at, _)| at),
        false => same,
    }
}

impl OnScreen {
    /// `live`: the words go in as they are spoken. `polishing`: the text is
    /// to be polished when the recording ends.
    pub(crate) fn new(live: bool, polishing: bool) -> OnScreen {
        let how = match (live, polishing) {
            (true, _) => How::Revising,
            (false, false) => How::Appending,
            (false, true) => How::AtTheEnd,
        };
        OnScreen { how, said: String::new(), screen: String::new(), put: 0, stale: String::new(), undo: None, ending: None, shortfall: None }
    }

    /// Goes on with a recording of which the application holds `standing`:
    /// nothing of that is erased, and what is said and not there yet is
    /// pasted, or the polished text once at the end if nothing is there.
    pub(crate) fn resuming(standing: Standing, polishing: bool) -> OnScreen {
        let untouched = standing.put == 0 && standing.there.is_empty();
        let how = if polishing && untouched { How::AtTheEnd } else { How::Appending };
        OnScreen { how, said: standing.said, put: standing.put, stale: standing.there, ..OnScreen::new(false, polishing) }
    }

    /// Whether the words are (still) going in as they are spoken.
    pub(crate) fn live(&self) -> bool {
        self.how == How::Revising
    }

    /// What could not be done at the end, once the recording is over.
    pub(crate) fn shortfall(&self) -> Option<Shortfall> {
        self.shortfall
    }

    pub(crate) fn on(&mut self, event: Event) -> Vec<Step> {
        match event {
            Event::Partial(text) => match self.how {
                How::Revising if plain(text) => self.revise(format!("{}{text}", self.said)),
                _ => Vec::new(),
            },
            Event::Sentence(text) => {
                self.said.push_str(text);
                match self.how {
                    How::Revising => {
                        let steps = self.revise(self.said.clone());
                        // It is written, and never touched again.
                        if !plain(text) {
                            self.settle();
                        }
                        steps
                    }
                    How::Appending => self.append(),
                    How::AtTheEnd => Vec::new(),
                }
            }
            Event::Result { text, polished } => {
                let polished = polished && text != self.said;
                self.ending = Some(if polished { Ending::Polished } else { Ending::AsHeard });
                match self.how {
                    // Not polished, the text is all there, but for a last
                    // sentence that came to nothing after all.
                    How::Revising if !polished => self.revise(self.said.clone()),
                    How::Revising => {
                        let same = common(&self.screen, text);
                        if self.screen[same..].chars().count() > MOST_ERASED {
                            self.shortfall = Some(Shortfall::NotPolished("there is too much text to replace where it stands"));
                            return self.revise(self.said.clone());
                        }
                        self.revise(text.to_owned())
                    }
                    How::Appending => {
                        if polished {
                            self.shortfall = Some(Shortfall::NotPolished(UNSAFE));
                        }
                        self.append()
                    }
                    How::AtTheEnd if text.is_empty() => Vec::new(),
                    How::AtTheEnd => vec![Step::Paste(text.to_owned())],
                }
            }
            Event::Cancelled => {
                self.ending = Some(Ending::Cancelled);
                match self.how {
                    How::Revising if self.screen.chars().count() > MOST_ERASED => {
                        self.shortfall = Some(Shortfall::NotTakenBack);
                        Vec::new()
                    }
                    How::Revising => self.revise(String::new()),
                    How::Appending => {
                        if self.put > 0 || !self.stale.is_empty() {
                            self.shortfall = Some(Shortfall::NotTakenBack);
                        }
                        Vec::new()
                    }
                    How::AtTheEnd => Vec::new(),
                }
            }
            // At the end there is no next time: it is lost.
            Event::PutOff if self.ending.is_some() => self.on(Event::Lost { erased: 0, put: false }),
            Event::PutOff => {
                if let Some(undo) = self.undo.take() {
                    self.screen = undo.before;
                }
                Vec::new()
            }
            Event::Lost { erased, put } => {
                let Some(undo) = self.undo.take() else { return Vec::new() };
                let mut there = undo.before;
                for _ in 0..erased.min(undo.erase) {
                    there.pop();
                }
                if put {
                    there.push_str(&undo.insert);
                }
                self.screen = there;
                match self.ending {
                    // What was to be taken back is not written again.
                    Some(Ending::Cancelled) => {
                        self.shortfall = Some(Shortfall::NotTakenBack);
                        self.settle();
                        Vec::new()
                    }
                    // The last text of all was sent: there is no more to write.
                    Some(_) if put => {
                        self.settle();
                        Vec::new()
                    }
                    ending => {
                        if ending == Some(Ending::Polished) {
                            self.shortfall = Some(Shortfall::NotPolished(UNSAFE));
                        }
                        self.fall_back()
                    }
                }
            }
        }
    }

    /// Makes what stands in the application `target`, erasing and writing
    /// no more than differs.
    fn revise(&mut self, target: String) -> Vec<Step> {
        let same = common(&self.screen, &target);
        let erase = self.screen[same..].chars().count();
        let insert = target[same..].to_owned();
        if erase == 0 && insert.is_empty() {
            return Vec::new();
        }
        self.undo = Some(Undo { before: std::mem::replace(&mut self.screen, target), erase, insert: insert.clone() });
        vec![Step::Replace { erase, insert }]
    }

    /// From here on nothing is erased, and all that was said is written.
    fn settle(&mut self) {
        self.how = How::Appending;
        self.put = self.said.len();
        self.stale.clear();
    }

    /// From here on nothing is erased: what stands in the application
    /// stays, and what was said and is not there yet is pasted.
    fn fall_back(&mut self) -> Vec<Step> {
        self.how = How::Appending;
        self.put = held(&self.screen, &self.said);
        self.stale = self.screen[self.put..].to_owned();
        self.append()
    }

    /// Pastes what was said and is not written yet: less the beginning of
    /// it that stands there already.
    fn append(&mut self) -> Vec<Step> {
        let owed = &self.said[self.put..];
        if owed.is_empty() {
            return Vec::new();
        }
        let text = owed[held(&self.stale, owed)..].to_owned();
        self.put = self.said.len();
        self.stale.clear();
        if text.is_empty() {
            Vec::new()
        } else {
            vec![Step::Paste(text)]
        }
    }
}

const UNSAFE: &str = "it could not safely take the place of the text, which was left as it was heard";

#[cfg(test)]
mod tests {
    use super::*;

    /// A text field as an application has one, and a user in front of it.
    /// Steps are carried out here as `dictation` carries them out, and what
    /// was done is noted: `-3+abc` for a replacement, `"abc"` for a paste.
    struct Field {
        text: String,
        screen: OnScreen,
        /// The user has touched the field: no replacement goes through.
        touched: bool,
        /// What becomes of the next replacement instead of its going in.
        losing: Option<(usize, bool)>,
        /// Another program has the keyboard: replacements are put off.
        taken: bool,
        done: Vec<String>,
    }

    impl Field {
        fn new(live: bool, polishing: bool) -> Field {
            Field { text: String::new(), screen: OnScreen::new(live, polishing), touched: false, losing: None, taken: false, done: Vec::new() }
        }

        fn holding(text: &str, live: bool, polishing: bool) -> Field {
            Field { text: text.into(), ..Field::new(live, polishing) }
        }

        fn erase(&mut self, count: usize) {
            for _ in 0..count {
                self.text.pop();
            }
        }

        fn hear(&mut self, event: Event) -> &mut Field {
            let mut steps: std::collections::VecDeque<Step> = self.screen.on(event).into();
            while let Some(step) = steps.pop_front() {
                match step {
                    Step::Replace { .. } if self.taken && !self.touched => {
                        self.done.push("put off".into());
                        steps.extend(self.screen.on(Event::PutOff));
                    }
                    Step::Replace { erase, insert } => {
                        let lost = if self.touched { Some((0, false)) } else { self.losing.take() };
                        let (erased, put) = lost.unwrap_or((erase, true));
                        self.erase(erased);
                        if put {
                            self.text.push_str(&insert);
                        }
                        self.done.push(format!("-{erased}+{}", if put { insert.as_str() } else { "" }));
                        if lost.is_some() {
                            self.touched = true;
                            steps.extend(self.screen.on(Event::Lost { erased, put }));
                        }
                    }
                    Step::Paste(text) => {
                        self.text.push_str(&text);
                        self.done.push(format!("{text:?}"));
                    }
                }
            }
            self
        }

        fn partial(&mut self, text: &str) -> &mut Field {
            self.hear(Event::Partial(text))
        }

        fn sentence(&mut self, text: &str) -> &mut Field {
            self.hear(Event::Sentence(text))
        }

        /// The recording ends with what was heard.
        fn ends(&mut self) -> &mut Field {
            let heard = self.screen.said.clone();
            self.hear(Event::Result { text: &heard, polished: false })
        }

        fn polished(&mut self, text: &str) -> &mut Field {
            self.hear(Event::Result { text, polished: true })
        }

        /// The user types into the field.
        fn types(&mut self, text: &str) -> &mut Field {
            self.text.push_str(text);
            self.touched = true;
            self
        }

        /// What was done since this was last asked.
        fn did(&mut self) -> Vec<String> {
            std::mem::take(&mut self.done)
        }
    }

    const NOTHING: [&str; 0] = [];

    #[test]
    fn words_go_in_as_the_sentence_grows() {
        let mut field = Field::new(true, false);
        field.partial("The weather").partial("The weather is").partial("The weather is nice");
        // Only what is new is written.
        assert_eq!(field.did(), ["-0+The weather", "-0+ is", "-0+ nice"]);
        // The same again changes nothing.
        field.partial("The weather is nice");
        assert_eq!(field.did(), NOTHING);
        field.sentence("The weather is nice today.").ends();
        assert_eq!(field.did(), ["-0+ today."]);
        assert_eq!(field.text, "The weather is nice today.");
        assert_eq!(field.screen.shortfall(), None);
    }

    #[test]
    fn a_change_of_mind_is_corrected_where_it_stands() {
        let mut field = Field::new(true, false);
        field.partial("I will bring 42 cents").partial("I will bring 42 sandwiches");
        // From where the two differ: no more is erased than is wrong.
        assert_eq!(field.did(), ["-0+I will bring 42 cents", "-5+sandwiches"]);
        // A change near the beginning costs the rest.
        field.partial("We will bring 42 sandwiches");
        assert_eq!(field.did(), ["-26+We will bring 42 sandwiches"]);
        assert_eq!(field.text, "We will bring 42 sandwiches");
        // And it may get shorter.
        field.partial("We will");
        assert_eq!(field.did(), ["-20+"]);
    }

    #[test]
    fn a_sentence_that_finishes_differently_is_what_stands() {
        let mut field = Field::holding("Dear Ada, ", true, false);
        field.partial("See who had 330").sentence("See you at 3:30.").ends();
        assert_eq!(field.did(), ["-0+See who had 330", "-11+you at 3:30."]);
        assert_eq!(field.text, "Dear Ada, See you at 3:30.");
    }

    #[test]
    fn sentences_follow_each_other_and_a_finished_one_is_left_alone() {
        let mut field = Field::new(true, false);
        field.partial("One").sentence("One.").partial(" Two").partial(" To").sentence(" Two three.").ends();
        // The second sentence's corrections stop at its own beginning.
        assert_eq!(field.did(), ["-0+One", "-0+.", "-0+ Two", "-2+o", "-1+wo three."]);
        assert_eq!(field.text, "One. Two three.");
    }

    #[test]
    fn characters_are_counted_as_backspace_counts_them() {
        let mut field = Field::new(true, false);
        // One character each, however many bytes or code units it is made of.
        field.partial("今天天气").partial("今天天汽车").partial("打开 VS Code，然后跑").partial("打开 VS Code，然后 run 😀");
        assert_eq!(field.did(), ["-0+今天天气", "-1+汽车", "-5+打开 VS Code，然后跑", "-1+ run 😀"]);
        field.partial("打开 VS Code，然后 run 🚀🚀").partial("안녕하세요").partial("안녕하십니까");
        assert_eq!(field.did(), ["-1+🚀🚀", "-20+안녕하세요", "-2+십니까"]);
        assert_eq!(field.text, "안녕하십니까");
    }

    #[test]
    fn only_text_of_characters_that_stand_alone_is_written_to_be_erased() {
        for text in ["plain text, with marks: (1) “quoted” — done… 100% №5", "今天天气很好，我们去公园散步吧。", "こんにちは、世界！カタカナとがぎぐ", "안녕하세요 반갑습니다.", "ok 😀🚀👍", "Größe, naïve, señor, Ελληνικά, по-русски"] {
            assert!(plain(text), "{text:?}");
        }
        for (text, what) in [
            ("two\nlines", "a line end"),
            ("a\ttab", "a tab"),
            ("e\u{301}", "a combining accent"),
            ("か\u{3099}", "a combining voice mark"),
            ("👍\u{1f3fd}", "a skin tone"),
            ("👩\u{200d}💻", "a joiner"),
            ("❤\u{fe0f}", "a variation selector"),
            ("🇩🇪", "a flag"),
            ("\u{1100}\u{1161}", "conjoining Hangul"),
            ("soft\u{ad}hyphen", "a soft hyphen"),
            ("right\u{200f}to left", "a direction mark"),
            ("ภาษาไทย", "a script that is not listed"),
        ] {
            assert!(!plain(text), "{what}");
        }
    }

    #[test]
    fn what_cannot_be_counted_is_written_once_and_never_corrected() {
        let mut field = Field::new(true, false);
        field.partial("Caf").partial("Cafe\u{301} au");
        // A partial that could not be taken back is not shown at all.
        assert_eq!(field.did(), ["-0+Caf"]);
        // The sentence itself has to go in; from then on nothing is erased.
        field.sentence("Cafe\u{301} au lait.");
        assert_eq!(field.did(), ["-0+e\u{301} au lait."]);
        assert!(!field.screen.live());
        field.partial(" And").sentence(" And tea.").ends();
        assert_eq!(field.did(), ["\" And tea.\""]);
        assert_eq!(field.text, "Cafe\u{301} au lait. And tea.");
    }

    #[test]
    fn once_the_user_types_nothing_is_erased_and_the_rest_is_added() {
        let mut field = Field::holding("Note: ", true, false);
        field.partial("The weather is nice").types("!");
        // The correction is refused, and the user's character stays.
        field.partial("The weather was nice today");
        assert_eq!(field.did(), ["-0+The weather is nice", "-0+"]);
        assert!(!field.screen.live());
        // No more partials are shown; the sentence adds what was not there.
        field.partial("The weather was nice today so");
        assert_eq!(field.did(), NOTHING);
        field.sentence("The weather is nice today, so let us walk.");
        assert_eq!(field.did(), ["\" today, so let us walk.\""]);
        // Later sentences are pasted whole.
        field.partial(" I will").sentence(" I will bring coffee.").ends();
        assert_eq!(field.did(), ["\" I will bring coffee.\""]);
        assert_eq!(field.text, "Note: The weather is nice! today, so let us walk. I will bring coffee.");
        assert_eq!(field.screen.shortfall(), None);
    }

    #[test]
    fn what_stood_there_wrongly_when_the_user_typed_stays_wrong() {
        let mut field = Field::new(true, false);
        field.partial("See who had").types(" ");
        // The beginning the two have in common is not written again.
        field.sentence("See you at three.");
        assert_eq!(field.did(), ["-0+See who had", "-0+", "\"you at three.\""]);
        assert_eq!(field.text, "See who had you at three.");
    }

    #[test]
    fn typing_between_two_sentences_loses_nothing() {
        let mut field = Field::new(true, false);
        field.partial("One").sentence("One.").types("\n");
        field.partial(" Two").sentence(" Two.").ends();
        assert_eq!(field.did(), ["-0+One", "-0+.", "-0+", "\" Two.\""]);
        assert_eq!(field.text, "One.\n Two.");
    }

    #[test]
    fn a_sentence_whose_correction_is_refused_is_added_instead() {
        let mut field = Field::new(true, false);
        field.partial("Hello wor").types("x").sentence("Hello world.");
        assert_eq!(field.did(), ["-0+Hello wor", "-0+", "\"ld.\""]);
        assert_eq!(field.text, "Hello worxld.");
    }

    #[test]
    fn a_step_that_went_half_way_is_reckoned_with() {
        // Three of five characters were erased when another window came to
        // the front, and the text was not sent.
        let mut field = Field::new(true, false);
        field.partial("I will bring 42 cents");
        field.losing = Some((3, false));
        field.partial("I will bring 42 sandwiches");
        assert_eq!(field.did(), ["-0+I will bring 42 cents", "-3+"]);
        field.sentence("I will bring 42 sandwiches.");
        // "ce" stands there still; what follows "I will bring 42 " is added
        // less nothing, since "ce" is not how it goes on.
        assert_eq!(field.did(), ["\"sandwiches.\""]);
        assert_eq!(field.text, "I will bring 42 cesandwiches.");

        // The text was sent and the application not seen to take it: it is
        // taken to be there, and never erased.
        let mut field = Field::new(true, false);
        field.losing = Some((0, true));
        field.partial("Hello");
        assert!(!field.screen.live());
        field.partial("Hello world").sentence("Hello world.").ends();
        assert_eq!(field.did(), ["-0+Hello", "\" world.\""]);
        assert_eq!(field.text, "Hello world.");
    }

    #[test]
    fn what_is_put_off_while_the_keyboard_is_taken_is_written_when_it_is_free() {
        // A hotkey is held to talk: the desktop has the keyboard throughout.
        let mut field = Field::holding("Notes: ", true, false);
        field.taken = true;
        field.partial("The weather").partial("The weather is nice").sentence("The weather is nice.").partial(" See");
        assert_eq!(field.did(), ["put off", "put off", "put off", "put off"]);
        assert_eq!(field.text, "Notes: ");
        assert!(field.screen.live());
        // Let go: all that was said is written in one piece, and corrected
        // from then on like any other.
        field.taken = false;
        field.partial(" See who").sentence(" See you.").ends();
        assert_eq!(field.did(), ["-0+The weather is nice. See who", "-3+you."]);
        assert_eq!(field.text, "Notes: The weather is nice. See you.");

        // Taken for a moment in the middle: nothing is lost or written twice.
        let mut field = Field::new(true, false);
        field.partial("One two");
        field.taken = true;
        field.partial("One too three");
        field.taken = false;
        field.sentence("One two three.");
        assert_eq!(field.did(), ["-0+One two", "put off", "-0+ three."]);

        // Still taken when the recording ends: there is no later, and what
        // was said is pasted as it would be without live words.
        let mut field = Field::new(true, false);
        field.taken = true;
        field.sentence("One.").sentence(" Two.").ends();
        assert_eq!(field.did(), ["put off", "put off", "put off", "\"One. Two.\""]);
        assert_eq!(field.text, "One. Two.");
        // Polished text that cannot go in then is left out like any other.
        let mut field = Field::new(true, true);
        field.partial("um one");
        field.taken = true;
        field.sentence("um one two.").polished("One, two.");
        assert_eq!(field.did(), ["-0+um one", "put off", "put off", "\" two.\""]);
        assert_eq!(field.text, "um one two.");
        assert_eq!(field.screen.shortfall(), Some(Shortfall::NotPolished(UNSAFE)));
    }

    #[test]
    fn polished_text_takes_the_place_of_what_was_heard() {
        // All of it, when it differs from the first word on.
        let mut field = Field::holding("> ", true, true);
        field.partial("um so the meeting").sentence("um so the meeting is on wednesday no thursday.");
        field.did();
        field.polished("The meeting is on Thursday.");
        assert_eq!(field.did(), ["-46+The meeting is on Thursday."]);
        assert_eq!(field.text, "> The meeting is on Thursday.");
        assert_eq!(field.screen.shortfall(), None);

        // Only its end, when that is all that changed.
        let mut field = Field::new(true, true);
        field.sentence("The meeting is on wednesday no thursday.");
        field.did();
        field.polished("The meeting is on Thursday.\n");
        assert_eq!(field.did(), ["-22+Thursday.\n"]);
        assert_eq!(field.text, "The meeting is on Thursday.\n");

        // Nothing, when polishing changed nothing or failed.
        let mut field = Field::new(true, true);
        field.sentence("All good.");
        field.did();
        field.polished("All good.");
        assert_eq!(field.did(), NOTHING);
        field.ends();
        assert_eq!(field.did(), NOTHING);
        assert_eq!(field.screen.shortfall(), None);
    }

    #[test]
    fn polished_text_is_refused_where_erasing_is_no_longer_safe() {
        // The user typed during the recording.
        let mut field = Field::new(true, true);
        field.partial("um hello").types("?").sentence("um hello there.").polished("Hello there.");
        assert_eq!(field.text, "um hello? there.");
        assert_eq!(field.screen.shortfall(), Some(Shortfall::NotPolished(UNSAFE)));

        // The user typed while it was being polished.
        let mut field = Field::new(true, true);
        field.sentence("um hello there.").types(" ok").polished("Hello there.");
        assert_eq!(field.did(), ["-0+um hello there.", "-0+"]);
        assert_eq!(field.text, "um hello there. ok");
        assert_eq!(field.screen.shortfall(), Some(Shortfall::NotPolished(UNSAFE)));

        // It was half erased when something came between: what was heard
        // is put back.
        let mut field = Field::new(true, true);
        field.sentence("um hello there.");
        field.losing = Some((9, false));
        field.polished("Hello there.");
        assert_eq!(field.did(), ["-0+um hello there.", "-9+", "\"lo there.\""]);
        assert_eq!(field.text, "um hello there.");
        assert_eq!(field.screen.shortfall(), Some(Shortfall::NotPolished(UNSAFE)));

        // It was sent and not seen to arrive: nothing more is written.
        let mut field = Field::new(true, true);
        field.sentence("um hello there.");
        field.losing = Some((15, true));
        field.polished("Hello there.");
        assert_eq!(field.text, "Hello there.");
        assert_eq!(field.screen.shortfall(), None);

        // More than is erased in one go.
        let long = "word ".repeat(MOST_ERASED / 5 + 1);
        let mut field = Field::new(true, true);
        field.sentence(&long).polished(&format!("Well, {long}"));
        assert_eq!(field.text, long);
        assert!(matches!(field.screen.shortfall(), Some(Shortfall::NotPolished(why)) if why.contains("too much")));
        // The same length with only its end changed is no trouble.
        let mut field = Field::new(true, true);
        field.sentence(&long).polished(&format!("{}words.", &long[..long.len() - 5]));
        assert!(field.text.ends_with("word words."));
        assert_eq!(field.screen.shortfall(), None);
    }

    #[test]
    fn a_sentence_in_progress_that_came_to_nothing_is_taken_back() {
        // Noise that looked like a word while it lasted.
        let mut field = Field::holding("Notes ", true, false);
        field.sentence("One.").partial(" Yeah").ends();
        assert_eq!(field.did(), ["-0+One.", "-0+ Yeah", "-5+"]);
        assert_eq!(field.text, "Notes One.");
        // Nothing was heard at all.
        let mut field = Field::holding("Notes ", true, false);
        field.partial("Mm").ends();
        assert_eq!(field.text, "Notes ");
    }

    #[test]
    fn a_cancelled_recording_is_taken_back_if_it_can_be() {
        let mut field = Field::holding("Dear Ada, ", true, false);
        field.sentence("One.").partial(" Two th").hear(Event::Cancelled);
        assert_eq!(field.did(), ["-0+One.", "-0+ Two th", "-11+"]);
        assert_eq!(field.text, "Dear Ada, ");
        assert_eq!(field.screen.shortfall(), None);

        // The user has typed: what was written stays, and that is said.
        let mut field = Field::holding("Dear Ada, ", true, false);
        field.sentence("One.").types("!").hear(Event::Cancelled);
        assert_eq!(field.text, "Dear Ada, One.!");
        assert_eq!(field.screen.shortfall(), Some(Shortfall::NotTakenBack));

        // Noticed only on taking it back: what is left of it stays.
        let mut field = Field::new(true, false);
        field.sentence("One two.");
        field.losing = Some((4, false));
        field.hear(Event::Cancelled);
        assert_eq!(field.did(), ["-0+One two.", "-4+"]);
        assert_eq!(field.text, "One ");
        assert_eq!(field.screen.shortfall(), Some(Shortfall::NotTakenBack));

        // Nothing was written yet: nothing to say.
        let mut field = Field::new(true, false);
        field.hear(Event::Cancelled);
        assert_eq!((field.did(), field.screen.shortfall()), (Vec::new(), None));
    }

    #[test]
    fn without_live_words_each_finished_sentence_is_pasted() {
        let mut field = Field::new(false, false);
        field.partial("Hel").partial("Hello").sentence("Hello.").partial(" Wor").sentence(" World.").ends();
        assert_eq!(field.did(), ["\"Hello.\"", "\" World.\""]);
        assert_eq!(field.text, "Hello. World.");
        // What was pasted cannot be taken back.
        let mut field = Field::new(false, false);
        field.sentence("Hello.").hear(Event::Cancelled);
        assert_eq!(field.text, "Hello.");
        assert_eq!(field.screen.shortfall(), Some(Shortfall::NotTakenBack));
        let mut field = Field::new(false, false);
        field.partial("Hel").hear(Event::Cancelled);
        assert_eq!((field.text.as_str(), field.screen.shortfall()), ("", None));
    }

    #[test]
    fn without_live_words_polished_text_goes_in_once_at_the_end() {
        let mut field = Field::new(false, true);
        field.partial("um hel").sentence("um hello.");
        assert_eq!(field.did(), NOTHING);
        field.polished("Hello.");
        assert_eq!(field.did(), ["\"Hello.\""]);
        // Polishing failed: the text as heard is still worth having.
        let mut field = Field::new(false, true);
        field.sentence("um hello.").ends();
        assert_eq!(field.did(), ["\"um hello.\""]);
        // Nothing heard, or cancelled: nothing pasted.
        let mut field = Field::new(false, true);
        field.ends();
        field.hear(Event::Cancelled);
        assert_eq!((field.did(), field.screen.shortfall()), (Vec::new(), None));
        let mut field = Field::new(false, true);
        field.sentence("um hello.").hear(Event::Cancelled);
        assert_eq!((field.text.as_str(), field.screen.shortfall()), ("", None));
    }

    #[test]
    fn a_recording_that_was_composed_until_now_goes_on_by_pasting() {
        let resumed = |said: &str, put: usize, there: &str, polishing: bool| {
            let screen = OnScreen::resuming(Standing { said: said.into(), put, there: there.into() }, polishing);
            Field { text: format!("{}{there}", &said[..put]), screen, ..Field::new(false, polishing) }
        };
        // A sentence was committed, and a key made part of the next one
        // the application's: what is said and not there yet is pasted.
        let mut field = resumed("One.", 4, " Two th", false);
        field.partial(" Two three").sentence(" Two three.").sentence(" Four.").ends();
        assert_eq!(field.did(), ["\"ree.\"", "\" Four.\""]);
        assert_eq!(field.text, "One. Two three. Four.");
        assert!(!field.screen.live());
        // The recording ended with a sentence still owed.
        let mut field = resumed("One. Two.", 4, "", false);
        field.ends();
        assert_eq!((field.did(), field.text.as_str()), (vec!["\" Two.\"".to_owned()], "One. Two."));
        // What is there is never erased: polished text is left out,
        let mut field = resumed("um one.", 7, "", true);
        field.sentence(" um two.").polished("One. Two.");
        assert_eq!(field.text, "um one. um two.");
        assert_eq!(field.screen.shortfall(), Some(Shortfall::NotPolished(UNSAFE)));
        // unless nothing of the recording is there yet.
        let mut field = resumed("um one.", 0, "", true);
        field.sentence(" um two.").polished("One. Two.");
        assert_eq!((field.did(), field.screen.shortfall()), (vec!["\"One. Two.\"".to_owned()], None));
        // And a cancelled one leaves what is there, and says so.
        let mut field = resumed("One.", 4, "", false);
        field.hear(Event::Cancelled);
        assert_eq!((field.text.as_str(), field.screen.shortfall()), ("One.", Some(Shortfall::NotTakenBack)));
    }

    #[test]
    fn a_report_of_loss_with_nothing_asked_changes_nothing() {
        let mut field = Field::new(true, false);
        assert_eq!(field.screen.on(Event::Lost { erased: 3, put: true }), []);
        assert!(field.screen.live());
        field.partial("Hello");
        assert_eq!(field.text, "Hello");
    }

    #[test]
    fn after_a_word_that_stands_there_wrongly_the_word_is_written_whole() {
        let owed = |there: &str, owed: &'static str| &owed[held(there, owed)..];
        // Where the two part in the middle of a word, the word is owed whole.
        assert_eq!(owed("let us watch", "let us walk to"), "walk to");
        assert_eq!(owed("bring 40", "bring 42 of them"), "42 of them");
        assert_eq!(owed("eine größere", "eine Größe und"), "Größe und");
        // Between words, and where what stands there is how the text goes
        // on, nothing is written twice.
        assert_eq!(owed("See who had", "See you at three."), "you at three.");
        assert_eq!(owed("we walked", "we walk to"), " to");
        assert_eq!(owed("Hello wor", "Hello world."), "ld.");
        assert_eq!(owed("Hello world", "Hello"), "");
        assert_eq!(owed("", "Hello"), "Hello");
        // Writing without spaces has no words to keep whole.
        assert_eq!(owed("今天天汽", "今天天气很好"), "气很好");

        // The user typed after a word the recognizer then changed its mind about.
        let mut field = Field::new(true, false);
        field.partial("let us watch").types("!").sentence("let us walk to the park.");
        assert_eq!(field.text, "let us watch!walk to the park.");
    }

    #[test]
    fn the_common_beginning_is_whole_characters() {
        assert_eq!(common("今天天气", "今天天汽"), "今天天".len());
        assert_eq!(common("é", "è"), 0);
        assert_eq!(common("abc", "abcd"), 3);
        assert_eq!(common("abcd", "abc"), 3);
        assert_eq!(common("", "abc"), 0);
        assert_eq!(common("😀a", "😀b"), "😀".len());
    }
}
