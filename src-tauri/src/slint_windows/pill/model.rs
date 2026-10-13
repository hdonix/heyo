//! What the dictation pill shows of a recording, as the host tells it of
//! one (`dictation::Told`), and what it tells the host when it is pressed.
//! Nothing here draws or knows of a window: this is what is tested.

use crate::dictation::Told;
use crate::i18n::t;

/// Where a recording is, as the pill shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Asked for; the microphone is not open yet.
    Loading,
    Listening,
    Polishing,
    Done,
    Error,
}

/// What the pill tells the host of what was done to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Did {
    /// A mouse button went down on it: said at once, before the click it
    /// may become, so that the host does not take it for a click in the
    /// application being written into.
    Pressed,
    /// It was clicked while recording: the recording is to finish.
    Finish,
    /// Its button was clicked: the recording is to be cancelled.
    Cancel,
}

/// Levels kept for the waveform: the newest is drawn in the middle and the
/// ones before it on either side, so that the bars swell from the centre.
const KEPT: usize = 3;

/// How tall a bar is when there is nothing to hear, as a share of the
/// tallest: a quiet room still shows a faint line.
const FAINT: f32 = 0.18;

pub(crate) struct Model {
    pub phase: Phase,
    /// The sentences that are finished, and the one in progress.
    committed: String,
    partial: String,
    /// Said in place of the words: how the recording ended, or what failed.
    note: String,
    /// The words are going into the application as they are spoken.
    live: bool,
    levels: [f32; KEPT],
    /// The loudest level lately, slowly forgotten.
    peak: f32,
}

impl Model {
    pub(crate) fn new() -> Model {
        Model { phase: Phase::Loading, committed: String::new(), partial: String::new(), note: String::new(), live: false, levels: [0.0; KEPT], peak: 0.2 }
    }

    /// The host says something of a recording.
    pub(crate) fn told(&mut self, told: &Told) {
        match told {
            Told::Start { .. } => {
                // The next recording starts with nothing decided. (Not the
                // peak: the microphone is the same one.)
                *self = Model { peak: self.peak, ..Model::new() };
            }
            Told::Ready => self.phase = Phase::Listening,
            Told::Level { value } => {
                // Scaled to the loudest recent level, so a quiet microphone
                // (a Bluetooth headset, say) still fills the bars when
                // spoken to.
                let value = value.unwrap_or(0.0) as f32;
                self.peak = value.max(self.peak * 0.995).max(0.2);
                self.levels.rotate_left(1);
                self.levels[KEPT - 1] = (value / self.peak).min(1.0);
            }
            Told::Partial { text } => self.partial = text.clone(),
            Told::Segment { text } => {
                self.committed.push_str(text);
                self.partial.clear();
            }
            Told::Writing { live } => self.live = *live,
            Told::Polishing => {
                self.phase = Phase::Polishing;
                self.partial.clear();
            }
            Told::Result { text, raw, warning, cancelled, kept } => {
                self.phase = Phase::Done;
                self.partial.clear();
                if let Some(warning) = warning.as_deref().filter(|warning| !warning.is_empty()) {
                    self.note = crate::i18n::tf("Not polished: {0}", &[&warning]);
                } else if *cancelled == Some(true) {
                    self.note = if *kept == Some(true) { t("Cancelled. What was written is still there.") } else { t("Cancelled") }.into();
                } else if raw.is_empty() {
                    self.note = t("Didn't catch anything").into();
                } else if !text.is_empty() {
                    self.committed = text.clone();
                }
            }
            Told::Error { message } => {
                self.phase = Phase::Error;
                self.note = message.clone();
            }
        }
    }

    /// Whether a recording is under way: only then does a click do
    /// anything, and only then is there a way out of it.
    pub(crate) fn recording(&self) -> bool {
        matches!(self.phase, Phase::Loading | Phase::Listening)
    }

    /// The words it shows: what is settled, and after it what may still
    /// change. Both empty when it has none to show: it then says what it
    /// is doing (`saying`), or a note. The words are not shown while they
    /// are going into the application as they are spoken: they are there
    /// to be read.
    pub(crate) fn words(&self) -> (&str, &str) {
        if !self.note.is_empty() || self.live {
            ("", "")
        } else {
            (&self.committed, &self.partial)
        }
    }

    /// What it says in place of the words: how the recording ended, or
    /// what failed. Not words that were spoken: of those the newest are
    /// what must be seen, of this the beginning.
    pub(crate) fn note(&self) -> &str {
        &self.note
    }

    /// What it says of itself when it has no words to show.
    pub(crate) fn saying(&self) -> &'static str {
        match self.phase {
            Phase::Loading => t("Starting…"),
            Phase::Listening => t("Listening"),
            Phase::Polishing => t("Polishing…"),
            Phase::Done => t("Done"),
            Phase::Error => "",
        }
    }

    /// The bars of the waveform, each as a share of the tallest: older,
    /// newer, newest, newer, older. They move only while it records.
    pub(crate) fn bars(&self) -> [f32; 5] {
        let level = |at: usize| if self.recording() { self.levels[at].max(FAINT) } else { FAINT };
        [level(0), level(1), level(2), level(1), level(0)]
    }

    /// A mouse button went down anywhere on the pill's window.
    pub(crate) fn pressed(&self) -> Vec<Did> {
        vec![Did::Pressed]
    }

    /// The pill was clicked. Once the recording is over a click does
    /// nothing.
    pub(crate) fn clicked(&self) -> Vec<Did> {
        if self.recording() { vec![Did::Finish] } else { Vec::new() }
    }

    /// Its cancel button was clicked.
    pub(crate) fn cancelled(&self) -> Vec<Did> {
        if self.recording() { vec![Did::Cancel] } else { Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(polish: &str) -> Told {
        Told::Start { polish: polish.into() }
    }

    fn partial(text: &str) -> Told {
        Told::Partial { text: text.into() }
    }

    fn segment(text: &str) -> Told {
        Told::Segment { text: text.into() }
    }

    fn result(text: &str, raw: &str) -> Told {
        Told::Result { text: text.into(), raw: raw.into(), warning: None, cancelled: None, kept: None }
    }

    fn told(model: &mut Model, events: &[Told]) {
        for event in events {
            model.told(event);
        }
    }

    /// What is read on the pill, a note or the words, or None when it says
    /// what it is doing instead.
    fn shown(model: &Model) -> Option<String> {
        let (settled, changing) = model.words();
        if !model.note().is_empty() {
            return Some(model.note().to_string());
        }
        (!settled.is_empty() || !changing.is_empty()).then(|| format!("{settled}{changing}"))
    }

    #[test]
    fn it_shows_the_words_as_they_are_recognized_where_nothing_else_does() {
        let mut model = Model::new();
        told(&mut model, &[start("off"), Told::Ready]);
        assert_eq!((shown(&model), model.saying()), (None, "Listening"));
        told(&mut model, &[partial("The weather")]);
        assert_eq!(shown(&model).as_deref(), Some("The weather"));
        // Each sentence is pasted when it is finished: the window is where to follow.
        told(&mut model, &[Told::Writing { live: false }, segment("The weather is nice."), partial(" See you")]);
        assert_eq!(model.words(), ("The weather is nice.", " See you"));
        told(&mut model, &[result("The weather is nice. See you soon.", "The weather is nice. See you soon.")]);
        assert_eq!(shown(&model).as_deref(), Some("The weather is nice. See you soon."));
        assert_eq!(model.phase, Phase::Done);
    }

    #[test]
    fn it_does_not_repeat_words_that_are_going_into_the_application_as_they_are_spoken() {
        let mut model = Model::new();
        told(&mut model, &[start("off"), Told::Ready, Told::Writing { live: true }, partial("The weather")]);
        assert_eq!(shown(&model), None);
        told(&mut model, &[segment("The weather is nice.")]);
        assert_eq!(shown(&model), None);
        // The user typed: from here the words are only pasted when a sentence is
        // finished, and the window shows them again, all of them.
        told(&mut model, &[Told::Writing { live: false }, partial(" See you")]);
        assert_eq!(shown(&model).as_deref(), Some("The weather is nice. See you"));
        // The next recording starts with nothing decided.
        told(&mut model, &[start("off"), partial("Hello")]);
        assert_eq!(shown(&model).as_deref(), Some("Hello"));
    }

    #[test]
    fn it_says_what_could_not_be_done_at_the_end() {
        let mut model = Model::new();
        told(&mut model, &[start("clean"), Told::Writing { live: true }, segment("um hello."), Told::Polishing]);
        assert_eq!((model.phase, model.saying()), (Phase::Polishing, "Polishing…"));
        let warning = "it could not safely take the place of the text, which was left as it was heard";
        told(&mut model, &[Told::Result { text: "um hello.".into(), raw: "um hello.".into(), warning: Some(warning.into()), cancelled: None, kept: None }]);
        assert_eq!(shown(&model), Some(format!("Not polished: {warning}")));

        let cancelled = |kept| Told::Result { text: String::new(), raw: String::new(), warning: None, cancelled: Some(true), kept };
        told(&mut model, &[start("off"), Told::Writing { live: true }, segment("Hello."), cancelled(None)]);
        assert_eq!(shown(&model).as_deref(), Some("Cancelled"));
        told(&mut model, &[start("off"), Told::Writing { live: true }, segment("Hello."), cancelled(Some(true))]);
        assert_eq!(shown(&model).as_deref(), Some("Cancelled. What was written is still there."));
        told(&mut model, &[start("off"), result("", "")]);
        assert_eq!(shown(&model).as_deref(), Some("Didn't catch anything"));
        told(&mut model, &[start("off"), Told::Error { message: "no microphone".into() }]);
        assert_eq!((shown(&model).as_deref(), model.phase), (Some("no microphone"), Phase::Error));
    }

    #[test]
    fn a_note_is_not_shown_as_words_are() {
        // Words are cut off at their beginning when there are too many, a
        // note at its end: the pill is told which it has.
        let mut model = Model::new();
        let unready = "Dictation is not set up yet. Open Heyo Settings → Dictation to download it.";
        told(&mut model, &[start("off"), Told::Error { message: unready.into() }]);
        assert_eq!((model.note(), model.words()), (unready, ("", "")));
        // Also when words were heard before it.
        told(&mut model, &[start("off"), segment("Hello."), partial(" See"), Told::Result { text: String::new(), raw: String::new(), warning: None, cancelled: Some(true), kept: None }]);
        assert_eq!((model.note(), model.words()), ("Cancelled", ("", "")));
        // The next recording has none.
        told(&mut model, &[start("off"), partial("Hello")]);
        assert_eq!((model.note(), model.words()), ("", ("", "Hello")));
    }

    #[test]
    fn a_press_on_it_is_told_at_once_and_a_click_finishes_or_cancels() {
        let mut model = Model::new();
        told(&mut model, &[start("off"), Told::Ready]);
        // The button going down, before it is a click: it is not a click in the
        // application being written into.
        assert_eq!(model.pressed(), [Did::Pressed]);
        assert_eq!(model.clicked(), [Did::Finish]);
        assert_eq!(model.cancelled(), [Did::Cancel]);
        // Once the recording is over a click does nothing, but is still not the user's.
        told(&mut model, &[Told::Polishing]);
        assert_eq!(model.pressed(), [Did::Pressed]);
        assert!(model.clicked().is_empty() && model.cancelled().is_empty());
    }

    #[test]
    fn the_bars_swell_from_the_middle_with_the_newest_level() {
        let mut model = Model::new();
        told(&mut model, &[start("off"), Told::Ready]);
        // A quiet room still shows a faint line.
        assert_eq!(model.bars(), [FAINT; 5]);
        told(&mut model, &[Told::Level { value: Some(0.05) }, Told::Level { value: Some(0.1) }, Told::Level { value: Some(0.4) }]);
        let bars = model.bars();
        // Scaled to the loudest lately: the newest, in the middle, is full.
        assert_eq!(bars[2], 1.0);
        assert!(bars[1] == bars[3] && bars[0] == bars[4] && bars[1] < bars[2] && bars[0] < bars[1], "{bars:?}");
        // A level that could not be measured is silence.
        told(&mut model, &[Told::Level { value: None }]);
        assert_eq!(model.bars()[2], FAINT);
        // Once it no longer records they are still.
        told(&mut model, &[result("Hello.", "Hello.")]);
        assert_eq!(model.bars(), [FAINT; 5]);
    }
}
