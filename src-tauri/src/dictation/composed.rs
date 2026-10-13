//! What a recording tells the input method, where its words reach the
//! application by being composed there (`platform::composing`).
//!
//! An input method has two things to say: this text is being composed (it
//! is shown in the application, underlined, and may be replaced as often as
//! it changes), and this text is the application's. [`Composed`] is told
//! what was heard and what happened in the field ([`Event`]), and answers
//! with what to say ([`Tell`]). It remembers and answers and touches
//! nothing.
//!
//! - The sentence in progress is what is being composed, replaced whole
//!   each time the recognizer has more of it; a finished sentence is
//!   committed. Nothing is ever erased: what is being composed is not the
//!   application's text yet.
//! - With polishing, the whole recording stays in composition until the
//!   polished text is there to be committed in its place. Nothing has to be
//!   replaced afterwards, which no input method can do in every
//!   application.
//! - A cancelled recording stops being composed and leaves no trace of
//!   what was not committed. Sentences that were committed stay: they are
//!   the application's, and [`Composed::shortfall`] says so.
//!
//! # What the user does meanwhile
//!
//! Text being composed cannot stay so while something else happens in the
//! field.
//!
//! - When the user presses a key, it is committed as it stands, ahead of
//!   the key. That part of the recording is the application's from then
//!   on, as it was heard up to then, and composing goes on behind it: what
//!   is heard next is composed less the beginning that already stands
//!   there. Polished text can then no longer take the place of the whole,
//!   and the rest is committed as it was heard.
//! - When the user clicks in the field, the field shows it no more, and it
//!   is shown again where the cursor is now.
//! - When the field loses the focus, it shows it no more, and nothing of a
//!   sentence that was not finished is left behind there. While no field
//!   has the focus (the user is between two windows, or the desktop has
//!   the keyboard because a hotkey is held) nothing can be said: what is
//!   heard is remembered, and said when a field has the focus again.
//!
//! # Where it ends
//!
//! Composing is over when the input method is no longer Heyo's
//! ([`Event::Gone`]), or when the recording ends with words still owed and
//! no field to say them in. [`Composed::standing`] is then what the
//! application holds of the recording, for whoever writes the rest.

use crate::i18n::marked;
use super::onscreen::{held, Shortfall, Standing};

/// What was heard, and what happened in the field.
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
    /// The recording broke off: no more will be heard.
    Failed,
    /// The user pressed a key that is on its way to the field.
    Key,
    /// The field shows what was being composed no more: the user clicked
    /// in it.
    Reset,
    /// The same, and no field has the focus any more.
    Left,
    /// A field has the focus (again).
    Entered,
    /// The input method is no longer Heyo's, and what was being composed
    /// is shown no more.
    Gone,
}

/// What to say to the input method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tell {
    /// This is what is being composed, in place of what was shown; none
    /// shows nothing.
    Show(String),
    /// This is the application's.
    Commit(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Composed {
    /// The sentence in progress is shown while it is spoken.
    live: bool,
    /// The text is to be polished when the recording ends.
    polishing: bool,
    /// The finished sentences so far, as heard,
    said: String,
    /// and the sentence in progress.
    partial: String,
    /// How much of `said` is the application's (bytes),
    put: usize,
    /// and what the application holds of the text after that: what was
    /// committed ahead of a key, as it was heard by then.
    kept: String,
    /// What is shown as being composed.
    shown: String,
    /// It was shown again after the field let go of it, and nothing has
    /// been heard since: a field that lets go of whatever it is shown is
    /// not shown the same again and again.
    shown_again: bool,
    /// A field has the focus.
    in_a_field: bool,
    over: bool,
    shortfall: Option<Shortfall>,
}

const KEPT_AS_HEARD: &str = marked("part of it had become the application's text as it was heard");

impl Composed {
    /// `live`: the sentence in progress is shown while it is spoken.
    /// `polishing`: the text is to be polished when the recording ends.
    /// Composing begins in a field that has the focus.
    pub(crate) fn new(live: bool, polishing: bool) -> Composed {
        Composed { live, polishing, said: String::new(), partial: String::new(), put: 0, kept: String::new(), shown: String::new(), shown_again: false, in_a_field: true, over: false, shortfall: None }
    }

    /// Whether the words are on screen as they are spoken at present.
    pub(crate) fn live(&self) -> bool {
        self.live && self.in_a_field && !self.over
    }

    /// Whether no field has the focus at present, while composing is not over.
    pub(crate) fn between_fields(&self) -> bool {
        !self.in_a_field && !self.over
    }

    /// What could not be done at the end, once the recording is over.
    pub(crate) fn shortfall(&self) -> Option<Shortfall> {
        self.shortfall
    }

    /// Composing is over, as when the input method is no longer Heyo's:
    /// what the application holds of the recording.
    pub(crate) fn given_up(&mut self) -> Standing {
        self.on(Event::Gone);
        Standing { said: self.said.clone(), put: self.put, there: self.kept.clone() }
    }

    /// Once composing is over: what the application holds of the
    /// recording. The event that was answered with nothing because of it
    /// is for whoever goes on from there.
    pub(crate) fn standing(&self) -> Option<Standing> {
        self.over.then(|| Standing { said: self.said.clone(), put: self.put, there: self.kept.clone() })
    }

    pub(crate) fn on(&mut self, event: Event) -> Vec<Tell> {
        if self.over {
            return Vec::new();
        }
        match event {
            Event::Partial(text) => {
                text.clone_into(&mut self.partial);
                self.shown_again = false;
                self.show()
            }
            Event::Sentence(text) => {
                self.said.push_str(text);
                self.partial.clear();
                self.shown_again = false;
                match self.polishing {
                    true => self.show(),
                    false => self.commit_what_was_said(),
                }
            }
            Event::Result { text, polished } => {
                self.partial.clear();
                // Polished text takes the place of all that was heard, and
                // cannot where some of that is the application's already.
                let whole = self.put == 0 && self.kept.is_empty();
                let polished = polished && text != self.said;
                if polished && !whole {
                    self.shortfall = Some(Shortfall::NotPolished(KEPT_AS_HEARD));
                }
                let owed = if polished && whole { text } else { self.owed_of_what_was_said() };
                if !owed.is_empty() && !self.in_a_field {
                    self.over = true;
                    self.shortfall = None;
                    return Vec::new();
                }
                let owed = owed.to_owned();
                self.put = self.said.len();
                self.kept.clear();
                self.finish(owed)
            }
            Event::Cancelled => {
                if self.put > 0 || !self.kept.is_empty() {
                    self.shortfall = Some(Shortfall::NotTakenBack);
                }
                self.finish(String::new())
            }
            // What is being composed is committed as it stands: ahead of the
            // key, and when no more will be heard.
            Event::Key | Event::Failed => {
                let shown = self.shown.clone();
                self.kept.push_str(&shown);
                self.finish(shown)
            }
            Event::Reset => {
                self.shown.clear();
                match std::mem::replace(&mut self.shown_again, true) {
                    true => Vec::new(),
                    false => self.show(),
                }
            }
            Event::Left => {
                self.shown.clear();
                self.in_a_field = false;
                Vec::new()
            }
            Event::Entered => {
                self.in_a_field = true;
                self.shown_again = false;
                let mut tells = if self.polishing { Vec::new() } else { self.commit_what_was_said() };
                tells.extend(self.show());
                tells
            }
            Event::Gone => {
                self.shown.clear();
                self.in_a_field = false;
                self.over = true;
                Vec::new()
            }
        }
    }

    /// Of the finished sentences, what the application does not hold yet:
    /// less the beginning of it that it kept.
    fn owed_of_what_was_said(&self) -> &str {
        let owed = &self.said[self.put..];
        &owed[held(&self.kept, owed)..]
    }

    /// Commits the finished sentences that are not the application's yet,
    /// if there is a field to commit them in.
    fn commit_what_was_said(&mut self) -> Vec<Tell> {
        if !self.in_a_field {
            return Vec::new();
        }
        let finished = &self.said[self.put..];
        let same = held(&self.kept, finished);
        let owed = finished[same..].to_owned();
        // What was kept beyond the finished sentences is of the one in
        // progress, unless it went another way than they did.
        self.kept = if same == finished.len() { self.kept[same..].to_owned() } else { String::new() };
        self.put = self.said.len();
        self.finish(owed)
    }

    /// Commits `text`, if any, and shows nothing as being composed.
    fn finish(&mut self, text: String) -> Vec<Tell> {
        let mut tells = Vec::new();
        if !text.is_empty() {
            tells.push(Tell::Commit(text));
        }
        if !std::mem::take(&mut self.shown).is_empty() {
            tells.push(Tell::Show(String::new()));
        }
        tells
    }

    /// Shows what was heard and is not the application's yet, if it is to
    /// be shown and is not what is shown already.
    fn show(&mut self) -> Vec<Tell> {
        if !(self.live && self.in_a_field) {
            return Vec::new();
        }
        let owed = format!("{}{}", &self.said[self.put..], self.partial);
        let rest = &owed[held(&self.kept, &owed)..];
        if rest == self.shown {
            return Vec::new();
        }
        rest.clone_into(&mut self.shown);
        vec![Tell::Show(rest.to_owned())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A text field as an application has one, an input method between it
    /// and `Composed`, and a user in front of it. What `Composed` says is
    /// carried out here as an input method carries it out, and noted:
    /// `~abc` for text shown as being composed, `abc` for text committed.
    struct Field {
        /// The application's text,
        text: String,
        /// and what is shown at its end as being composed.
        composing: String,
        composed: Composed,
        told: Vec<String>,
    }

    impl Field {
        fn new(live: bool, polishing: bool) -> Field {
            Field { text: String::new(), composing: String::new(), composed: Composed::new(live, polishing), told: Vec::new() }
        }

        fn holding(text: &str, live: bool, polishing: bool) -> Field {
            Field { text: text.into(), ..Field::new(live, polishing) }
        }

        fn hear(&mut self, event: Event) -> &mut Field {
            for tell in self.composed.on(event) {
                match tell {
                    Tell::Show(text) => {
                        self.told.push(format!("~{text}"));
                        self.composing = text;
                    }
                    Tell::Commit(text) => {
                        self.text.push_str(&text);
                        self.told.push(text);
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
            let heard = self.composed.said.clone();
            self.hear(Event::Result { text: &heard, polished: false })
        }

        fn polished(&mut self, text: &str) -> &mut Field {
            self.hear(Event::Result { text, polished: true })
        }


        /// The user presses a key: the input method is told first, and
        /// then the key does what it does.
        fn types(&mut self, text: &str) -> &mut Field {
            self.hear(Event::Key);
            assert_eq!(self.composing, "", "a key went into the field while text was being composed");
            self.text.push_str(text);
            self
        }

        /// The user clicks in the field, which lets go of what is being
        /// composed, as an input method has it do.
        fn clicks(&mut self) -> &mut Field {
            self.composing.clear();
            self.hear(Event::Reset)
        }

        /// The field loses the focus,
        fn leaves(&mut self) -> &mut Field {
            self.composing.clear();
            self.hear(Event::Left)
        }

        /// and has it again.
        fn enters(&mut self) -> &mut Field {
            self.hear(Event::Entered)
        }

        /// The session has another input method.
        fn loses_the_input_method(&mut self) -> &mut Field {
            self.composing.clear();
            self.hear(Event::Gone)
        }

        /// What was said to the input method since this was last asked.
        fn told(&mut self) -> Vec<String> {
            std::mem::take(&mut self.told)
        }

        /// What the user sees: the text, and what is being composed in brackets.
        fn seen(&self) -> String {
            match self.composing.as_str() {
                "" => self.text.clone(),
                composing => format!("{}[{composing}]", self.text),
            }
        }
    }

    const NOTHING: [&str; 0] = [];

    #[test]
    fn the_sentence_in_progress_is_composed_and_the_finished_one_committed() {
        let mut field = Field::holding("Dear Ada, ", true, false);
        field.partial("The weather").partial("The whether is").partial("The weather is nice");
        // Replaced whole each time: nothing is compared, nothing erased.
        assert_eq!(field.told(), ["~The weather", "~The whether is", "~The weather is nice"]);
        assert_eq!(field.seen(), "Dear Ada, [The weather is nice]");
        // The same again is not said again.
        field.partial("The weather is nice");
        assert_eq!(field.told(), NOTHING);
        field.sentence("The weather is nice today.");
        assert_eq!(field.told(), ["The weather is nice today.", "~"]);
        field.partial(" See who").sentence(" See you at 3:30.").ends();
        assert_eq!(field.told(), ["~ See who", " See you at 3:30.", "~"]);
        assert_eq!(field.seen(), "Dear Ada, The weather is nice today. See you at 3:30.");
        assert_eq!((field.composed.shortfall(), field.composed.standing()), (None, None));
    }

    #[test]
    fn any_text_can_be_composed() {
        // Nothing is erased, so nothing has to be counted: marks that
        // combine, emoji put together from several, line ends.
        let mut field = Field::new(true, false);
        field.partial("Cafe\u{301}").partial("Cafe\u{301} 👩\u{200d}💻").sentence("Cafe\u{301} 👩\u{200d}💻\n三明治。").ends();
        assert_eq!(field.told(), ["~Cafe\u{301}", "~Cafe\u{301} 👩\u{200d}💻", "Cafe\u{301} 👩\u{200d}💻\n三明治。", "~"]);
    }

    #[test]
    fn a_sentence_in_progress_that_came_to_nothing_is_no_longer_composed() {
        let mut field = Field::holding("Notes ", true, false);
        field.sentence("One.").partial(" Yeah").ends();
        assert_eq!(field.told(), ["One.", "~ Yeah", "~"]);
        assert_eq!(field.seen(), "Notes One.");
        // Nothing was heard at all.
        let mut field = Field::new(true, false);
        field.ends();
        assert_eq!((field.told(), field.seen()), (Vec::new(), String::new()));
    }

    #[test]
    fn a_key_of_the_users_commits_what_is_composed_and_composing_goes_on_behind_it() {
        let mut field = Field::holding("Note: ", true, false);
        field.partial("The weather is").types("!");
        assert_eq!(field.told(), ["~The weather is", "The weather is", "~"]);
        assert_eq!(field.seen(), "Note: The weather is!");
        // What is heard next is composed less what stands there already,
        field.partial("The weather is nice today");
        assert_eq!(field.told(), ["~ nice today"]);
        assert_eq!(field.seen(), "Note: The weather is![ nice today]");
        // and so is the sentence when it is finished.
        field.sentence("The weather is nice today, so let us walk.").partial(" I will").sentence(" I will bring coffee.").ends();
        assert_eq!(field.told(), [" nice today, so let us walk.", "~", "~ I will", " I will bring coffee.", "~"]);
        assert_eq!(field.seen(), "Note: The weather is! nice today, so let us walk. I will bring coffee.");
        assert_eq!(field.composed.shortfall(), None);
    }

    #[test]
    fn a_key_with_nothing_being_composed_is_only_a_key() {
        let mut field = Field::new(true, false);
        field.types("a").sentence("One.").types("\n").partial(" Two").sentence(" Two.").ends();
        assert_eq!(field.told(), ["One.", "~ Two", " Two.", "~"]);
        assert_eq!(field.seen(), "aOne.\n Two.");
    }

    #[test]
    fn what_was_kept_wrongly_stays_wrong() {
        // It is the application's: only what the two begin with in common
        // is not said again.
        let mut field = Field::new(true, false);
        field.partial("See who had").types(" ").partial("See you at").sentence("See you at three.").ends();
        assert_eq!(field.told(), ["~See who had", "See who had", "~", "~you at", "you at three.", "~"]);
        assert_eq!(field.seen(), "See who had you at three.");
        // A word that was kept wrongly is followed by the word, whole.
        let mut field = Field::new(true, false);
        field.partial("let us watch").types("#").partial("let us walk to").sentence("let us walk to the park.").ends();
        assert_eq!(field.told(), ["~let us watch", "let us watch", "~", "~walk to", "walk to the park.", "~"]);
        assert_eq!(field.seen(), "let us watch#walk to the park.");
    }

    #[test]
    fn after_a_click_in_the_field_what_is_composed_is_shown_again() {
        let mut field = Field::holding("Dear Ada", true, false);
        field.partial("Hello wor").clicks();
        assert_eq!(field.told(), ["~Hello wor", "~Hello wor"]);
        assert_eq!(field.seen(), "Dear Ada[Hello wor]");
        // A field that lets go of it at once is not shown it over and over:
        // it is shown what is heard next.
        field.clicks().clicks();
        assert_eq!((field.told(), field.seen().as_str()), (Vec::new(), "Dear Ada"));
        field.partial("Hello world").clicks().sentence("Hello world.").ends();
        assert_eq!(field.told(), ["~Hello world", "~Hello world", "Hello world.", "~"]);
        assert_eq!(field.seen(), "Dear AdaHello world.");
    }

    #[test]
    fn words_spoken_between_two_fields_go_into_the_second() {
        let mut field = Field::new(true, false);
        field.sentence("Zero.").partial(" One two").leaves();
        // Nothing of the sentence that was not finished is left behind.
        assert_eq!(field.seen(), "Zero.");
        assert!(!field.composed.live());
        // Nothing can be said meanwhile, and nothing is forgotten.
        field.partial(" One two three").sentence(" One two three.").partial(" Four");
        assert_eq!(field.told(), ["Zero.", "~ One two"]);
        field.enters();
        assert_eq!(field.told(), [" One two three.", "~ Four"]);
        assert!(field.composed.live());
        field.sentence(" Four five.").ends();
        assert_eq!(field.seen(), "Zero. One two three. Four five.");
        assert_eq!(field.composed.standing(), None);
    }

    #[test]
    fn words_spoken_while_a_hotkey_is_held_go_in_when_it_is_let_go() {
        // The desktop has the keyboard from the first word on.
        let mut field = Field::holding("Notes: ", true, false);
        field.leaves().partial("The weather").sentence("The weather is nice.").partial(" See");
        assert_eq!((field.told(), field.seen()), (Vec::new(), "Notes: ".to_owned()));
        field.enters().sentence(" See you.").ends();
        assert_eq!(field.told(), ["The weather is nice.", "~ See", " See you.", "~"]);
        assert_eq!(field.seen(), "Notes: The weather is nice. See you.");
    }

    #[test]
    fn a_recording_that_ends_with_no_field_to_say_it_in_is_for_someone_else_to_finish() {
        let mut field = Field::new(true, false);
        field.sentence("One.").partial(" Two th").leaves().sentence(" Two three.");
        assert_eq!(field.composed.standing(), None);
        field.ends();
        // The event was answered with nothing, and is still to be acted on.
        assert_eq!(field.told(), ["One.", "~ Two th"]);
        assert_eq!(field.composed.standing(), Some(Standing { said: "One. Two three.".into(), put: 4, there: String::new() }));
        assert_eq!(field.composed.shortfall(), None);
        // From then on nothing is said, whatever is heard.
        field.enters().partial(" Four").hear(Event::Cancelled);
        assert_eq!(field.told(), NOTHING);

        // With nothing owed there is nothing to finish.
        let mut field = Field::new(true, false);
        field.sentence("One.").leaves().ends();
        assert_eq!(field.composed.standing(), None);
    }

    #[test]
    fn when_the_input_method_is_no_longer_heyos_the_rest_is_for_someone_else() {
        let mut field = Field::new(true, false);
        field.sentence("One.").partial(" Two th").loses_the_input_method();
        assert_eq!(field.seen(), "One.");
        assert_eq!(field.composed.standing(), Some(Standing { said: "One.".into(), put: 4, there: String::new() }));
        assert!(!field.composed.live());
        field.partial(" Two three").sentence(" Two three.").ends();
        assert_eq!(field.told(), ["One.", "~ Two th"]);
        // What is heard after that is not noted here: whoever goes on hears it.
        assert_eq!(field.composed.standing().unwrap().said, "One.");

        // What a key had made the application's is part of what it holds.
        let mut field = Field::new(true, false);
        field.partial("One tw").types("!").partial("One two thr").loses_the_input_method();
        assert_eq!(field.seen(), "One tw!");
        assert_eq!(field.composed.standing(), Some(Standing { said: String::new(), put: 0, there: "One tw".into() }));
    }

    #[test]
    fn with_polishing_the_whole_recording_is_composed_until_the_polished_text_is_there() {
        let mut field = Field::holding("> ", true, true);
        field.partial("um so the meeting").sentence("um so the meeting is on wednesday.").partial(" no thurs").sentence(" no thursday.");
        assert_eq!(field.told(), ["~um so the meeting", "~um so the meeting is on wednesday.", "~um so the meeting is on wednesday. no thurs", "~um so the meeting is on wednesday. no thursday."]);
        assert_eq!(field.seen(), "> [um so the meeting is on wednesday. no thursday.]");
        field.polished("The meeting is on Thursday.");
        assert_eq!(field.told(), ["The meeting is on Thursday.", "~"]);
        assert_eq!(field.seen(), "> The meeting is on Thursday.");
        assert_eq!(field.composed.shortfall(), None);
    }

    #[test]
    fn where_polishing_changed_nothing_or_failed_the_text_is_committed_as_heard() {
        for polished in [true, false] {
            let mut field = Field::new(true, true);
            field.sentence("All good.").partial(" Mm").hear(Event::Result { text: "All good.", polished });
            assert_eq!(field.told(), ["~All good.", "~All good. Mm", "All good.", "~"]);
            assert_eq!((field.seen().as_str(), field.composed.shortfall()), ("All good.", None));
        }
    }

    #[test]
    fn polished_text_is_left_out_where_part_of_the_recording_is_the_applications_already() {
        // The user typed during the recording,
        let mut field = Field::new(true, true);
        field.partial("um hello").types("?").sentence("um hello there.").polished("Hello there.");
        assert_eq!(field.told(), ["~um hello", "um hello", "~", "~ there.", " there.", "~"]);
        assert_eq!(field.seen(), "um hello? there.");
        assert_eq!(field.composed.shortfall(), Some(Shortfall::NotPolished(KEPT_AS_HEARD)));
        // or while it was being polished,
        let mut field = Field::new(true, true);
        field.sentence("um hello there.").types(" ok").polished("Hello there.");
        assert_eq!(field.told(), ["~um hello there.", "um hello there.", "~"]);
        assert_eq!(field.seen(), "um hello there. ok");
        assert_eq!(field.composed.shortfall(), Some(Shortfall::NotPolished(KEPT_AS_HEARD)));
    }

    #[test]
    fn polished_text_still_takes_the_place_of_the_whole_after_a_click_or_another_window() {
        // Neither leaves anything of the recording in the application.
        let mut field = Field::new(true, true);
        field.partial("um hel").leaves().sentence("um hello.").enters().clicks().polished("Hello.");
        assert_eq!(field.told(), ["~um hel", "~um hello.", "~um hello.", "Hello.", "~"]);
        assert_eq!((field.seen().as_str(), field.composed.shortfall()), ("Hello.", None));
    }

    #[test]
    fn polished_text_with_no_field_to_commit_it_in_is_for_someone_else_to_write() {
        let mut field = Field::new(false, true);
        field.sentence("um hello.").leaves().polished("Hello.");
        assert_eq!(field.told(), NOTHING);
        // Nothing of the recording is in the application: it can still be
        // written polished.
        assert_eq!(field.composed.standing(), Some(Standing { said: "um hello.".into(), put: 0, there: String::new() }));
        assert_eq!(field.composed.shortfall(), None);
    }

    #[test]
    fn a_cancelled_recording_is_no_longer_composed_and_what_was_committed_stays() {
        // With polishing nothing was committed: nothing is left.
        let mut field = Field::holding("Dear Ada, ", true, true);
        field.sentence("One.").partial(" Two th").hear(Event::Cancelled);
        assert_eq!(field.told(), ["~One.", "~One. Two th", "~"]);
        assert_eq!((field.seen().as_str(), field.composed.shortfall()), ("Dear Ada, ", None));
        // A finished sentence is the application's, and that is said.
        let mut field = Field::holding("Dear Ada, ", true, false);
        field.sentence("One.").partial(" Two th").hear(Event::Cancelled);
        assert_eq!(field.told(), ["One.", "~ Two th", "~"]);
        assert_eq!((field.seen().as_str(), field.composed.shortfall()), ("Dear Ada, One.", Some(Shortfall::NotTakenBack)));
        // So is what was committed ahead of a key.
        let mut field = Field::new(true, true);
        field.partial("One").types("!").hear(Event::Cancelled);
        assert_eq!((field.seen().as_str(), field.composed.shortfall()), ("One!", Some(Shortfall::NotTakenBack)));
        // Nothing was said: nothing to say.
        let mut field = Field::new(true, false);
        field.hear(Event::Cancelled);
        assert_eq!((field.told(), field.composed.shortfall()), (Vec::new(), None));
    }

    #[test]
    fn a_recording_that_breaks_off_leaves_what_is_on_screen() {
        let mut field = Field::new(true, false);
        field.sentence("One.").partial(" Two th").hear(Event::Failed);
        assert_eq!(field.told(), ["One.", "~ Two th", " Two th", "~"]);
        assert_eq!(field.seen(), "One. Two th");
        let mut field = Field::new(true, true);
        field.sentence("um one.").hear(Event::Failed);
        assert_eq!(field.seen(), "um one.");
    }

    #[test]
    fn without_live_words_nothing_is_shown_and_sentences_are_committed() {
        let mut field = Field::new(false, false);
        field.partial("Hel").partial("Hello").sentence("Hello.").partial(" Wor").types("!").sentence(" World.").ends();
        assert_eq!(field.told(), ["Hello.", " World."]);
        assert_eq!(field.seen(), "Hello.! World.");
        assert!(!field.composed.live());
        // With polishing, the polished text once at the end;
        let mut field = Field::new(false, true);
        field.partial("um hel").sentence("um hello.").types("x").polished("Hello.");
        assert_eq!(field.told(), ["Hello."]);
        assert_eq!((field.seen().as_str(), field.composed.shortfall()), ("xHello.", None));
        // nothing if it is cancelled or nothing was heard.
        let mut field = Field::new(false, true);
        field.sentence("um hello.").hear(Event::Cancelled);
        assert_eq!((field.told(), field.composed.shortfall()), (Vec::new(), None));
        let mut field = Field::new(false, true);
        field.ends();
        assert_eq!(field.told(), NOTHING);
    }

    /// Every event, with and without polishing, from the same beginning: a
    /// finished sentence, and one in progress that is being composed.
    #[test]
    fn what_each_event_says_with_and_without_polishing() {
        type Step = fn(&mut Field) -> &mut Field;
        // What happens; then, without polishing and with it, what is said and what the user sees.
        let table: &[(&str, Step, [(&[&str], &str); 2])] = &[
            ("more of the sentence", |f| f.partial(" Two three"), [(&["~ Two three"], "One.[ Two three]"), (&["~One. Two three"], "[One. Two three]")]),
            ("the sentence finished", |f| f.sentence(" Two three."), [(&[" Two three.", "~"], "One. Two three."), (&["~One. Two three."], "[One. Two three.]")]),
            ("the end, as heard", |f| f.ends(), [(&["~"], "One."), (&["One.", "~"], "One.")]),
            ("the end, polished", |f| f.polished("First."), [(&["~"], "One."), (&["First.", "~"], "First.")]),
            ("cancelled", |f| f.hear(Event::Cancelled), [(&["~"], "One."), (&["~"], "")]),
            ("broken off", |f| f.hear(Event::Failed), [(&[" Two", "~"], "One. Two"), (&["One. Two", "~"], "One. Two")]),
            ("a key", |f| f.types("x"), [(&[" Two", "~"], "One. Twox"), (&["One. Two", "~"], "One. Twox")]),
            ("a click", |f| f.clicks(), [(&["~ Two"], "One.[ Two]"), (&["~One. Two"], "[One. Two]")]),
            ("the focus lost", |f| f.leaves(), [(&[], "One."), (&[], "")]),
            ("the focus lost and found", |f| f.leaves().enters(), [(&["~ Two"], "One.[ Two]"), (&["~One. Two"], "[One. Two]")]),
            ("another input method", |f| f.loses_the_input_method(), [(&[], "One."), (&[], "")]),
        ];
        for (what, step, expected) in table {
            for (polishing, (told, seen)) in [false, true].into_iter().zip(expected) {
                let mut field = Field::new(true, polishing);
                field.sentence("One.").partial(" Two").told();
                step(&mut field);
                assert_eq!((field.told(), field.seen().as_str()), (told.iter().map(|tell| tell.to_string()).collect(), *seen), "{what}, polishing: {polishing}");
            }
        }
    }
}
