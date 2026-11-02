//! What the host answers the settings window, as the window reads it.
//!
//! Each `Command` is answered with one of these (`Running::call` says
//! which), and the window model takes them as they are typed: no answer
//! is looked into by the names of its fields. Where the host's own type
//! can be read back, it is the answer itself; the macOS window, a process
//! of its own, is sent the same as JSON (`process.rs`) and reads it back
//! into the same types.
//!
//! Two answers are not the host's types but the window's reading of
//! them, by the same names: the applications (the palette's items carry
//! much the window has no use for) and dictation's (whose types are not
//! made to be read back). A test holds those to the host's.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::clipboard::meaning;
use crate::clipboard::sources::Source;
use crate::commands::media::OcrStatus;
use crate::commands::settings::{Hotkeys, SettingsView};
use crate::extensions::ExtensionInfo;
use crate::models::kinds::Checked;
use crate::permissions::Row;
use crate::platform::Capabilities;

/// What the host answered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub(crate) enum Answer {
    /// The settings, as they are now.
    Settings(Box<SettingsView>),
    Hotkeys(Hotkeys),
    Capabilities(Capabilities),
    Extensions(Vec<ExtensionInfo>),
    Applications(Vec<Application>),
    /// The hotkeys of the palette's items, by the item's id.
    ItemHotkeys(BTreeMap<String, String>),
    Dictation(DictationStatus),
    /// The speech models of this computer.
    Speech(Vec<SpeechModel>),
    Ocr(OcrStatus),
    Meaning(meaning::Status),
    /// The applications that can be left out of something.
    Sources(Vec<Source>),
    /// What an extension may ask Heyo for.
    Permissions(Vec<Row>),
    /// The models a service lists.
    Models(Vec<String>),
    /// What checking a provider came to.
    Checked(Checked),
    /// A newer version, by its number; none if this is the newest.
    Update(Option<String>),
    /// Done, with nothing to say.
    Done,
}

/// An application of the palette, as much of it as the window shows
/// (`root::Item`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Application {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
}

impl From<crate::root::Item> for Application {
    fn from(item: crate::root::Item) -> Application {
        Application { id: item.id, title: item.title, subtitle: item.subtitle, icon: item.icon }
    }
}

/// Whether dictation can be used (`dictation::Status`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(rename = "DictationStatus")]
pub(crate) struct DictationStatus {
    pub installed: bool,
    pub installing: bool,
    pub microphones: Vec<Microphone>,
}

/// A microphone (`dictation::microphone::Microphone`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub(crate) struct Microphone {
    pub name: String,
    pub description: String,
}

/// A speech model of this computer (`dictation::ModelState`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SpeechModel")]
pub(crate) struct SpeechModel {
    pub id: String,
    pub name: String,
    pub languages: String,
    pub note: String,
    pub megabytes: u32,
    pub installed: bool,
    pub in_use: bool,
    pub takes_language: bool,
    pub downloading: Option<SpeechProgress>,
}

/// How far a speech model's download is (`dictation::Progress`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(rename = "SpeechModelProgress")]
pub(crate) struct SpeechProgress {
    pub step: String,
    pub received: u64,
    pub total: u64,
}

/// One of the host's answers as the window's own type, by the names both
/// have: for those of its types that cannot be read back themselves.
pub(crate) fn read<T: Serialize, U: for<'de> Deserialize<'de>>(answer: T) -> Result<U, String> {
    serde_json::to_value(answer).and_then(serde_json::from_value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ts_rs::{Config, TypeVisitor, TS};

    /// What TypeScript is told of a type without what is said of it.
    fn undocumented(mut declared: String) -> String {
        while let Some(start) = declared.find("\n/**") {
            let end = declared[start..].find("*/\n").map_or(declared.len(), |end| start + end + 3);
            declared.replace_range(start..end, "");
        }
        declared
    }

    /// A type as TypeScript is told it, with every type it is made of: by
    /// name, each written out.
    fn shape<T: TS + 'static>() -> Vec<(String, String)> {
        struct Shapes(Config, Vec<(String, String)>);
        impl TypeVisitor for Shapes {
            fn visit<T: TS + 'static + ?Sized>(&mut self) {
                let shape = (T::name(&self.0), undocumented(T::inline(&self.0)));
                if !self.1.contains(&shape) {
                    self.1.push(shape);
                    T::visit_dependencies(self);
                }
            }
        }
        let mut shapes = Shapes(Config::default(), Vec::new());
        shapes.visit::<T>();
        shapes.1.sort();
        shapes.1
    }

    /// The window's reading of dictation's answers has the very fields of
    /// the host's, by the declarations both make for TypeScript: a field
    /// renamed, added or taken out there fails here.
    #[test]
    fn dictations_answers_are_read_by_their_own_names() {
        assert_eq!(shape::<DictationStatus>(), shape::<crate::dictation::Status>());
        assert_eq!(shape::<SpeechModel>(), shape::<crate::dictation::ModelState>());
    }

    /// Every answer reaches the macOS window as it left the host.
    #[test]
    fn an_answer_is_read_back_as_it_was_sent() {
        let answers = [
            Answer::Settings(Box::default()),
            Answer::Hotkeys(Hotkeys { supported: true, toggle: Some("<Super>space".into()), ..Hotkeys::default() }),
            Answer::Capabilities(crate::platform::capabilities()),
            Answer::Speech(vec![SpeechModel { id: "x".into(), downloading: Some(SpeechProgress { step: "speech-model".into(), received: 1, total: 2 }), ..SpeechModel::default() }]),
            Answer::Update(None),
            Answer::Done,
        ];
        for answer in answers {
            let sent = serde_json::to_string(&answer).unwrap();
            assert_eq!(serde_json::from_str::<Answer>(&sent).unwrap(), answer, "{sent}");
        }
    }
}
