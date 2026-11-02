//! One recording, as a recognizer that is not a streaming one hears it:
//! audio goes in, and out come the sentence being spoken, as far as it is
//! recognized (`Partial`), and each sentence once the speaker has paused
//! after it (`Sentence`).
//!
//! The recognizer is given the whole sentence so far, again and again, and
//! may write its beginning differently each time. Whoever shows the partial
//! text must be ready to correct it.
//!
//! What is said here is text as it is to be written one piece after the
//! other: a sentence begins with the space that separates it from the one
//! before, where the language has one, so that the pieces only need joining.

use std::sync::LazyLock;

use regex::Regex;

use super::{Recognized, SAMPLE_RATE};

/// The samples the voice-activity detector looks at a time.
pub(crate) const VAD_WINDOW: usize = 512;
/// How often the sentence in progress is re-recognized.
pub(crate) const PARTIAL_INTERVAL_MS: u64 = 300;
/// Shorter audio than this decodes to noise ("Yeah.", stray syllables).
const MIN_PARTIAL_SECONDS: f64 = 0.8;
/// Audio kept from before speech was detected, so first syllables survive.
const PREROLL_SECONDS: f64 = 0.4;
const PREROLL: usize = (PREROLL_SECONDS * SAMPLE_RATE as f64) as usize;

/// A stretch of speech the detector found: where it starts, counted in
/// samples since the detector was last reset, and its audio.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Segment {
    pub start: usize,
    pub samples: Vec<f32>,
}

/// The recognizer and the voice-activity detector, as much of them as a
/// recording uses (sherpa-onnx's, or a test's own).
pub(crate) trait Engine {
    /// The text of `samples`, trimmed.
    fn decode(&mut self, samples: &[f32]) -> String;
    fn accept(&mut self, window: &[f32]);
    /// Whether speech is going on.
    fn detected(&self) -> bool;
    /// The oldest sentence it has finished, if any.
    fn front(&self) -> Option<Segment>;
    fn pop(&mut self);
    /// Finishes the sentence in progress.
    fn flush(&mut self);
    /// Forgets all it has heard, and counts its samples from nothing again.
    fn reset(&mut self);
}

/// Writing, letters or digits: SenseVoice returns a lone punctuation mark
/// for noise.
fn meaningful(text: &str) -> bool {
    static WORDY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\p{N}]").unwrap());
    WORDY.is_match(text)
}

/// Chinese and Japanese writing, and the full-width marks that go with it:
/// nothing stands between two sentences there.
fn unspaced(c: char) -> bool {
    static UNSPACED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{Han}\p{Hiragana}\p{Katakana}\x{3000}-\x{303f}\x{ff00}-\x{ffef}]").unwrap());
    UNSPACED.is_match(c.encode_utf8(&mut [0; 4]))
}

/// What goes between what was said so far and the next sentence: a space,
/// unless the text before ends in writing that has none between sentences
/// (or in a space already, or there is no text before).
pub(crate) fn between(before: &str, sentence: &str) -> &'static str {
    match before.chars().last() {
        None => "",
        Some(last) if sentence.is_empty() || last.is_whitespace() || unspaced(last) || sentence.starts_with(char::is_whitespace) => "",
        Some(_) => " ",
    }
}

/// A sentence still being spoken, without the mark the recognizer ends
/// whatever it is given with: the full stop after "The weather is." would
/// only have to be taken back when the next word comes.
pub(crate) fn unfinished(text: &str) -> &str {
    static CLOSING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\s.,!?;:…。，！？；：、~—–-]+$").unwrap());
    let open = match CLOSING.find(text) {
        Some(found) => &text[..found.start()],
        None => text,
    };
    if meaningful(open) {
        open
    } else {
        text
    }
}

/// One recording: feeds audio to the detector and says partial and final text.
pub(crate) struct Session {
    pending: Vec<f32>,
    /// Audio of the sentence in progress (with pre-roll), a window each.
    speech: Vec<Vec<f32>>,
    speech_samples: usize,
    /// Samples given to the detector so far; `speech` ends here.
    fed: usize,
    speaking: bool,
    last_partial: u64,
    /// The finished sentences, each as it joins those before it.
    segments: Vec<String>,
}

impl Session {
    /// The detector outlives a recording. Left as it was, it would go on
    /// counting samples where the last one stopped, and no sentence of this
    /// one would be found in what was kept of it (`with_preroll`).
    pub fn new(engine: &mut impl Engine) -> Session {
        engine.reset();
        Session { pending: Vec::new(), speech: Vec::new(), speech_samples: 0, fed: 0, speaking: false, last_partial: 0, segments: Vec::new() }
    }

    /// Hears `samples`; `now` is the time in milliseconds.
    pub fn feed(&mut self, engine: &mut impl Engine, samples: &[f32], now: u64, say: &mut dyn FnMut(Recognized)) {
        let mut joined = std::mem::take(&mut self.pending);
        joined.extend_from_slice(samples);
        let mut windows = joined.chunks_exact(VAD_WINDOW);
        for window in &mut windows {
            engine.accept(window);
            self.speech.push(window.to_vec());
            self.speech_samples += VAD_WINDOW;
            self.fed += VAD_WINDOW;
            self.speaking |= engine.detected();
            if !self.speaking {
                self.trim_preroll();
            }
            self.drain(engine, say);
        }
        self.pending = windows.remainder().to_vec();

        let seconds = self.speech_samples as f64 / SAMPLE_RATE as f64;
        if self.speaking && seconds >= MIN_PARTIAL_SECONDS && now.saturating_sub(self.last_partial) >= PARTIAL_INTERVAL_MS {
            self.last_partial = now;
            let heard = engine.decode(&self.speech.concat());
            let text = unfinished(&heard);
            if meaningful(text) {
                say(Recognized::Partial(self.joined(text)));
            }
        }
    }

    /// The sentence as it is written after the ones before it.
    fn joined(&self, text: &str) -> String {
        between(&self.segments.concat(), text).to_owned() + text
    }

    fn trim_preroll(&mut self) {
        while let Some(first) = self.speech.first() {
            if self.speech_samples - first.len() < PREROLL {
                break;
            }
            self.speech_samples -= first.len();
            self.speech.remove(0);
        }
    }

    /// Says the sentences the detector has finished.
    fn drain(&mut self, engine: &mut impl Engine, say: &mut dyn FnMut(Recognized)) {
        while let Some(segment) = engine.front() {
            engine.pop();
            let text = engine.decode(&self.with_preroll(segment));
            self.speech.clear();
            self.speech_samples = 0;
            self.speaking = false;
            if !meaningful(&text) {
                continue;
            }
            let sentence = self.joined(&text);
            self.segments.push(sentence.clone());
            say(Recognized::Sentence(sentence));
        }
    }

    /// The detector's segment, extended backwards: it trims first syllables.
    fn with_preroll(&self, segment: Segment) -> Vec<f32> {
        let buffer = self.speech.concat();
        let buffer_start = self.fed - buffer.len();
        let end = segment.start + segment.samples.len();
        let start = buffer_start.max(segment.start.saturating_sub(PREROLL));
        // Fall back to the bare segment if the positions do not line up.
        if !(segment.start >= buffer_start && end <= self.fed && start < end) {
            return segment.samples;
        }
        buffer[start - buffer_start..end - buffer_start].to_vec()
    }

    /// Ends the recording: what was still being said is finished, and all of
    /// it is returned.
    pub fn finish(&mut self, engine: &mut impl Engine, say: &mut dyn FnMut(Recognized)) -> String {
        engine.flush();
        self.drain(engine, say);
        self.segments.concat()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recognizer and a detector for tests. Audio is words: a word is a
    /// stretch of samples of one loudness, which names it. The recognizer
    /// writes the words it is given and ends with a mark, as SenseVoice
    /// does; the detector finishes a sentence after a pause, and like the
    /// real one it notices speech late: what it calls a sentence has lost
    /// most of its first word.
    const WORD: usize = SAMPLE_RATE / 4;
    const PAUSE: usize = (0.6 * SAMPLE_RATE as f64 / VAD_WINDOW as f64).ceil() as usize;
    const LATE: usize = 5 * VAD_WINDOW;

    struct Words {
        words: Vec<&'static str>,
        mark: &'static str,
        fed: usize,
        start: Option<usize>,
        quiet: usize,
        speech: Vec<f32>,
        done: Vec<Segment>,
    }

    fn engine(words: &[&'static str], mark: &'static str) -> Words {
        Words { words: words.to_vec(), mark, fed: 0, start: None, quiet: 0, speech: Vec::new(), done: Vec::new() }
    }

    impl Engine for Words {
        fn decode(&mut self, samples: &[f32]) -> String {
            let mut heard = Vec::new();
            let mut run = 0;
            for i in 0..=samples.len() {
                if i > 0 && i < samples.len() && samples[i] == samples[i - 1] {
                    run += 1;
                    continue;
                }
                if i > 0 {
                    let named = (samples[i - 1] * 100.0).round() as usize;
                    // Half a word is not heard yet.
                    if let Some(word) = named.checked_sub(1).and_then(|n| self.words.get(n)) {
                        if run >= WORD / 2 {
                            heard.push(*word);
                        }
                    }
                }
                run = 1;
            }
            heard.join(" ") + self.mark
        }

        fn accept(&mut self, window: &[f32]) {
            let loud = window.iter().any(|&sample| sample != 0.0);
            if loud && self.start.is_none() {
                self.start = Some(self.fed + LATE);
            }
            if self.start.is_some_and(|start| self.fed >= start) {
                self.speech.extend_from_slice(window);
            }
            self.quiet = if loud { 0 } else { self.quiet + 1 };
            self.fed += window.len();
            if self.start.is_some() && self.quiet >= PAUSE {
                self.flush();
            }
        }

        fn detected(&self) -> bool {
            self.start.is_some()
        }

        fn front(&self) -> Option<Segment> {
            self.done.first().cloned()
        }

        fn pop(&mut self) {
            self.done.remove(0);
        }

        fn flush(&mut self) {
            if let Some(start) = self.start.take() {
                self.done.push(Segment { start, samples: std::mem::take(&mut self.speech) });
            }
            self.speech.clear();
        }

        fn reset(&mut self) {
            self.flush();
            self.done.clear();
            self.fed = 0;
            self.quiet = 0;
        }
    }

    /// The audio of a sentence: its words (by number, from 1), then a pause.
    fn audio(sentence: &[usize], pause: f64) -> Vec<f32> {
        let mut samples = vec![0.0; sentence.len() * WORD + (pause * SAMPLE_RATE as f64) as usize];
        for (i, word) in sentence.iter().enumerate() {
            samples[i * WORD..(i + 1) * WORD].fill(*word as f32 / 100.0);
        }
        samples
    }

    struct Recorded {
        said: Vec<Recognized>,
        text: String,
    }

    impl Recorded {
        fn texts(&self, partial: bool) -> Vec<&str> {
            self.said
                .iter()
                .filter_map(|said| match said {
                    Recognized::Partial(text) if partial => Some(text.as_str()),
                    Recognized::Sentence(text) if !partial => Some(text.as_str()),
                    _ => None,
                })
                .collect()
        }

        fn partials(&self) -> Vec<&str> {
            let mut seen: Vec<&str> = Vec::new();
            for text in self.texts(true) {
                if !seen.contains(&text) {
                    seen.push(text);
                }
            }
            seen
        }
    }

    /// Feeds a recording a tenth of a second at a time, as the recorder
    /// does, with the clock moving along, and collects what it says.
    fn record(engine: &mut Words, recording: &[Vec<f32>]) -> Recorded {
        let mut said = Vec::new();
        let mut now = 1000;
        let mut session = Session::new(engine);
        for samples in recording {
            for chunk in samples.chunks(SAMPLE_RATE / 10) {
                session.feed(engine, chunk, now, &mut |heard| said.push(heard));
                now += 100;
            }
        }
        let text = session.finish(engine, &mut |heard| said.push(heard));
        Recorded { said, text }
    }

    const WORDS: &[&str] = &["the", "weather", "is", "nice", "today", "walk", "to", "park"];

    #[test]
    fn says_the_sentence_being_spoken_as_it_grows_and_then_the_sentence() {
        let recorded = record(&mut engine(WORDS, "."), &[audio(&[1, 2, 3, 4, 5], 1.0)]);
        // Nothing before there is enough to recognize; then every 300 ms, also
        // through the pause in which the sentence might still go on.
        // What is in progress has no full stop yet: each is the last and more.
        assert_eq!(recorded.partials(), ["the weather is", "the weather is nice", "the weather is nice today"]);
        assert_eq!(recorded.texts(false), ["the weather is nice today."]);
        assert_eq!(recorded.text, "the weather is nice today.");
    }

    #[test]
    fn recognizes_the_sentence_in_progress_no_oftener_than_its_interval() {
        let recorded = record(&mut engine(WORDS, "."), &[audio(&[1, 2, 3, 4, 5, 6, 7, 8], 1.0)]);
        let partials = recorded.texts(true).len() as u64;
        // Two seconds of speech, of which the first 0.8 s say nothing.
        assert!(partials <= 2000_u64.div_ceil(PARTIAL_INTERVAL_MS), "{partials}");
        assert!(partials >= 3, "{partials}");
    }

    #[test]
    fn finishes_each_sentence_at_the_pause_after_it() {
        let recorded = record(&mut engine(WORDS, "."), &[audio(&[1, 2, 3, 4], 1.0), audio(&[6, 7, 1, 8], 1.0)]);
        // The second begins with the space that sets it off from the first.
        assert_eq!(recorded.texts(false), ["the weather is nice.", " walk to the park."]);
        assert_eq!(recorded.text, "the weather is nice. walk to the park.");
        assert!(recorded.texts(true).iter().filter(|partial| partial.contains("walk")).all(|partial| partial.starts_with(" walk")));
        // A sentence is said after the last that was heard of it in
        // progress, and nothing more is heard of it then.
        let first = recorded.said.iter().position(|said| *said == Recognized::Sentence("the weather is nice.".into())).unwrap();
        assert!(first > 0);
        assert!(recorded.said[..first].iter().all(|said| matches!(said, Recognized::Partial(text) if text.starts_with("the"))));
        assert!(recorded.said[first + 1..].iter().all(|said| matches!(said, Recognized::Partial(text) | Recognized::Sentence(text) if text.contains("walk"))));
    }

    #[test]
    fn finishes_what_was_still_being_said_when_it_is_ended() {
        let recorded = record(&mut engine(WORDS, "."), &[audio(&[1, 2, 3, 4], 0.0)]);
        assert_eq!(recorded.texts(false), ["the weather is nice."]);
        assert_eq!(recorded.text, "the weather is nice.");
    }

    #[test]
    fn keeps_the_first_word_of_a_sentence_which_the_detector_is_late_for() {
        let recorded = record(&mut engine(WORDS, "."), &[audio(&[8, 3, 4], 1.0)]);
        assert_eq!(recorded.texts(false), ["park is nice."]);
    }

    #[test]
    fn hears_a_second_recording_as_it_heard_the_first() {
        // The recognizer and detector are kept from one to the next.
        let mut kept = engine(WORDS, ".");
        let first = record(&mut kept, &[audio(&[1, 2, 3, 4], 1.0), audio(&[6, 7, 1, 8], 1.0)]);
        let second = record(&mut kept, &[audio(&[1, 2, 3, 4], 1.0), audio(&[6, 7, 1, 8], 1.0)]);
        assert_eq!(second.texts(false), first.texts(false));
        assert_eq!(second.text, "the weather is nice. walk to the park.");
    }

    #[test]
    fn joins_sentences_as_their_writing_does() {
        let chinese = ["今天", "天气", "很好", "我们", "去", "公园"];
        let recorded = record(&mut engine(&chinese, "。"), &[audio(&[1, 2, 3], 1.0), audio(&[4, 5, 6], 1.0)]);
        assert_eq!(recorded.texts(false), ["今天 天气 很好。", "我们 去 公园。"]);
        assert_eq!(recorded.text, "今天 天气 很好。我们 去 公园。");
    }

    #[test]
    fn says_nothing_of_noise() {
        // A sound the recognizer makes no word of: only a mark comes back.
        let recorded = record(&mut engine(WORDS, "."), &[audio(&[50, 50, 50, 50, 50], 1.0)]);
        assert!(recorded.texts(true).is_empty());
        assert!(recorded.texts(false).is_empty());
        assert_eq!(recorded.text, "");
    }

    #[test]
    fn between_two_sentences_is_a_space_where_sentences_are_set_apart_by_one() {
        assert_eq!(between("See you at three.", "Bring coffee."), " ");
        assert_eq!(between("Really?", "Yes!"), " ");
        assert_eq!(between("안녕하세요.", "반갑습니다."), " ");
        assert_eq!(between("no mark at the end", "Next."), " ");
        // English after Chinese needs none; Chinese after English does.
        assert_eq!(between("I use Python.", "我也是。"), " ");
    }

    #[test]
    fn between_two_sentences_is_nothing_in_chinese_and_japanese() {
        assert_eq!(between("今天天气很好。", "我们去公园。"), "");
        assert_eq!(between("今日はいい天気", "散歩しましょう。"), "");
        assert_eq!(between("真的吗？", "Yes."), "");
        assert_eq!(between("我用的是Python。", "It works."), "");
    }

    #[test]
    fn between_two_sentences_is_nothing_where_there_is_nothing_to_set_apart() {
        assert_eq!(between("", "First."), "");
        assert_eq!(between("One. ", "Two."), "");
        assert_eq!(between("One.\n", "Two."), "");
        assert_eq!(between("One.", " Two."), "");
        assert_eq!(between("One.", ""), "");
    }

    #[test]
    fn a_sentence_still_being_spoken_is_said_without_the_mark_the_recognizer_ends_it_with() {
        assert_eq!(unfinished("The weather is."), "The weather is");
        assert_eq!(unfinished("The weather is nice today, so."), "The weather is nice today, so");
        assert_eq!(unfinished("today-. "), "today");
        assert_eq!(unfinished("今天天气。"), "今天天气");
        assert_eq!(unfinished("今天天气很好，"), "今天天气很好");
        assert_eq!(unfinished("Really?!"), "Really");
        assert_eq!(unfinished("I will bring 42"), "I will bring 42");
    }

    #[test]
    fn a_sentence_still_being_spoken_keeps_the_marks_inside_it() {
        assert_eq!(unfinished("Well, yes. And no."), "Well, yes. And no");
        assert_eq!(unfinished("3.5."), "3.5");
    }

    #[test]
    fn a_sentence_still_being_spoken_is_left_alone_when_it_is_nothing_but_marks() {
        assert_eq!(unfinished("..."), "...");
        assert_eq!(unfinished("。"), "。");
        assert_eq!(unfinished(""), "");
    }
}
