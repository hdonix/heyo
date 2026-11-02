//! The providers the user has connected, and which of their models each
//! feature uses.
//!
//! A provider is one place models are reached through: Claude's API, an
//! OpenAI-compatible server, Claude Code or Codex on this computer, the
//! speech models Heyo downloads and runs itself, or Doubao's speech
//! service. Its kind declares what it needs to be set up (`Kind::fields`)
//! and what it can do (`Can`); there may be several of one kind (two
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

use crate::i18n::marked;
use crate::settings::Settings;

use super::{address, Address, CLAUDE_MODEL};

/// What a provider is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ProviderKind")]
pub enum Kind {
    /// Claude, through Anthropic's API.
    Anthropic,
    /// Any server that speaks OpenAI's chat API: vLLM, LM Studio, Ollama,
    /// llama.cpp, on this computer or elsewhere.
    OpenaiCompatible,
    ClaudeCode,
    Codex,
    /// The speech models Heyo downloads and runs in its own process.
    LocalSpeech,
    /// Volcengine's streaming speech recognition.
    Doubao,
}

/// The kinds a user can add, in the order they are offered.
pub const ADDABLE: &[Kind] = &[Kind::Anthropic, Kind::OpenaiCompatible, Kind::ClaudeCode, Kind::Codex, Kind::Doubao];

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

const fn field(name: &'static str, title: &'static str, entry: Entry, placeholder: &'static str, description: &'static str) -> Field {
    Field { name, title, entry, placeholder, description, shown_when: None }
}

const fn when(field: Field, other: &'static str, value: &'static str) -> Field {
    Field { shown_when: Some((other, value)), ..field }
}

/// The address field of a server, and the key some servers want.
pub const URL: &str = "url";
pub const KEY: &str = "key";
pub const PATH: &str = "path";
pub const MODELS: &str = "models";

const ANTHROPIC_FIELDS: &[Field] = &[field(KEY, marked("API Key"), Entry::Secret, "sk-ant-…", marked("Leave empty to use ANTHROPIC_API_KEY or an `ant auth login` profile."))];
const OPENAI_FIELDS: &[Field] = &[
    field(URL, marked("Server"), Entry::Url, "http://127.0.0.1:8000/v1", marked("Any OpenAI-compatible server works (vLLM, LM Studio, Ollama).")),
    field(KEY, marked("API Key"), Entry::Secret, "", marked("Only for a server that asks for one.")),
    field(MODELS, marked("More Models"), Entry::List, "qwen3, llama3.3", marked("Models to offer besides the ones the server lists, separated by commas.")),
];
const CLAUDE_CODE_FIELDS: &[Field] = &[
    field(PATH, marked("Executable"), Entry::Path, "claude (from PATH)", ""),
    field(MODELS, marked("More Models"), Entry::List, "claude-opus-5-5", marked("Besides its default, opus, sonnet and haiku: a model's full name, separated by commas.")),
];
const CODEX_FIELDS: &[Field] = &[
    field(PATH, marked("Executable"), Entry::Path, "codex (from PATH)", ""),
    field(MODELS, marked("More Models"), Entry::List, "gpt-5.5", marked("Besides its default: models to offer, separated by commas. Heyo runs Codex without your config.toml, so its model setting does not apply here.")),
];
const DOUBAO_FIELDS: &[Field] = {
    use crate::dictation::doubao::*;
    &[
        field(CONSOLE, marked("Console"), Entry::Choice(CONSOLES), "", marked("Which of Volcengine's consoles your key is from.")),
        when(field(API_KEY, marked("API Key"), Entry::Secret, "", marked("From the new console.")), CONSOLE, "new"),
        when(field(APP_ID, marked("App ID"), Entry::Text, "", marked("From the old console, with its Access Token.")), CONSOLE, "old"),
        when(field(ACCESS_TOKEN, marked("Access Token"), Entry::Secret, "", marked("From the old console.")), CONSOLE, "old"),
        field(REMOVE_FILLERS, marked("Remove Filler Words"), Entry::Switch, "", marked("Doubao takes out filler words, stutters and repeated words.")),
    ]
};

impl Kind {
    /// What it is called, and what a new one is named.
    pub fn title(self) -> &'static str {
        match self {
            Kind::Anthropic => marked("Claude API"),
            Kind::OpenaiCompatible => marked("OpenAI-Compatible Server"),
            Kind::ClaudeCode => "Claude Code",
            Kind::Codex => "Codex",
            Kind::LocalSpeech => marked("Speech Models on This Computer"),
            Kind::Doubao => marked("Doubao (Volcengine)"),
        }
    }

    /// What it needs to be set up.
    pub fn fields(self) -> &'static [Field] {
        match self {
            Kind::Anthropic => ANTHROPIC_FIELDS,
            Kind::OpenaiCompatible => OPENAI_FIELDS,
            Kind::ClaudeCode => CLAUDE_CODE_FIELDS,
            Kind::Codex => CODEX_FIELDS,
            Kind::LocalSpeech => &[],
            Kind::Doubao => DOUBAO_FIELDS,
        }
    }

    /// Its name as it is stored, which an id is made from.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Anthropic => "anthropic",
            Kind::OpenaiCompatible => "openai-compatible",
            Kind::ClaudeCode => "claude-code",
            Kind::Codex => "codex",
            Kind::LocalSpeech => "local-speech",
            Kind::Doubao => "doubao",
        }
    }

    pub fn from_word(word: &str) -> Option<Kind> {
        [Kind::Anthropic, Kind::OpenaiCompatible, Kind::ClaudeCode, Kind::Codex, Kind::LocalSpeech, Kind::Doubao].into_iter().find(|kind| kind.word() == word)
    }

    /// Whether the user may remove it: not the speech models of this
    /// computer, which are always there.
    pub fn removable(self) -> bool {
        self != Kind::LocalSpeech
    }
}

/// A connected provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Provider {
    /// Its own, never reused while it is there: what references name.
    pub id: String,
    pub kind: Kind,
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
    pub fn new(id: &str, kind: Kind) -> Provider {
        Provider { id: id.into(), kind, name: crate::i18n::translate(kind.title()).into(), config: BTreeMap::new() }
    }

    /// Whether `field` applies, as the other fields are set.
    pub fn applies(&self, field: &Field) -> bool {
        let Some((other, value)) = field.shown_when else { return true };
        let first = self.kind.fields().iter().find(|known| known.name == other).and_then(|known| match known.entry {
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
        use Can::*;
        match self.kind {
            Kind::Anthropic | Kind::ClaudeCode | Kind::Codex => matches!(need, Text | Images | Streaming),
            // Only as private as where the server is.
            Kind::OpenaiCompatible => matches!(need, Text | Streaming) || (need == Private && address(self.get(URL)) == Address::ThisComputer),
            Kind::LocalSpeech => matches!(need, Speech | Streaming | Private),
            Kind::Doubao => matches!(need, Speech | Streaming),
        }
    }

    /// The models it offers that are known without asking it, the default
    /// (an empty name) first where it has one. An OpenAI-compatible server
    /// also offers what it lists (`openai::list`), which the settings window
    /// asks for.
    pub fn models(&self) -> Vec<String> {
        let mut models: Vec<String> = match self.kind {
            Kind::Anthropic => vec![CLAUDE_MODEL.into()],
            Kind::OpenaiCompatible | Kind::Codex => vec![String::new()],
            Kind::ClaudeCode => ["", "opus", "sonnet", "haiku"].map(String::from).to_vec(),
            // The default (or the one downloaded, `dictation::models::chosen`) first.
            Kind::LocalSpeech => std::iter::once(String::new()).chain(crate::dictation::models::CATALOGUE.iter().map(|model| model.id.to_owned())).collect(),
            Kind::Doubao => crate::dictation::doubao::RESOURCES.iter().map(|(id, _)| id.to_string()).collect(),
        };
        for more in self.listed(MODELS) {
            if !models.contains(&more) {
                models.push(more);
            }
        }
        models
    }

    /// What one of its models is called in a picker.
    pub fn model_title(&self, model: &str) -> String {
        use crate::i18n::t;
        match (self.kind, model) {
            (Kind::OpenaiCompatible, "") => t("First Listed").into(),
            (_, "") => t("Default").into(),
            (Kind::Doubao, id) => crate::dictation::doubao::RESOURCES.iter().find(|(known, _)| *known == id).map_or(id.to_owned(), |(_, title)| crate::i18n::translate(title).into()),
            (Kind::LocalSpeech, id) => crate::dictation::models::find(id).map_or(id.to_owned(), |model| model.name.to_owned()),
            (_, name) => name.into(),
        }
    }

    /// What is missing for it to be used at all, in words; none if nothing is.
    pub fn unset(&self) -> Option<&'static str> {
        use crate::i18n::t;
        match self.kind {
            Kind::OpenaiCompatible if self.get(URL).is_empty() => Some(t("The server's address is missing.")),
            Kind::Doubao if !crate::dictation::doubao::Config::keyed(self) => Some(t("The key is missing.")),
            _ => None,
        }
    }
}

/// The providers the settings begin with: the speech models of this
/// computer, and Claude's API (which needs no key where Anthropic's own
/// tools are signed in).
pub fn defaults() -> Vec<Provider> {
    vec![Provider::new(LOCAL_SPEECH, Kind::LocalSpeech), Provider::new(CLAUDE, Kind::Anthropic)]
}

/// Puts the speech models of this computer among `providers` if they are
/// not, and keeps them to one: they cannot be removed.
pub fn with_fixed(mut providers: Vec<Provider>) -> Vec<Provider> {
    let mut seen = false;
    providers.retain(|provider| {
        let fixed = provider.kind == Kind::LocalSpeech;
        let keep = !fixed || (!seen && provider.id == LOCAL_SPEECH);
        seen |= fixed && keep;
        keep
    });
    if !seen {
        providers.insert(0, Provider::new(LOCAL_SPEECH, Kind::LocalSpeech));
    }
    providers
}

/// An id for a new provider of `kind`: its kind's word, numbered after the
/// first.
pub fn new_id(providers: &[Provider], kind: Kind) -> String {
    let taken = |id: &str| providers.iter().any(|provider| provider.id == id);
    let word = kind.word();
    if !taken(word) {
        return word.into();
    }
    (2..).map(|n| format!("{word}-{n}")).find(|id| !taken(id)).expect("there is always another number")
}

/// What `field` of the provider `id` names is set to `value`: one of its
/// kind's fields, or its `name`. Values are kept trimmed; an empty secret
/// is a secret removed.
pub fn set_field(providers: &mut [Provider], id: &str, field: &str, value: &str) -> Result<(), String> {
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
    if !provider.kind.fields().iter().any(|known| known.name == field) {
        return Err(tf("{0} has no setting {1}.", &[&provider.name, &field]));
    }
    if value.is_empty() {
        provider.config.remove(field);
    } else {
        provider.config.insert(field.into(), value.into());
    }
    Ok(())
}

/// Takes the provider `id` away, if it is one that can be.
pub fn remove(providers: &mut Vec<Provider>, id: &str) -> Result<(), String> {
    use crate::i18n::tf;
    let at = providers.iter().position(|provider| provider.id == id).ok_or_else(|| tf("There is no provider {0}.", &[&id]))?;
    if !providers[at].kind.removable() {
        return Err(tf("{0} cannot be removed.", &[&providers[at].name]));
    }
    providers.remove(at);
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

    /// The setting, as it is stored.
    pub fn setting(self) -> &'static str {
        match self {
            Use::Answers => "aiModel",
            Use::DictationRecognition => "dictationModel",
            Use::DictationPolish => "polishModel",
            Use::ClipboardCases => "casesModel",
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
        let name = if provider.kind == Kind::LocalSpeech { crate::i18n::t("This Computer") } else { provider.name.as_str() };
        offered.extend(models.into_iter().map(|model| (ModelRef::to(&provider.id, &model), format!("{name} · {}", provider.model_title(&model)))));
    }
    offered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: &str, url: &str) -> Provider {
        Provider { config: BTreeMap::from([(URL.into(), url.into())]), ..Provider::new(id, Kind::OpenaiCompatible) }
    }

    #[test]
    fn each_kind_declares_what_it_can_do() {
        let claude = Provider::new("claude", Kind::Anthropic);
        assert!(claude.can(Can::Text) && claude.can(Can::Images) && !claude.can(Can::Speech) && !claude.can(Can::Private));
        // A tool on this computer sends what it is asked to its maker's service.
        for kind in [Kind::ClaudeCode, Kind::Codex] {
            assert!(Provider::new("x", kind).can(Can::Images) && !Provider::new("x", kind).can(Can::Private));
        }
        // A server is as private as where it is.
        assert!(server("a", "http://127.0.0.1:11434/v1").can(Can::Private));
        assert!(!server("b", "http://192.168.1.20:8000/v1").can(Can::Private));
        assert!(!server("c", "http://127.0.0.1:8000/v1").can(Can::Images));
        let speech = Provider::new(LOCAL_SPEECH, Kind::LocalSpeech);
        assert!(speech.can(Can::Speech) && speech.can(Can::Private) && !speech.can(Can::Text));
        let doubao = Provider::new("doubao", Kind::Doubao);
        assert!(doubao.can(Can::Speech) && !doubao.can(Can::Private));
    }

    #[test]
    fn the_speech_models_of_this_computer_are_always_there_once() {
        assert_eq!(with_fixed(Vec::new()).iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), [LOCAL_SPEECH]);
        let twice = vec![Provider::new("claude", Kind::Anthropic), Provider::new(LOCAL_SPEECH, Kind::LocalSpeech), Provider::new("other", Kind::LocalSpeech)];
        assert_eq!(with_fixed(twice).iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["claude", LOCAL_SPEECH]);
        assert!(!Kind::LocalSpeech.removable() && ADDABLE.iter().all(|kind| kind.removable()));
    }

    #[test]
    fn a_provider_is_set_up_field_by_field_and_removed_unless_it_is_fixed() {
        let mut providers = defaults();
        set_field(&mut providers, CLAUDE, KEY, "  sk-ant-1 ").unwrap();
        set_field(&mut providers, CLAUDE, "name", " Work ").unwrap();
        assert_eq!((providers[1].get(KEY), providers[1].name.as_str()), ("sk-ant-1", "Work"));
        // An empty secret is one removed; a field the kind has not, or no name, is refused.
        set_field(&mut providers, CLAUDE, KEY, "").unwrap();
        assert!(!providers[1].config.contains_key(KEY));
        assert!(set_field(&mut providers, CLAUDE, URL, "http://x").is_err());
        assert!(set_field(&mut providers, CLAUDE, "name", " ").is_err());
        assert!(set_field(&mut providers, "gone", KEY, "x").is_err());
        assert!(remove(&mut providers, LOCAL_SPEECH).is_err());
        remove(&mut providers, CLAUDE).unwrap();
        assert_eq!(providers.len(), 1);
        assert!(remove(&mut providers, CLAUDE).is_err());
    }

    #[test]
    fn several_of_one_kind_each_have_an_id() {
        let mut providers = defaults();
        let first = new_id(&providers, Kind::OpenaiCompatible);
        providers.push(server(&first, ""));
        let second = new_id(&providers, Kind::OpenaiCompatible);
        assert_eq!((first.as_str(), second.as_str()), ("openai-compatible", "openai-compatible-2"));
        assert_eq!(new_id(&providers, Kind::Anthropic), "anthropic");
    }

    #[test]
    fn a_reference_reaches_only_what_can_be_used_for_it() {
        let providers = vec![Provider::new(LOCAL_SPEECH, Kind::LocalSpeech), Provider::new("claude", Kind::Anthropic), server("ollama", "http://localhost:11434/v1"), server("far", "https://api.example.com/v1"), server("blank", "")];
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
        let mut code = Provider::new("claude-code", Kind::ClaudeCode);
        code.name = "Claude Code".into();
        let providers = vec![Provider::new(LOCAL_SPEECH, Kind::LocalSpeech), ollama, code];
        let listed = BTreeMap::from([("ollama".to_string(), vec!["qwen3".to_string(), "llama3".to_string()])]);
        let titles = |need| offered(&providers, need, &listed).into_iter().map(|(_, title)| title).collect::<Vec<_>>();
        assert_eq!(titles(Use::Answers), ["Ollama · First Listed", "Ollama · qwen3", "Ollama · extra", "Ollama · llama3", "Claude Code · Default", "Claude Code · opus", "Claude Code · sonnet", "Claude Code · haiku"]);
        assert_eq!(titles(Use::ClipboardCases), ["Ollama · First Listed", "Ollama · qwen3", "Ollama · extra", "Ollama · llama3"]);
        // The speech models of this computer: the default (or the one downloaded) first.
        assert_eq!(offered(&providers, Use::DictationRecognition, &listed)[0].0, ModelRef::to(LOCAL_SPEECH, ""));
    }
}
