//! A model that answers: what Heyo's AI features ask of one, whichever
//! answers. Claude (`claude.rs`) and any model behind an OpenAI-compatible
//! server (`openai.rs`) are its two adapters, and [`Model::from_settings`]
//! picks one. Every model Heyo asks is asked here, in this process, which
//! alone holds the key: the extension runtime asks through `host.ai.ask`
//! (`ai.rs`), dictation polishes what it heard (`dictation.rs`), and the
//! clipboard's evaluation has cases written (`clipboard/cases.rs`).
//!
//! An answer is written as it comes, to a callback; it is called off by
//! dropping the future that asks (aborting the task it runs in), which
//! closes the connection.

mod claude;
mod openai;
pub(crate) mod address;

#[cfg(test)]
pub(crate) mod mock;

use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::i18n::{t, tf};
use crate::settings::Settings;
use crate::wire::AiProvider;

pub(crate) use address::{address, Address};
pub(crate) use claude::CLAUDE_MODEL;

/// An earlier question or answer of a conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Turn {
    #[ts(type = "\"user\" | \"assistant\"")]
    pub role: String,
    pub content: String,
}

/// A picture to go with a question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct PromptImage {
    #[ts(type = "\"image/png\" | \"image/jpeg\" | \"image/gif\" | \"image/webp\"")]
    pub media_type: String,
    /// Base64.
    pub data: String,
}

/// What is asked. Without `history` the prompt is text to work on
/// (translate, rewrite, clean up…), and it is given as material, never as
/// a request to the model; with `history` (even an empty one) it is the
/// user's next message in a conversation.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Question {
    pub system: String,
    pub prompt: String,
    #[serde(default)]
    #[ts(optional)]
    pub history: Option<Vec<Turn>>,
    /// Only for a model that sees images (Claude); asking another with
    /// images fails.
    #[serde(default)]
    #[ts(optional, as = "Option<Vec<PromptImage>>")]
    pub images: Vec<PromptImage>,
    /// How freely to answer, 0 to 2. A hint: a model that takes none ignores it.
    #[serde(default)]
    #[ts(optional)]
    pub temperature: Option<f64>,
}

impl Question {
    /// Text to work on, by the instructions in `system`.
    pub fn material(system: impl Into<String>, text: impl Into<String>) -> Question {
        Question { system: system.into(), prompt: text.into(), ..Question::default() }
    }
}

/// The model that answers.
#[derive(Debug, Clone, PartialEq)]
pub enum Model {
    Claude(claude::Claude),
    /// A model behind an OpenAI-compatible server (vLLM, LM Studio,
    /// Ollama, llama.cpp), on this computer or anywhere.
    Local(openai::Local),
}

/// Why a model gave no answer, as the user is told it ([`ModelError::message`]).
#[derive(Debug, Clone, PartialEq)]
pub enum ModelError {
    /// Claude is the provider, and no credentials were found anywhere.
    NotSetUp,
    /// Claude did not take the credentials.
    InvalidKey,
    RateLimited,
    ClaudeUnreachable,
    /// Claude's API answered with an error: its status (or the kind of
    /// error, for one in the middle of an answer), and what it said.
    Claude { status: String, message: String },
    /// Claude declined to answer (`stop_reason: "refusal"`).
    Declined,
    ClaudeSaidNothing,
    /// The local server could not be reached at this origin.
    LocalUnreachable(String),
    /// The local server answered with an error status, and what it said.
    Local { status: u16, detail: String },
    NoImages,
    NoModels,
    LocalSaidNothing,
    /// The server did not answer in the shape of one (a list of models).
    NotAModelList,
}

impl ModelError {
    /// In words for the user, in the language Heyo speaks.
    pub fn message(&self) -> String {
        match self {
            ModelError::NotSetUp => t("AI is not set up. Add an Anthropic API key, or choose a local model, in Heyo Settings → Advanced.").into(),
            ModelError::InvalidKey => t("The Claude API key is invalid. Check it in Heyo Settings → Advanced.").into(),
            ModelError::RateLimited => t("Claude's rate limit was reached. Try again shortly.").into(),
            ModelError::ClaudeUnreachable => t("Could not reach the Claude API.").into(),
            ModelError::Claude { status, message } => tf("Claude API error {0}: {1}", &[status, message]),
            ModelError::Declined => t("Claude declined this request.").into(),
            ModelError::ClaudeSaidNothing => t("Claude returned no text.").into(),
            ModelError::LocalUnreachable(origin) => tf("Could not reach the local model at {0}. Is it running? (Heyo Settings → Advanced)", &[origin]),
            ModelError::Local { status, detail } => tf("The local model answered with an error ({0}): {1}", &[status, detail]),
            ModelError::NoImages => t("The local model is not given images.").into(),
            ModelError::NoModels => t("The local server lists no models.").into(),
            ModelError::LocalSaidNothing => t("The local model returned no text.").into(),
            ModelError::NotAModelList => t("the server did not answer with a model list").into(),
        }
    }
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl Model {
    /// The model the settings name.
    pub fn from_settings(settings: &Settings) -> Model {
        match settings.ai_provider.as_str() {
            "local" => Model::Local(openai::Local::new(&settings.local_model_url, &settings.local_model_name)),
            _ => Model::Claude(claude::Claude::new(&settings.anthropic_api_key)),
        }
    }

    /// The model `model` (empty: the first the server lists) of the
    /// OpenAI-compatible server at `url`, whatever the settings name.
    pub fn local(url: &str, model: &str) -> Model {
        Model::Local(openai::Local::new(url, model))
    }

    /// Whether it can be given images besides text.
    pub fn sees_images(&self) -> bool {
        matches!(self, Model::Claude(_))
    }

    /// The whole answer, trimmed, with nothing of what the model says
    /// before it. `on_text` is given the answer so far each time it grows.
    pub async fn answer(&self, question: &Question, on_text: &mut (dyn FnMut(&str) + Send)) -> Result<String, ModelError> {
        match self {
            Model::Claude(claude) => claude.answer(question, on_text).await,
            Model::Local(local) => local.answer(question, on_text).await,
        }
    }
}

/// The models an OpenAI-compatible server offers.
pub async fn local_models(url: &str) -> Result<Vec<String>, ModelError> {
    openai::list(url, Some(Duration::from_secs(3))).await
}

/// Which model answers `AI.ask`, for an extension deciding what to send.
pub fn provider(settings: &Settings) -> AiProvider {
    match settings.ai_provider.as_str() {
        "local" => AiProvider { kind: "local", model: settings.local_model_name.clone(), local: address(&settings.local_model_url) == Address::ThisComputer },
        _ => AiProvider { kind: "claude", model: CLAUDE_MODEL.into(), local: false },
    }
}

// --- What both adapters share ---------------------------------------------------

/// An HTTP client for one question. Not kept: a connection belongs to the
/// async runtime it was opened on, and questions are asked from more than one.
fn client(direct: bool) -> reqwest::Client {
    static TLS: std::sync::Once = std::sync::Once::new();
    // TLS through rustls, with ring for its cryptography (another crate
    // may have chosen first, which is as good).
    TLS.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    let builder = reqwest::Client::builder().connect_timeout(Duration::from_secs(10));
    // A server on this computer or the local network is not reached through a proxy.
    let builder = if direct { builder.no_proxy() } else { builder };
    builder.build().expect("the HTTP client has nothing to fail on")
}

/// Reads a response as server-sent events: the JSON of each `data:` line,
/// in order, to `each`, until the stream ends or `each` says to stop.
async fn events(mut response: reqwest::Response, mut each: impl FnMut(&str) -> bool) -> Result<(), reqwest::Error> {
    let mut buffer = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        buffer.extend_from_slice(&chunk);
        while let Some(end) = buffer.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buffer.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let Some(data) = line.trim_end().strip_prefix("data:") else { continue };
            if !each(data.trim()) {
                return Ok(());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_name_the_adapter_and_what_it_can_be_given() {
        let mut settings = Settings::default();
        let claude = Model::from_settings(&settings);
        assert!(matches!(claude, Model::Claude(_)) && claude.sees_images());
        settings.ai_provider = "local".into();
        let local = Model::from_settings(&settings);
        assert!(matches!(local, Model::Local(_)) && !local.sees_images());
    }

    #[test]
    fn an_extension_is_told_which_model_answers_and_whether_its_questions_stay_here() {
        let mut settings = Settings::default();
        assert_eq!(provider(&settings), AiProvider { kind: "claude", model: CLAUDE_MODEL.into(), local: false });
        settings.ai_provider = "local".into();
        settings.local_model_name = "qwen3".into();
        assert_eq!(provider(&settings), AiProvider { kind: "local", model: "qwen3".into(), local: true });
        // A server on another machine is a local model only by the setting's name.
        settings.local_model_url = "http://192.168.1.20:8000/v1".into();
        assert!(!provider(&settings).local);
    }

    #[test]
    fn every_failure_is_worded_for_the_user() {
        let cases = [
            (ModelError::NotSetUp, "AI is not set up. Add an Anthropic API key, or choose a local model, in Heyo Settings → Advanced."),
            (ModelError::InvalidKey, "The Claude API key is invalid. Check it in Heyo Settings → Advanced."),
            (ModelError::RateLimited, "Claude's rate limit was reached. Try again shortly."),
            (ModelError::ClaudeUnreachable, "Could not reach the Claude API."),
            (ModelError::Claude { status: "529".into(), message: "Overloaded".into() }, "Claude API error 529: Overloaded"),
            (ModelError::Declined, "Claude declined this request."),
            (ModelError::ClaudeSaidNothing, "Claude returned no text."),
            (ModelError::LocalUnreachable("http://127.0.0.1:9".into()), "Could not reach the local model at http://127.0.0.1:9. Is it running? (Heyo Settings → Advanced)"),
            (ModelError::Local { status: 404, detail: "no such model".into() }, "The local model answered with an error (404): no such model"),
            (ModelError::NoImages, "The local model is not given images."),
            (ModelError::NoModels, "The local server lists no models."),
            (ModelError::LocalSaidNothing, "The local model returned no text."),
        ];
        for (error, words) in cases {
            assert_eq!(error.to_string(), words);
        }
    }
}
