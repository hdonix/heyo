//! The kinds of provider, each in a file of its own: what it is called,
//! what it needs to be set up, what it can do, which models it has and
//! where they come from, how it is said to be and checked, and what it
//! opens when a feature uses one of its models (a text model, a speech
//! recognizer).
//!
//! A kind is a [`ProviderKind`], registered once in [`KINDS`]. Everything
//! else (the settings window's pages, the pickers, what a use resolves
//! to) asks the kind and names none: a new kind is a file here and a line
//! in that list.

mod anthropic;
mod claude_code;
mod codex;
mod doubao;
mod local_speech;
mod openai;

use crate::dictation::engine::Failure;
use crate::i18n::{marked, t, tf, tn};

use super::registry::{Can, Field, Provider};

pub(crate) use anthropic::WORD as ANTHROPIC;
#[cfg(test)]
pub(crate) use claude_code::WORD as CLAUDE_CODE;
#[cfg(test)]
pub(crate) use codex::WORD as CODEX;
#[cfg(test)]
pub(crate) use doubao::WORD as DOUBAO;
pub(crate) use local_speech::WORD as LOCAL_SPEECH;
#[cfg(test)]
pub(crate) use openai::WORD as OPENAI_COMPATIBLE;

/// Every kind there is, in the order they are offered.
pub(crate) const KINDS: &[&dyn ProviderKind] = &[&local_speech::LocalSpeech, &anthropic::Anthropic, &openai::OpenaiCompatible, &claude_code::ClaudeCode, &codex::Codex, &doubao::Doubao];

/// The kind stored as `word`.
pub(crate) fn find(word: &str) -> Option<&'static dyn ProviderKind> {
    KINDS.iter().copied().find(|kind| kind.word() == word)
}

/// The kinds a user can add: all but those that are always there.
pub(crate) fn addable() -> impl Iterator<Item = &'static dyn ProviderKind> {
    KINDS.iter().copied().filter(|kind| !kind.builtin())
}

/// Where a kind's models come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    /// Known without asking it (with those the user adds, where it has a
    /// field for them).
    Fixed,
    /// Asked of the service, which lists them ([`ProviderKind::list`]).
    Listed,
    /// A catalogue Heyo downloads from and runs itself, each model with
    /// its state (`dictation::models`).
    Catalogue,
}

/// What is known of a provider besides its settings, which how it is
/// ([`ProviderKind::status`]) is said from. The settings window has it
/// from the host's answers.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Known {
    /// A command-line tool: its version and where it is, or why there is
    /// none to run.
    pub found: Option<Result<(String, String), String>>,
    /// The models the service listed, or why it could not be asked.
    pub listed: Option<Result<Vec<String>, String>>,
    /// How many of a catalogue's models are on this computer.
    pub downloaded: Option<usize>,
}

/// How a provider is: in a word or two for the list, in full for its
/// page, and whether that is a warning.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Status {
    pub short: String,
    pub long: String,
    pub warning: bool,
}

impl Status {
    pub(crate) fn fine(short: &str, long: impl Into<String>) -> Status {
        Status { short: short.into(), long: long.into(), warning: false }
    }

    pub(crate) fn warns(short: &str, long: impl Into<String>) -> Status {
        Status { short: short.into(), long: long.into(), warning: true }
    }
}

/// What is missing for a provider to be used at all: in a word or two,
/// and in full. Both `marked`, translated where they are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Missing {
    pub short: &'static str,
    pub said: &'static str,
}

/// What checking a provider's connection came to: what to say of it, and
/// the models the service listed, where it lists them.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Checked {
    pub said: String,
    pub models: Option<Vec<String>>,
}

/// What hears dictation, as a provider of speech opens it.
pub(crate) use crate::dictation::engine::Opener;

/// The words of what a provider can do, as the chips beside it say it.
pub(crate) const ABILITIES: &[(Can, &str)] = &[(Can::Text, marked("Text")), (Can::Images, marked("Images")), (Can::Speech, marked("Speech")), (Can::Streaming, marked("Streaming")), (Can::Private, marked("Private"))];

/// A kind of provider.
pub(crate) trait ProviderKind: Sync {
    /// Its name as it is stored, which an id is made from.
    fn word(&self) -> &'static str;
    /// What it is called (`marked`), and what a new one is named.
    fn title(&self) -> &'static str;
    /// The symbol it is shown with.
    fn icon(&self) -> &'static str;
    /// What it is, in a sentence (`marked`).
    fn about(&self) -> &'static str;
    /// What it needs to be set up, with what a new one starts with.
    fn fields(&self) -> &'static [Field] {
        &[]
    }
    /// Whether it is always there, once: never added or removed.
    fn builtin(&self) -> bool {
        false
    }
    /// What it can do.
    fn can(&self, provider: &Provider, need: Can) -> bool;
    /// Where what it is given goes, in words; none where its status says it.
    fn goes(&self, _provider: &Provider) -> &'static str {
        ""
    }
    fn source(&self) -> Source {
        Source::Fixed
    }
    /// The models it has that are known without asking it, the default
    /// (an empty name) first where it has one. Those of a `models` field
    /// are added to them (`Provider::models`).
    fn models(&self, provider: &Provider) -> Vec<String>;
    /// What one of its models is called.
    fn model_title(&self, _provider: &Provider, model: &str) -> String {
        if model.is_empty() { t("Default").into() } else { model.into() }
    }
    /// What is missing for it to be used at all; none if nothing is.
    fn unset(&self, _provider: &Provider) -> Option<Missing> {
        None
    }
    /// How it is, by what is known of it.
    fn status(&self, provider: &Provider, known: &Known) -> Status {
        status(self.unset(provider), known)
    }
    /// The program it runs, where it is one: where it is and its version,
    /// or why there is none to run. Asked as the settings are shown.
    fn found(&self, _provider: &Provider) -> Option<Result<super::cli::Found, String>> {
        None
    }
    /// The models the service lists. Blocks: for a thread that can wait.
    fn list(&self, _provider: &Provider) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
    /// Whether it answers as it is set up: a connection, a sign-in, a
    /// program found. Blocks, up to some seconds: for a thread that can wait.
    fn check(&self, provider: &Provider) -> Result<Checked, String>;
    /// `model` of it, as the adapter of a model that answers in text;
    /// none if it has none. What it can be given besides text is the
    /// adapter's to say ([`text_can`]).
    fn text(&self, _provider: &Provider, _model: &str) -> Option<Box<dyn super::TextAdapter>> {
        None
    }
    /// What hears dictation with `model` of it, with the words of
    /// `vocabulary`; none if it does not recognize speech.
    fn recognizer(&self, _provider: &Provider, _model: &str, _vocabulary: &crate::dictation::vocabulary::Vocabulary) -> Option<Result<Box<dyn Opener>, Failure>> {
        None
    }
    /// What extensions are told answers `AI.ask` when one of its models does.
    fn api_name(&self) -> &'static str {
        ""
    }
}

/// How a provider is, as most say it: what is missing, else what the
/// service listed or the tool that was found, else that it is set up.
pub(crate) fn status(missing: Option<Missing>, known: &Known) -> Status {
    if let Some(missing) = missing {
        return Status::warns(crate::i18n::translate(missing.short), crate::i18n::translate(missing.said));
    }
    match (&known.listed, &known.found, known.downloaded) {
        (Some(Ok(models)), ..) if models.is_empty() => Status::warns(t("No models"), t("Connected, but the server lists no models.")),
        (Some(Ok(models)), ..) => Status::fine(&tn("{0} model", "{0} models", models.len() as u64, &[&models.len()]), tf("Connected: {0}.", &[&models.join(", ")])),
        (Some(Err(e)), ..) => Status::warns(t("Not reachable"), tf("Not reachable: {0}.", &[e])),
        (_, Some(Ok((version, path))), _) => Status::fine(t("Set up"), tf("Using {0} at {1}.", &[version, path])),
        (_, Some(Err(why)), _) => Status::warns(t("Not found"), why.clone()),
        (.., Some(0)) => Status::fine(t("Nothing downloaded"), t("Download a model below to dictate without sending your voice anywhere.")),
        (.., Some(n)) => Status::fine(&tn("{0} downloaded", "{0} downloaded", n as u64, &[&n]), t("Downloaded and run by Heyo itself.")),
        (None, None, None) => Status::fine(t("Set up"), ""),
    }
}

/// What a kind that answers in text can do as one: answer, and what its
/// adapter says it can besides.
pub(crate) fn text_can(kind: &dyn ProviderKind, provider: &Provider, need: Can) -> bool {
    kind.text(provider, "").is_some_and(|adapter| need == Can::Text || adapter.can(need))
}

/// A failure of speech recognition, in words.
pub(crate) fn worded(failure: &Failure) -> String {
    crate::helper::worded(&failure.said(), t("Dictation failed")).full()
}

/// What an unknown kind is: a provider of a kind this Heyo does not have.
pub(crate) struct Unknown;

impl ProviderKind for Unknown {
    fn word(&self) -> &'static str {
        ""
    }
    fn title(&self) -> &'static str {
        marked("Unknown Provider")
    }
    fn icon(&self) -> &'static str {
        "question-mark-circle"
    }
    fn about(&self) -> &'static str {
        ""
    }
    fn can(&self, _provider: &Provider, _need: Can) -> bool {
        false
    }
    fn models(&self, _provider: &Provider) -> Vec<String> {
        Vec::new()
    }
    fn check(&self, _provider: &Provider) -> Result<Checked, String> {
        Err(t("Heyo does not know this kind of provider.").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_is_registered_once_by_its_own_word() {
        let mut words: Vec<&str> = KINDS.iter().map(|kind| kind.word()).collect();
        assert!(words.iter().all(|word| find(word).is_some_and(|kind| kind.word() == *word)));
        words.sort();
        words.dedup();
        assert_eq!(words.len(), KINDS.len());
        assert!(find("no-such").is_none());
        // Only the speech models of this computer are always there.
        assert_eq!(KINDS.iter().filter(|kind| kind.builtin()).map(|kind| kind.word()).collect::<Vec<_>>(), [LOCAL_SPEECH]);
        assert_eq!(addable().count(), KINDS.len() - 1);
        for kind in KINDS {
            assert!(crate::slint_windows::icons::svg(kind.icon()).is_some(), "{}", kind.word());
        }
    }

    #[test]
    fn a_status_says_what_is_missing_then_what_is_known() {
        let missing = Missing { short: "Key missing", said: "The key is missing." };
        assert_eq!(status(Some(missing), &Known::default()), Status::warns("Key missing", "The key is missing."));
        let listed = Known { listed: Some(Ok(vec!["a".into(), "b".into(), "c".into()])), ..Known::default() };
        assert_eq!(status(None, &listed).short, "3 models");
        let refused = Known { listed: Some(Err("connection refused".into())), ..Known::default() };
        assert!(status(None, &refused).warning);
        let lost = Known { found: Some(Err("Claude Code was not found.".into())), ..Known::default() };
        assert_eq!((status(None, &lost).short.as_str(), status(None, &lost).warning), ("Not found", true));
        let found = Known { found: Some(Ok(("2.1.0".into(), "/usr/bin/claude".into()))), ..Known::default() };
        assert_eq!(status(None, &found).short, "Set up");
        assert_eq!(status(None, &Known { downloaded: Some(2), ..Known::default() }).short, "2 downloaded");
        assert_eq!(status(None, &Known::default()).short, "Set up");
    }
}
