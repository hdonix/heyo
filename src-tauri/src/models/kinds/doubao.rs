//! Volcengine's streaming speech recognition, Doubao
//! (`dictation/engine/doubao.rs`).

use crate::dictation::doubao::{self, Config, ACCESS_TOKEN, API_KEY, APP_ID, CONSOLE, CONSOLES, REMOVE_FILLERS, RESOURCES};
use crate::i18n::{marked, t, translate};
use crate::models::registry::{field, when, Can, Entry, Field, Provider};

use super::{Checked, Known, Missing, Opener, ProviderKind, Status};

pub(crate) const WORD: &str = "doubao";

const FIELDS: &[Field] = &[
    field(CONSOLE, marked("Console"), Entry::Choice(CONSOLES), "", marked("Which of Volcengine's consoles your key is from.")),
    when(field(API_KEY, marked("API Key"), Entry::Secret, "", marked("From the new console.")), CONSOLE, "new"),
    when(field(APP_ID, marked("App ID"), Entry::Text, "", marked("From the old console, with its Access Token.")), CONSOLE, "old"),
    when(field(ACCESS_TOKEN, marked("Access Token"), Entry::Secret, "", marked("From the old console.")), CONSOLE, "old"),
    field(REMOVE_FILLERS, marked("Remove Filler Words"), Entry::Switch, "", marked("Doubao takes out filler words, stutters and repeated words.")),
];

pub(super) struct Doubao;

impl ProviderKind for Doubao {
    fn word(&self) -> &'static str {
        WORD
    }
    fn title(&self) -> &'static str {
        marked("Doubao (Volcengine)")
    }
    fn icon(&self) -> &'static str {
        "cloud"
    }
    fn about(&self) -> &'static str {
        marked("Volcengine's streaming speech recognition, with a key from its console.")
    }
    fn fields(&self) -> &'static [Field] {
        FIELDS
    }
    fn can(&self, _provider: &Provider, need: Can) -> bool {
        matches!(need, Can::Speech | Can::Streaming)
    }
    /// Its resources, the first being the default.
    fn models(&self, _provider: &Provider) -> Vec<String> {
        RESOURCES.iter().map(|(id, _)| id.to_string()).collect()
    }
    fn model_title(&self, _provider: &Provider, model: &str) -> String {
        RESOURCES.iter().find(|(id, _)| *id == model).map_or(model.to_owned(), |(_, title)| translate(title).into())
    }
    fn unset(&self, provider: &Provider) -> Option<Missing> {
        (!Config::keyed(provider)).then_some(Missing { short: marked("Key missing"), said: marked("The key is missing.") })
    }
    fn status(&self, provider: &Provider, known: &Known) -> Status {
        match self.unset(provider) {
            Some(_) => super::status(self.unset(provider), known),
            None => Status::fine(t("Set up"), t("Your audio is sent to Volcengine (ByteDance) while you dictate, and recognized there.")),
        }
    }
    fn check(&self, provider: &Provider) -> Result<Checked, String> {
        let config = Config::of(provider, "", &Default::default()).map_err(|failure| super::worded(&failure))?;
        doubao::check(&config).map_err(|failure| super::worded(&failure))?;
        Ok(Checked { said: t("Volcengine takes the key.").into(), models: None })
    }
    fn recognizer(&self, provider: &Provider, model: &str, vocabulary: &crate::dictation::vocabulary::Vocabulary) -> Option<Result<Box<dyn Opener>, crate::dictation::engine::Failure>> {
        Some(Config::of(provider, model, vocabulary).map(|config| Box::new(config) as Box<dyn Opener>))
    }
}
