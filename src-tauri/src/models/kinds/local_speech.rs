//! The speech models Heyo downloads and runs in its own process
//! (`dictation::models`). Always there, once.

use crate::dictation::models::{find, CATALOGUE};
use crate::i18n::{marked, t, tn};
use crate::models::registry::{Can, Provider};

use super::{Checked, ProviderKind, Recognizer, Source};

pub(crate) const WORD: &str = "local-speech";

pub(super) struct LocalSpeech;

impl ProviderKind for LocalSpeech {
    fn word(&self) -> &'static str {
        WORD
    }
    fn title(&self) -> &'static str {
        marked("Speech Models on This Computer")
    }
    fn icon(&self) -> &'static str {
        "audio-waveform"
    }
    fn about(&self) -> &'static str {
        marked("Speech models Heyo downloads and runs itself: your voice never leaves this computer.")
    }
    fn builtin(&self) -> bool {
        true
    }
    fn can(&self, _provider: &Provider, need: Can) -> bool {
        matches!(need, Can::Speech | Can::Streaming | Can::Private)
    }
    fn goes(&self, _provider: &Provider) -> &'static str {
        marked("Runs on this computer: what it is given stays here.")
    }
    fn source(&self) -> Source {
        Source::Catalogue
    }
    /// The default (or the one downloaded, `dictation::models::chosen`) first.
    fn models(&self, _provider: &Provider) -> Vec<String> {
        std::iter::once(String::new()).chain(CATALOGUE.iter().map(|model| model.id.to_owned())).collect()
    }
    fn model_title(&self, _provider: &Provider, model: &str) -> String {
        match model {
            "" => t("Default").into(),
            id => find(id).map_or(id.to_owned(), |model| model.name.to_owned()),
        }
    }
    fn check(&self, _provider: &Provider) -> Result<Checked, String> {
        let dir = crate::dictation::dir();
        match CATALOGUE.iter().filter(|model| model.installed(&dir)).count() {
            0 => Err(t("No speech model is downloaded yet.").into()),
            n => Ok(Checked { said: tn("{0} model is downloaded and ready.", "{0} models are downloaded and ready.", n as u64, &[&n]), models: None }),
        }
    }
    fn recognizer(&self, _provider: &Provider, _model: &str, _vocabulary: &crate::dictation::vocabulary::Vocabulary) -> Option<Result<Recognizer, crate::dictation::engine::Failure>> {
        Some(Ok(Recognizer::ThisComputer))
    }
}
