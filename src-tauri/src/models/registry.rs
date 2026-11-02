//! The providers the user has connected, and which of their models each
//! feature uses.
//!
//! A provider is one place models are reached through: Claude's API, an
//! OpenAI-compatible server, Claude Code or Codex on this computer, the
//! speech models Heyo downloads and runs itself, or Doubao's speech
//! service. Its kind (`kinds`) declares what it needs to be set up and
//! what it can do (`Can`); there may be several of one kind (two
//! servers, say), each by an id of its own. The speech models on this
//! computer are always there, once.
//!
//! A feature does not carry a provider's settings: it refers to one of
//! them and a model of it ([`ModelRef`], one setting for each [`Use`]), and
//! what that reference reaches is decided here alone ([`resolve`]), so
//! that every picker offers what can be used and every use refuses what
//! cannot.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::secrets::{Owner, Secrets};
use crate::settings::Settings;

use super::kinds::{self, ProviderKind};

/// What a provider can do, which is what a use asks of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Can {
    /// Answer in text.
    Text,
    /// Be given pictures with the text.
    Images,
    /// Turn speech into text.
    Speech,
    /// Say what it has so far while it answers.
    Streaming,
    /// Answer with calls of the tools it is offered, and go on from their
    /// results (`models::text`). Nothing asks for it yet: what will is an
    /// agent's loop.
    #[cfg_attr(not(test), allow(dead_code))]
    ToolCalls,
    /// Keep what it is given on this computer: it runs here and sends
    /// nothing elsewhere. Claude Code and Codex run here but send what
    /// they are asked to their makers' services, so they do not.
    Private,
}

/// How a field of a provider's configuration is entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    Text,
    /// An http(s) address.
    Url,
    /// Never shown again once saved, and never sent to a window or logged.
    Secret,
    /// An executable.
    Path,
    /// Names, comma-separated.
    List,
    /// One of these: value, title.
    Choice(&'static [(&'static str, &'static str)]),
    /// On (`true`) or off.
    Switch,
}

/// A field of a provider's configuration: its name in `config`, title,
/// how it is entered, what the empty field shows, and what it is.
/// `shown_when`: it only applies while another field has that value (an
/// empty one being the first of a choice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    pub title: &'static str,
    pub entry: Entry,
    pub placeholder: &'static str,
    pub description: &'static str,
    pub shown_when: Option<(&'static str, &'static str)>,
}

pub(crate) const fn field(name: &'static str, title: &'static str, entry: Entry, placeholder: &'static str, description: &'static str) -> Field {
    Field { name, title, entry, placeholder, description, shown_when: None }
}

pub(crate) const fn when(field: Field, other: &'static str, value: &'static str) -> Field {
    Field { shown_when: Some((other, value)), ..field }
}

/// The address field of a server, and the key some servers want.
pub const URL: &str = "url";
pub const KEY: &str = "key";
pub const PATH: &str = "path";
pub const MODELS: &str = "models";

/// A connected provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Provider {
    /// Its own, never reused while it is there: what references name.
    pub id: String,
    /// Its kind, by the word it is stored as (`ProviderKind::word`).
    pub kind: String,
    /// As the user calls it.
    pub name: String,
    /// The values of its kind's fields, by name; a missing one is empty.
    #[serde(default)]
    pub config: BTreeMap<String, String>,
}

/// The id of the speech models on this computer, which are always there.
pub const LOCAL_SPEECH: &str = "local-speech";
/// The one the settings begin with for text.
pub const CLAUDE: &str = "claude";

impl Provider {
    /// A new one of the kind `kind` names, named as its kind is.
    pub fn new(id: &str, kind: &str) -> Provider {
        let mut provider = Provider { id: id.into(), kind: kind.into(), name: String::new(), config: BTreeMap::new() };
        provider.name = crate::i18n::translate(provider.kind().title()).into();
        provider
    }

    /// What it is.
    pub fn kind(&self) -> &'static dyn ProviderKind {
        kinds::find(&self.kind).unwrap_or(&kinds::Unknown)
    }

    /// Whether `field` applies, as the other fields are set.
    pub fn applies(&self, field: &Field) -> bool {
        let Some((other, value)) = field.shown_when else { return true };
        let first = self.kind().fields().iter().find(|known| known.name == other).and_then(|known| match known.entry {
            Entry::Choice(choices) => choices.first().map(|(first, _)| *first),
            _ => None,
        });
        let set = match self.get(other) {
            "" => first.unwrap_or(""),
            set => set,
        };
        set == value
    }

    /// A field's value, trimmed; empty if it has none.
    pub fn get(&self, name: &str) -> &str {
        self.config.get(name).map_or("", |value| value.trim())
    }

    fn listed(&self, name: &str) -> Vec<String> {
        self.get(name).split([',', '\n']).map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).collect()
    }

    /// What it can do.
    pub fn can(&self, need: Can) -> bool {
        self.kind().can(self, need)
    }

    /// The models it offers that are known without asking it, the default
    /// (an empty name) first where it has one, and those its `models`
    /// field adds. A service that lists its models (`Source::Listed`) also
    /// offers what it lists, which the settings window asks for.
    pub fn models(&self) -> Vec<String> {
        let mut models = self.kind().models(self);
        if self.kind().fields().iter().any(|field| field.name == MODELS) {
            for more in self.listed(MODELS) {
                if !models.contains(&more) {
                    models.push(more);
                }
            }
        }
        models
    }

    /// What one of its models is called in a picker.
    pub fn model_title(&self, model: &str) -> String {
        self.kind().model_title(self, model)
    }

    /// What is missing for it to be used at all, in words (`marked`); none
    /// if nothing is.
    pub fn unset(&self) -> Option<&'static str> {
        self.kind().unset(self).map(|missing| missing.said)
    }
}

/// The providers the settings begin with: the speech models of this
/// computer, and Claude's API (which needs no key where Anthropic's own
/// tools are signed in).
pub fn defaults() -> Vec<Provider> {
    vec![Provider::new(LOCAL_SPEECH, kinds::LOCAL_SPEECH), Provider::new(CLAUDE, kinds::ANTHROPIC)]
}

/// Puts the speech models of this computer among `providers` if they are
/// not, and keeps them to one: they cannot be removed.
pub fn with_fixed(mut providers: Vec<Provider>) -> Vec<Provider> {
    let mut seen = false;
    providers.retain(|provider| {
        let fixed = provider.kind().builtin();
        let keep = !fixed || (!seen && provider.id == LOCAL_SPEECH);
        seen |= fixed && keep;
        keep
    });
    if !seen {
        providers.insert(0, Provider::new(LOCAL_SPEECH, kinds::LOCAL_SPEECH));
    }
    providers
}

/// An id for a new provider of `kind`: its kind's word, numbered after the
/// first.
pub fn new_id(providers: &[Provider], word: &str) -> String {
    let taken = |id: &str| providers.iter().any(|provider| provider.id == id);
    if !taken(word) {
        return word.into();
    }
    (2..).map(|n| format!("{word}-{n}")).find(|id| !taken(id)).expect("there is always another number")
}

/// What `field` of the provider `id` names is set to `value`: one of its
/// kind's fields, or its `name`. Values are kept trimmed; an empty secret
/// is a secret removed. A secret is kept among `secrets`, never in the
/// provider's configuration.
pub fn set_field(providers: &mut [Provider], secrets: &mut Secrets, id: &str, field: &str, value: &str) -> Result<(), String> {
    use crate::i18n::tf;
    let provider = providers.iter_mut().find(|provider| provider.id == id).ok_or_else(|| tf("There is no provider {0}.", &[&id]))?;
    let value = value.trim();
    if field == "name" {
        if value.is_empty() {
            return Err(crate::i18n::t("A provider needs a name.").into());
        }
        provider.name = value.into();
        return Ok(());
    }
    let Some(known) = provider.kind().fields().iter().find(|known| known.name == field) else {
        return Err(tf("{0} has no setting {1}.", &[&provider.name, &field]));
    };
    if known.entry == Entry::Secret {
        secrets.store(Owner::Provider(id), field, value);
        return Ok(());
    }
    if value.is_empty() {
        provider.config.remove(field);
    } else {
        provider.config.insert(field.into(), value.into());
    }
    Ok(())
}

/// Takes the provider `id` away, if it is one that can be, and its secrets
/// with it.
pub fn remove(providers: &mut Vec<Provider>, secrets: &mut Secrets, id: &str) -> Result<(), String> {
    use crate::i18n::tf;
    let at = providers.iter().position(|provider| provider.id == id).ok_or_else(|| tf("There is no provider {0}.", &[&id]))?;
    if providers[at].kind().builtin() {
        return Err(tf("{0} cannot be removed.", &[&providers[at].name]));
    }
    providers.remove(at);
    secrets.forget(Owner::Provider(id));
    Ok(())
}

/// A provider and one of its models; no provider means none is used.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct ModelRef {
    pub provider: String,
    /// Empty: the provider's default.
    pub model: String,
}

impl ModelRef {
    pub fn to(provider: &str, model: &str) -> ModelRef {
        ModelRef { provider: provider.into(), model: model.into() }
    }

    pub fn none() -> ModelRef {
        ModelRef::default()
    }

    pub fn is_none(&self) -> bool {
        self.provider.is_empty()
    }
}

/// Each use of a model, and the setting that says which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Use {
    /// Ask AI, AI Chat, the selected-text actions, and extensions' `AI.ask`.
    Answers,
    DictationRecognition,
    DictationPolish,
    /// Writing search cases for the clipboard's evaluation: given what was
    /// copied, so only a model that keeps it here.
    ClipboardCases,
}

impl Use {
    /// What it needs of a provider.
    pub fn needs(self) -> &'static [Can] {
        match self {
            Use::Answers | Use::DictationPolish => &[Can::Text],
            // Words go into the window as they are spoken.
            Use::DictationRecognition => &[Can::Speech, Can::Streaming],
            Use::ClipboardCases => &[Can::Text, Can::Private],
        }
    }

    pub fn reference(self, settings: &Settings) -> &ModelRef {
        match self {
            Use::Answers => &settings.ai_model,
            Use::DictationRecognition => &settings.dictation_model,
            Use::DictationPolish => &settings.polish_model,
            Use::ClipboardCases => &settings.cases_model,
        }
    }

    /// The same, to change.
    pub fn reference_mut(self, settings: &mut Settings) -> &mut ModelRef {
        match self {
            Use::Answers => &mut settings.ai_model,
            Use::DictationRecognition => &mut settings.dictation_model,
            Use::DictationPolish => &mut settings.polish_model,
            Use::ClipboardCases => &mut settings.cases_model,
        }
    }

    pub fn suits(self, provider: &Provider) -> bool {
        self.needs().iter().all(|need| provider.can(*need))
    }
}

/// Why a reference reaches nothing that can be used.
#[derive(Debug, Clone, PartialEq)]
pub enum Unusable {
    /// None is chosen.
    NoneChosen,
    /// The provider it names is no longer there.
    Missing(String),
    /// The provider (by name) cannot do what the use needs.
    Cannot(String),
    /// The provider (by name) is not set up: what is missing.
    NotSetUp(String, &'static str),
}

/// The provider and model `reference` names, if it can be used for `need`.
pub fn resolve<'a>(providers: &'a [Provider], reference: &'a ModelRef, need: Use) -> Result<(&'a Provider, &'a str), Unusable> {
    if reference.is_none() {
        return Err(Unusable::NoneChosen);
    }
    let provider = providers.iter().find(|provider| provider.id == reference.provider).ok_or_else(|| Unusable::Missing(reference.provider.clone()))?;
    if !need.suits(provider) {
        return Err(Unusable::Cannot(provider.name.clone()));
    }
    if let Some(missing) = provider.unset() {
        return Err(Unusable::NotSetUp(provider.name.clone(), missing));
    }
    Ok((provider, reference.model.trim()))
}

/// What a picker offers for `need`: each model of each provider that can
/// do it, with what is called "provider · model". `listed` has the models
/// each server listed, by provider id.
pub fn offered(providers: &[Provider], need: Use, listed: &BTreeMap<String, Vec<String>>) -> Vec<(ModelRef, String)> {
    let mut offered = Vec::new();
    for provider in providers.iter().filter(|provider| need.suits(provider)) {
        let mut models = provider.models();
        for name in listed.get(&provider.id).into_iter().flatten() {
            if !models.contains(name) {
                models.push(name.clone());
            }
        }
        // The speech models of this computer are said short: there is one such provider.
        let name = if provider.kind().builtin() { crate::i18n::t("This Computer") } else { provider.name.as_str() };
        offered.extend(models.into_iter().map(|model| (ModelRef::to(&provider.id, &model), format!("{name} · {}", provider.model_title(&model)))));
    }
    offered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CLAUDE_MODEL;

    fn server(id: &str, url: &str) -> Provider {
        Provider { config: BTreeMap::from([(URL.into(), url.into())]), ..Provider::new(id, kinds::OPENAI_COMPATIBLE) }
    }

    #[test]
    fn each_kind_declares_what_it_can_do() {
        let claude = Provider::new("claude", kinds::ANTHROPIC);
        assert!(claude.can(Can::Text) && claude.can(Can::Images) && claude.can(Can::ToolCalls) && !claude.can(Can::Speech) && !claude.can(Can::Private));
        // A tool on this computer sends what it is asked to its maker's
        // service, and its own tools stay off.
        for kind in [kinds::CLAUDE_CODE, kinds::CODEX] {
            let tool = Provider::new("x", kind);
            assert!(tool.can(Can::Text) && tool.can(Can::Images) && !tool.can(Can::Private) && !tool.can(Can::ToolCalls));
        }
        assert!(server("d", "http://127.0.0.1:8000/v1").can(Can::ToolCalls));
        // A server is as private as where it is.
        assert!(server("a", "http://127.0.0.1:11434/v1").can(Can::Private));
        assert!(!server("b", "http://192.168.1.20:8000/v1").can(Can::Private));
        assert!(!server("c", "http://127.0.0.1:8000/v1").can(Can::Images));
        let speech = Provider::new(LOCAL_SPEECH, kinds::LOCAL_SPEECH);
        assert!(speech.can(Can::Speech) && speech.can(Can::Private) && !speech.can(Can::Text));
        let doubao = Provider::new("doubao", kinds::DOUBAO);
        assert!(doubao.can(Can::Speech) && !doubao.can(Can::Private));
    }

    #[test]
    fn the_speech_models_of_this_computer_are_always_there_once() {
        assert_eq!(with_fixed(Vec::new()).iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), [LOCAL_SPEECH]);
        let twice = vec![Provider::new("claude", kinds::ANTHROPIC), Provider::new(LOCAL_SPEECH, kinds::LOCAL_SPEECH), Provider::new("other", kinds::LOCAL_SPEECH)];
        assert_eq!(with_fixed(twice).iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["claude", LOCAL_SPEECH]);
        assert!(kinds::find(kinds::LOCAL_SPEECH).unwrap().builtin() && kinds::addable().all(|kind| !kind.builtin()));
    }

    #[test]
    fn a_provider_is_set_up_field_by_field_and_removed_unless_it_is_fixed() {
        let (mut providers, mut secrets) = (defaults(), Secrets::default());
        set_field(&mut providers, &mut secrets, CLAUDE, KEY, "  sk-ant-1 ").unwrap();
        set_field(&mut providers, &mut secrets, CLAUDE, "name", " Work ").unwrap();
        // The key is kept with the secrets, not in the configuration.
        assert_eq!((providers[1].get(KEY), secrets.read(Owner::Provider(CLAUDE), KEY), providers[1].name.as_str()), ("", Some("sk-ant-1"), "Work"));
        // An empty secret is one removed; a field the kind has not, or no name, is refused.
        set_field(&mut providers, &mut secrets, CLAUDE, KEY, "").unwrap();
        assert!(secrets.is_empty());
        assert!(set_field(&mut providers, &mut secrets, CLAUDE, URL, "http://x").is_err());
        assert!(set_field(&mut providers, &mut secrets, CLAUDE, "name", " ").is_err());
        assert!(set_field(&mut providers, &mut secrets, "gone", KEY, "x").is_err());
        assert!(remove(&mut providers, &mut secrets, LOCAL_SPEECH).is_err());
        // Removed, it takes its secrets with it.
        set_field(&mut providers, &mut secrets, CLAUDE, KEY, "sk-ant-2").unwrap();
        remove(&mut providers, &mut secrets, CLAUDE).unwrap();
        assert_eq!((providers.len(), secrets.is_empty()), (1, true));
        assert!(remove(&mut providers, &mut secrets, CLAUDE).is_err());
    }

    #[test]
    fn several_of_one_kind_each_have_an_id() {
        let mut providers = defaults();
        let first = new_id(&providers, kinds::OPENAI_COMPATIBLE);
        providers.push(server(&first, ""));
        let second = new_id(&providers, kinds::OPENAI_COMPATIBLE);
        assert_eq!((first.as_str(), second.as_str()), ("openai-compatible", "openai-compatible-2"));
        assert_eq!(new_id(&providers, kinds::ANTHROPIC), "anthropic");
    }

    #[test]
    fn a_reference_reaches_only_what_can_be_used_for_it() {
        let providers = vec![Provider::new(LOCAL_SPEECH, kinds::LOCAL_SPEECH), Provider::new("claude", kinds::ANTHROPIC), server("ollama", "http://localhost:11434/v1"), server("far", "https://api.example.com/v1"), server("blank", "")];
        let to = ModelRef::to;
        assert_eq!(resolve(&providers, &to("ollama", " qwen3 "), Use::Answers).map(|(p, m)| (p.id.as_str(), m)), Ok(("ollama", "qwen3")));
        assert_eq!(resolve(&providers, &ModelRef::none(), Use::Answers), Err(Unusable::NoneChosen));
        assert_eq!(resolve(&providers, &to("gone", ""), Use::Answers), Err(Unusable::Missing("gone".into())));
        // Speech is not text, nor text speech.
        assert!(matches!(resolve(&providers, &to(LOCAL_SPEECH, crate::dictation::models::DEFAULT), Use::Answers), Err(Unusable::Cannot(_))));
        assert!(matches!(resolve(&providers, &to("claude", CLAUDE_MODEL), Use::DictationRecognition), Err(Unusable::Cannot(_))));
        assert!(resolve(&providers, &to(LOCAL_SPEECH, crate::dictation::models::DEFAULT), Use::DictationRecognition).is_ok());
        // What was copied goes only to a model that keeps it here.
        assert!(resolve(&providers, &to("ollama", ""), Use::ClipboardCases).is_ok());
        assert!(matches!(resolve(&providers, &to("far", ""), Use::ClipboardCases), Err(Unusable::Cannot(_))));
        assert!(matches!(resolve(&providers, &to("claude", ""), Use::ClipboardCases), Err(Unusable::Cannot(_))));
        assert!(matches!(resolve(&providers, &to("blank", ""), Use::Answers), Err(Unusable::NotSetUp(..))));
    }

    #[test]
    fn a_picker_offers_each_model_of_each_provider_that_can_be_used() {
        let mut ollama = server("ollama", "http://localhost:11434/v1");
        ollama.name = "Ollama".into();
        ollama.config.insert(MODELS.into(), "qwen3, extra".into());
        let mut code = Provider::new("claude-code", kinds::CLAUDE_CODE);
        code.name = "Claude Code".into();
        let providers = vec![Provider::new(LOCAL_SPEECH, kinds::LOCAL_SPEECH), ollama, code];
        let listed = BTreeMap::from([("ollama".to_string(), vec!["qwen3".to_string(), "llama3".to_string()])]);
        let titles = |need| offered(&providers, need, &listed).into_iter().map(|(_, title)| title).collect::<Vec<_>>();
        assert_eq!(titles(Use::Answers), ["Ollama · First Listed", "Ollama · qwen3", "Ollama · extra", "Ollama · llama3", "Claude Code · Default", "Claude Code · opus", "Claude Code · sonnet", "Claude Code · haiku"]);
        assert_eq!(titles(Use::ClipboardCases), ["Ollama · First Listed", "Ollama · qwen3", "Ollama · extra", "Ollama · llama3"]);
        // The speech models of this computer: the default (or the one downloaded) first.
        assert_eq!(offered(&providers, Use::DictationRecognition, &listed)[0].0, ModelRef::to(LOCAL_SPEECH, ""));
    }
}
