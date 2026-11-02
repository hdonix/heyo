//! Dictation: the model that hears, the hotkey and the key held to talk,
//! typing as one speaks, the microphone and language, the words to know,
//! and polishing what was said.

use super::models::{listings, pick, picker, to_models};
use super::{on, tile, Known, Page, Reaction, Sheet};
use crate::commands::settings::HotkeyName;
use crate::i18n::{marked, t, tf, translate};
use crate::models::registry::Use;
use crate::platform::{Support, Words};
use crate::slint_windows::host::Command;
use crate::slint_windows::ids::{Act, Bound, Id, PageId, Setting};
use crate::slint_windows::model::{ask, Ask, Entry, Line, Then, DICTATION_LANGUAGES};

pub(crate) const PAGE: &str = "dictation";

const POLISH_MODES: &[(&str, &str, &str)] = &[
    ("off", marked("Off"), marked("The text is left as it was recognized.")),
    ("clean", marked("Clean Up"), marked("When you finish, the model removes filler words, applies your corrections and fixes punctuation, then the result is pasted.")),
    ("translate", marked("Translate to English"), marked("When you finish, the model cleans up what you said and pastes it in English.")),
];

pub(crate) struct Dictation;

/// The ways dictated words can reach an application in this session.
fn ways(known: &Known) -> &[Words] {
    known.capabilities.as_ref().map_or(&[], |capabilities| capabilities.dictation_words.as_slice())
}

/// What "Type as You Speak" says of this session, by the ways dictated
/// words can reach an application in it.
fn live_description(known: &Known) -> String {
    let ways = ways(known);
    match ways.first() {
        None => String::new(),
        Some(Words::Composed) => tf(
            "Your words go into the window as you speak, through the input method: the sentence you are speaking is shown there as text being composed, and becomes the window's text when it is finished. Your own input method (Pinyin, say) is set aside while you dictate and comes back when you finish. {} When this is off, nothing is shown before a sentence is finished.",
            &[&if ways.contains(&Words::Rewritten) {
                t("A window that takes no input method gets the words by pasting, corrected where they stand until you press a key or click.")
            } else {
                t("A window that takes no input method gets each sentence pasted when it is finished.")
            }],
        ),
        Some(Words::Rewritten) => t("Your words go into the window as they are recognized, and are corrected there when a later word changes an earlier one. Heyo only ever erases what it wrote itself: once you press a key or click while dictating, it stops correcting and adds each finished sentence where the cursor is. In terminals, password managers and the applications excluded from snippet expansion, and when this is off, each sentence is pasted when it is finished.").into(),
        Some(_) if known.capabilities.as_ref().is_some_and(|capabilities| capabilities.session == "wayland") => t("Not available on Wayland, where Heyo cannot tell whether you typed in between, and so must not erase anything. Each sentence is pasted when it is finished.").into(),
        Some(_) => t("Not available in this desktop session. Each sentence is pasted when it is finished.").into(),
    }
}

/// What the Dictation page, and the Models page, need to know besides.
pub(crate) fn needed(known: &Known) -> Vec<Ask> {
    [vec![ask(Command::DictationStatus, Then::Known), ask(Command::SpeechModels, Then::Known)], listings(known)].concat()
}

impl Page for Dictation {
    fn entries(&self, _: &Known) -> Vec<Entry> {
        vec![tile(PAGE, marked("Dictation"), "microphone-16")]
    }

    fn shows(&self, at: &PageId) -> bool {
        *at == PageId::own(PAGE)
    }

    fn lines(&self, _: &PageId, known: &Known) -> Vec<Line> {
        let settings = &known.settings.settings;
        let mut page = Sheet::default();
        page.note(t("Hold the hotkey while you speak and let go, or tap it to start and tap again to finish: what you said is typed into the window you are working in."));
        // Speech models are downloaded on the Models page.
        let engine = vec![picker(known, Use::DictationRecognition, t("Speech Recognition"), t("Models on this computer keep your audio here; a service is sent it. Download speech models in Models."), None)];
        let hotkey = Line::row("", t("Hotkey"));
        let hotkey = if known.hotkeys_supported() { hotkey.says(t("Tap to start and tap again to finish, or hold it while you speak")).hotkey(&Bound::Own(HotkeyName::Dictation).to_string(), known.hotkeys.dictation.as_deref()) } else { hotkey.says(t("Bind a desktop keyboard shortcut to heyo --dictate.")) };
        let hold = Line::row(Id::Set(Setting::DictationHoldKey), t("Hold-to-Talk Key"));
        let hold = match &known.capabilities {
            Some(capabilities) if capabilities.hold_key == Support::None => hold.says(t("A single key cannot be watched in this desktop session; use the hotkey instead.")),
            capabilities => {
                let hold = hold.says(if settings.dictation_hold_key.is_empty() { t("A single key to hold while you speak. It keeps working as usual in shortcuts.") } else { t("Hold it and speak; let go to finish. A quick tap, or using it in a shortcut, does nothing.") });
                match capabilities {
                    Some(capabilities) => hold.select(&settings.dictation_hold_key, std::iter::once(("", t("None"))).chain(capabilities.hold_keys.iter().map(|key| (key.value.as_ref(), key.title.as_ref())))),
                    None => hold,
                }
            }
        };
        let mut rows = engine;
        rows.extend([hotkey, hold]);
        let live = live_description(known);
        if !live.is_empty() {
            let row = Line::row(Id::Set(Setting::DictationLive), t("Type as You Speak")).says(live);
            rows.push(if ways(known).first().is_some_and(|way| *way != Words::Pasted) { row.switch(settings.dictation_live) } else { row });
        }
        page.card("", rows);

        let microphones = known.dictation.as_ref().map_or(&[][..], |dictation| dictation.microphones.as_slice());
        let none = known.dictation.is_some() && microphones.is_empty();
        page.card(
            t("Input"),
            vec![
                Line::row(Id::Set(Setting::DictationSource), t("Microphone")).says(if none { t("No microphone found. Connect one to use dictation.") } else { "" }).warns(none).select(&settings.dictation_source, std::iter::once(("", t("System Default"))).chain(microphones.iter().map(|m| (m.name.as_str(), m.description.as_str())))),
                Line::row(Id::Set(Setting::DictationLanguage), t("Language")).says(t("Automatic detection handles mixed Chinese and English")).select(&settings.dictation_language, DICTATION_LANGUAGES.iter().map(|(value, title)| (*value, translate(title)))),
            ],
        );
        page.card(
            t("Vocabulary"),
            vec![
                Line::row("", t("Your Words")).says(t("Names, jargon and product names, one to a line, written the way you want them: “type script” then comes out as TypeScript. For a word that is heard as something else, write heard as -> written as. The recognizer cannot be taught new words, so these are put right in its text; a Chinese name heard as other characters of the same sound is only fixed if you list those characters as “heard as”. Polishing with AI is given the list too.")),
                Line { kind: "area", id: Id::Words.into(), label: t("Your Words").into(), value: settings.dictation_vocabulary.join("\n"), placeholder: "TypeScript\nKubernetes\njason -> JSON\n杰森 -> JSON".into(), ..Line::default() },
            ],
        );
        let mode = POLISH_MODES.iter().find(|(value, ..)| *value == settings.dictation_polish).unwrap_or(&POLISH_MODES[0]);
        let mut rows = vec![Line::row(Id::Set(Setting::DictationPolish), t("Mode")).says(translate(mode.2)).select(mode.0, POLISH_MODES.iter().map(|(value, title, _)| (*value, translate(title))))];
        if mode.0 != "off" {
            rows.push(picker(known, Use::DictationPolish, t("Model"), "", None));
        }
        page.card(t("Polish with AI"), rows);
        page.card("", vec![to_models()]);
        page.done()
    }

    fn acted(&mut self, _: &PageId, known: &mut Known, id: &Id, value: &str) -> Option<Reaction> {
        let text = value.to_string();
        let asks = match id {
            Id::Set(Setting::DictationHoldKey) => known.change(|view| view.settings.dictation_hold_key = text),
            Id::Set(Setting::DictationLive) => known.change(|view| view.settings.dictation_live = on(value)),
            Id::Set(Setting::DictationSource) => known.change(|view| view.settings.dictation_source = text),
            Id::Set(Setting::DictationLanguage) => known.change(|view| view.settings.dictation_language = text),
            Id::Set(Setting::DictationPolish) => known.change(|view| view.settings.dictation_polish = text),
            Id::Words => {
                let lines: Vec<String> = value.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect();
                if lines == known.settings.settings.dictation_vocabulary { Vec::new() } else { known.change(|view| view.settings.dictation_vocabulary = lines) }
            }
            Id::Pick(need) => pick(known, *need, value),
            Id::Act(Act::OpenModels) => return Some(Reaction::showing(PageId::own(super::models::PAGE))),
            _ => return None,
        };
        Some(Reaction::asking(asks))
    }

    fn entered(&self, _: &PageId, known: &Known) -> Vec<Ask> {
        needed(known)
    }
}
