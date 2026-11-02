//! A model that answers: what Heyo's AI features ask of one, whichever
//! answers. Every text model is a [`TextAdapter`] (`text.rs`): Claude
//! (`claude.rs`), any model behind an OpenAI-compatible server
//! (`openai.rs`), and the models the command-line tools Claude Code
//! (`claude_code.rs`) and Codex (`codex.rs`) reach with the user's own
//! sign-in (both run through `cli.rs`). A provider's kind opens one
//! (`kinds`), and [`Model::for_use`] the one the settings name for a use
//! (`registry`). Every model Heyo asks is asked here, in this process,
//! which alone holds the key: the extension runtime asks through
//! `host.ai.ask` (`ai.rs`), dictation polishes what it heard
//! (`dictation.rs`), and the clipboard's evaluation has cases written
//! (`clipboard/cases.rs`).
//!
//! An answer is written as it comes, to a callback; it is called off by
//! dropping the future that asks (aborting the task it runs in), which
//! closes the connection, or ends the tool that was answering.

pub(crate) mod address;
mod claude;
pub(crate) mod claude_code;
pub(crate) mod cli;
pub(crate) mod codex;
pub(crate) mod kinds;
mod openai;
pub(crate) mod registry;
pub(crate) mod text;

#[cfg(test)]
mod contract;
#[cfg(test)]
pub(crate) mod fake;
#[cfg(test)]
pub(crate) mod mock;

use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::i18n::{t, tf};
use crate::settings::Settings;
use crate::wire::AiProvider;
use registry::{Unusable, Use};

pub(crate) use address::{address, Address};
pub(crate) use claude::CLAUDE_MODEL;
pub(crate) use text::{Conversation, TextAdapter};

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

/// What is asked, once. Without `history` the prompt is text to work on
/// (translate, rewrite, clean up…), and it is given as material, never as
/// a request to the model; with `history` (even an empty one) it is the
/// user's next message in a conversation. It is asked as one step of a
/// conversation.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Question {
    pub system: String,
    pub prompt: String,
    #[serde(default)]
    #[ts(optional)]
    pub history: Option<Vec<Turn>>,
    /// Only for a model that sees images (Claude, also through Claude Code,
    /// and Codex); asking another with images fails.
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

/// The model that answers, as the settings name it for a use: its
/// adapter, with what a one-shot question needs of it.
#[derive(Debug)]
pub struct Model(Box<dyn TextAdapter>);

/// Why a model gave no answer: what kind of failure, in the words its
/// adapter says it in ([`ModelError::message`]), for the user.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelError {
    /// It cannot be asked as it is set up: no credentials, no program, no model.
    NotSetUp(String),
    /// Its service did not take the credentials, or the tool is signed out.
    Auth(String),
    RateLimited(String),
    /// Its service or server could not be reached.
    Unreachable(String),
    /// The model declined to answer.
    Refused(String),
    /// Anything else, with what was said of it.
    Failed(String),
    /// The use has no model, or one that cannot be used for it (`registry::resolve`).
    Unusable(Unusable),
}

impl ModelError {
    /// In words for the user, in the language Heyo speaks.
    pub fn message(&self) -> String {
        match self {
            ModelError::NotSetUp(said) | ModelError::Auth(said) | ModelError::RateLimited(said) | ModelError::Unreachable(said) | ModelError::Refused(said) | ModelError::Failed(said) => said.clone(),
            ModelError::Unusable(why) => unusable(why),
        }
    }
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl Model {
    /// The model the settings name for `need`, as its provider's kind opens it.
    pub fn for_use(settings: &Settings, need: Use) -> Result<Model, ModelError> {
        let (provider, model) = registry::resolve(&settings.providers, need.reference(settings), need).map_err(ModelError::Unusable)?;
        provider.kind().text(provider, model).map(Model).ok_or_else(|| ModelError::Unusable(Unusable::Cannot(provider.name.clone())))
    }

    /// The model `model` (empty: the first the server lists) of the
    /// OpenAI-compatible server at `url`, whatever the settings name.
    #[cfg(test)]
    pub fn local(url: &str, model: &str) -> Model {
        Model::of(openai::Local::new(url, "", model))
    }

    /// The model `adapter` is, whatever the settings name.
    #[cfg(test)]
    pub fn of(adapter: impl TextAdapter + 'static) -> Model {
        Model(Box::new(adapter))
    }

    /// The whole answer to a one-shot question, trimmed, with nothing of
    /// what the model says before it. `on_text` is given the answer so far
    /// each time it grows.
    pub async fn answer(&self, question: &Question, on_text: &mut (dyn FnMut(&str) + Send)) -> Result<String, ModelError> {
        Ok(self.0.step(&Conversation::of(question), on_text).await?.text)
    }
}

/// The adapter's own: what it can do, and a step of a conversation.
impl std::ops::Deref for Model {
    type Target = dyn TextAdapter;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// What a use's model not being usable is, in words for the user.
fn unusable(why: &Unusable) -> String {
    match why {
        Unusable::NoneChosen => t("No model is chosen for this. Choose one in Heyo Settings.").into(),
        Unusable::Missing(id) => tf("The provider “{0}” this uses is no longer connected. Choose another model in Heyo Settings.", &[id]),
        Unusable::Cannot(name) => tf("{0} cannot be used for this. Choose another model in Heyo Settings.", &[name]),
        Unusable::NotSetUp(name, missing) => tf("{0} is not set up: {1} (Heyo Settings → Models)", &[name, &crate::i18n::translate(missing)]),
    }
}

/// Which model answers `AI.ask`, for an extension deciding what to send.
pub fn provider(settings: &Settings) -> AiProvider {
    let Ok((provider, model)) = registry::resolve(&settings.providers, &settings.ai_model, Use::Answers) else {
        return AiProvider { kind: "none", model: String::new(), local: false };
    };
    let kind = provider.kind().api_name();
    let model = provider.kind().text(provider, model).map(|adapter| adapter.model()).unwrap_or_default();
    AiProvider { kind, model, local: provider.can(registry::Can::Private) }
}

// --- What the adapters over HTTP share --------------------------------------------

/// An HTTP client for one question. Not kept: a connection belongs to the
/// async runtime it was opened on, and questions are asked from more than one.
pub(crate) fn client(direct: bool) -> reqwest::Client {
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

    use registry::{Can, ModelRef, Provider};

    fn with(providers: Vec<Provider>, ai: ModelRef) -> Settings {
        let mut all = registry::defaults();
        all.extend(providers);
        Settings { providers: all, ai_model: ai, ..Settings::default() }
    }

    fn tool(id: &str, kind: &str, path: &str) -> Provider {
        let mut provider = Provider::new(id, kind);
        provider.config.insert(registry::PATH.into(), path.into());
        provider
    }

    fn server(id: &str, url: &str) -> Provider {
        let mut provider = Provider::new(id, kinds::OPENAI_COMPATIBLE);
        provider.config.insert(registry::URL.into(), url.into());
        provider
    }

    /// What the adapter a model is, as it prints.
    fn shown(model: &Model) -> String {
        format!("{:?}", model.0)
    }

    #[test]
    fn the_reference_names_the_adapter_and_what_it_can_be_given() {
        let claude = Model::for_use(&Settings::default(), Use::Answers).unwrap();
        assert!(shown(&claude).starts_with("Claude {") && claude.can(Can::Images) && claude.can(Can::ToolCalls));
        let settings = with(vec![server("ollama", "http://127.0.0.1:11434/v1")], ModelRef::to("ollama", "qwen3"));
        let local = Model::for_use(&settings, Use::Answers).unwrap();
        assert_eq!(shown(&local), shown(&Model::local("http://127.0.0.1:11434/v1", "qwen3")));
        assert!(!local.can(Can::Images) && local.can(Can::ToolCalls));
        let settings = with(vec![tool("cc", kinds::CLAUDE_CODE, "/opt/claude")], ModelRef::to("cc", " opus "));
        let claude_code = Model::for_use(&settings, Use::Answers).unwrap();
        assert_eq!(shown(&claude_code), r#"ClaudeCode(Cli { program: "Claude Code", path: "/opt/claude", model: "opus" })"#);
        assert!(claude_code.can(Can::Images) && !claude_code.can(Can::ToolCalls));
        let settings = with(vec![tool("codex", kinds::CODEX, "")], ModelRef::to("codex", ""));
        assert_eq!(shown(&Model::for_use(&settings, Use::Answers).unwrap()), r#"Codex(Cli { program: "Codex", path: "", model: "" })"#);
        // Each use goes by its own reference.
        let settings = Settings { polish_model: ModelRef::to("codex", ""), ai_model: ModelRef::to(registry::CLAUDE, ""), ..settings };
        assert!(shown(&Model::for_use(&settings, Use::DictationPolish).unwrap()).starts_with("Codex("));
        assert!(shown(&Model::for_use(&settings, Use::Answers).unwrap()).starts_with("Claude {"));
        // A reference to what is gone, or to what cannot answer in text, is refused in words.
        let gone = with(vec![], ModelRef::to("ollama", ""));
        assert_eq!(Model::for_use(&gone, Use::Answers).unwrap_err().to_string(), "The provider “ollama” this uses is no longer connected. Choose another model in Heyo Settings.");
        let speech = with(vec![], ModelRef::to(registry::LOCAL_SPEECH, crate::dictation::models::DEFAULT));
        assert!(matches!(Model::for_use(&speech, Use::Answers), Err(ModelError::Unusable(Unusable::Cannot(_)))));
        assert_eq!(Model::for_use(&with(vec![], ModelRef::none()), Use::Answers).unwrap_err().to_string(), "No model is chosen for this. Choose one in Heyo Settings.");
    }

    #[test]
    fn an_extension_is_told_which_model_answers_and_whether_its_questions_stay_here() {
        assert_eq!(provider(&Settings::default()), AiProvider { kind: "claude", model: CLAUDE_MODEL.into(), local: false });
        // Claude's API has the one model, whatever the reference says.
        let claude = Settings { ai_model: ModelRef::to(registry::CLAUDE, ""), ..Settings::default() };
        assert_eq!(provider(&claude).model, CLAUDE_MODEL);
        let settings = with(vec![server("ollama", "http://127.0.0.1:11434/v1"), server("lan", "http://192.168.1.20:8000/v1")], ModelRef::to("ollama", "qwen3"));
        assert_eq!(provider(&settings), AiProvider { kind: "local", model: "qwen3".into(), local: true });
        // A server on another machine is not this computer, whatever it runs.
        assert!(!provider(&Settings { ai_model: ModelRef::to("lan", ""), ..settings.clone() }).local);
        // A tool on this computer sends what it is asked to its maker's service.
        let settings = with(vec![tool("cc", kinds::CLAUDE_CODE, ""), tool("codex", kinds::CODEX, "")], ModelRef::to("cc", "sonnet"));
        assert_eq!(provider(&settings), AiProvider { kind: "claude-code", model: "sonnet".into(), local: false });
        assert_eq!(provider(&Settings { ai_model: ModelRef::to("codex", ""), ..settings.clone() }), AiProvider { kind: "codex", model: "".into(), local: false });
        assert_eq!(provider(&Settings { ai_model: ModelRef::none(), ..settings }).kind, "none");
    }

    #[test]
    fn a_failure_is_its_adapters_words_or_why_the_use_has_no_model() {
        assert_eq!(ModelError::RateLimited("Slow down.".into()).to_string(), "Slow down.");
        assert_eq!(ModelError::Unusable(Unusable::Cannot("Doubao".into())).to_string(), "Doubao cannot be used for this. Choose another model in Heyo Settings.");
    }
}
