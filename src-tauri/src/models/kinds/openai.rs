//! Any server that speaks OpenAI's chat API: vLLM, LM Studio, Ollama,
//! llama.cpp, on this computer or elsewhere (`models/openai.rs`).

use std::time::Duration;

use crate::i18n::{marked, t, tf, tn};
use crate::models::registry::{field, Can, Entry, Field, Provider, KEY, MODELS, URL};
use crate::models::{address, openai, Address, TextAdapter};

use super::{text_can, Checked, Known, Missing, ProviderKind, Source, Status};

pub(crate) const WORD: &str = "openai-compatible";

const FIELDS: &[Field] = &[
    field(URL, marked("Server"), Entry::Url, "http://127.0.0.1:8000/v1", marked("Any OpenAI-compatible server works (vLLM, LM Studio, Ollama).")),
    field(KEY, marked("API Key"), Entry::Secret, "", marked("Only for a server that asks for one.")),
    field(MODELS, marked("More Models"), Entry::List, "qwen3, llama3.3", marked("Models to offer besides the ones the server lists, separated by commas.")),
];

pub(super) struct OpenaiCompatible;

impl ProviderKind for OpenaiCompatible {
    fn word(&self) -> &'static str {
        WORD
    }
    fn title(&self) -> &'static str {
        marked("OpenAI-Compatible Server")
    }
    fn icon(&self) -> &'static str {
        "server"
    }
    fn about(&self) -> &'static str {
        marked("A model server such as Ollama, vLLM or LM Studio, on this computer or elsewhere.")
    }
    fn fields(&self) -> &'static [Field] {
        FIELDS
    }
    fn can(&self, provider: &Provider, need: Can) -> bool {
        // Only as private as where the server is.
        text_can(self, provider, need) || (need == Can::Private && address(provider.get(URL)) == Address::ThisComputer)
    }
    fn source(&self) -> Source {
        Source::Listed
    }
    fn models(&self, _provider: &Provider) -> Vec<String> {
        vec![String::new()]
    }
    fn model_title(&self, _provider: &Provider, model: &str) -> String {
        if model.is_empty() { t("First Listed").into() } else { model.into() }
    }
    fn unset(&self, provider: &Provider) -> Option<Missing> {
        provider.get(URL).is_empty().then_some(Missing { short: marked("Address missing"), said: marked("The server's address is missing.") })
    }
    /// Whether it answers, and where what is sent to it goes.
    fn status(&self, provider: &Provider, known: &Known) -> Status {
        let mut status = super::status(self.unset(provider), known);
        if self.unset(provider).is_some() {
            return status;
        }
        if known.listed.is_none() {
            status = Status::fine(t("Checking…"), t("Checking…"));
        }
        let (whereabouts, warning) = match address(provider.get(URL)) {
            Address::ThisComputer => (t("Text stays on this computer.").to_string(), false),
            Address::Unknown => (t("This is not an http or https address.").to_string(), true),
            Address::Elsewhere { host, encrypted: true } => (tf("This address is not this computer: the text you select, ask about and dictate is sent to {0}.", &[&host]), true),
            Address::Elsewhere { host, encrypted: false } => (tf("This address is not this computer, and the connection is not encrypted: the text you select, ask about and dictate is sent to {0} over plain http, readable by anyone on the network in between. Use https, or a server on this computer.", &[&host]), true),
        };
        Status { long: format!("{} {whereabouts}", status.long), warning: status.warning || warning, ..status }
    }
    fn list(&self, provider: &Provider) -> Result<Vec<String>, String> {
        tauri::async_runtime::block_on(openai::list(provider.get(URL), provider.get(KEY), Some(Duration::from_secs(3)))).map_err(|e| e.message())
    }
    fn check(&self, provider: &Provider) -> Result<Checked, String> {
        let models = self.list(provider)?;
        Ok(Checked { said: tn("The server answers and lists {0} model.", "The server answers and lists {0} models.", models.len() as u64, &[&models.len()]), models: Some(models) })
    }
    fn text(&self, provider: &Provider, model: &str) -> Option<Box<dyn TextAdapter>> {
        Some(Box::new(openai::Local::new(provider.get(URL), provider.get(KEY), model)))
    }
    fn api_name(&self) -> &'static str {
        "local"
    }
}
