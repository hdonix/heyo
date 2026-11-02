//! Claude, through Anthropic's API (`models/claude.rs`).

use crate::i18n::{marked, t};
use crate::models::registry::{field, Can, Entry, Field, Provider, KEY};
use crate::models::{claude, Model, CLAUDE_MODEL};

use super::{Checked, Known, ProviderKind, Status};

pub(crate) const WORD: &str = "anthropic";

const FIELDS: &[Field] = &[field(KEY, marked("API Key"), Entry::Secret, "sk-ant-…", marked("Leave empty to use ANTHROPIC_API_KEY or an `ant auth login` profile."))];

pub(super) struct Anthropic;

impl ProviderKind for Anthropic {
    fn word(&self) -> &'static str {
        WORD
    }
    fn title(&self) -> &'static str {
        marked("Claude API")
    }
    fn icon(&self) -> &'static str {
        "sparkles"
    }
    fn about(&self) -> &'static str {
        marked("Claude, with an API key or your `ant auth login` sign-in.")
    }
    fn fields(&self) -> &'static [Field] {
        FIELDS
    }
    fn can(&self, _provider: &Provider, need: Can) -> bool {
        matches!(need, Can::Text | Can::Images | Can::Streaming)
    }
    fn goes(&self, _provider: &Provider) -> &'static str {
        marked("What it is given is sent to Anthropic.")
    }
    fn models(&self, _provider: &Provider) -> Vec<String> {
        vec![CLAUDE_MODEL.into()]
    }
    fn status(&self, provider: &Provider, _known: &Known) -> Status {
        if provider.get(KEY).is_empty() {
            Status::fine(t("Set up"), t("No API key saved: Heyo uses ANTHROPIC_API_KEY or an `ant auth login` profile, if there is one."))
        } else {
            Status::fine(t("Set up"), t("Connected with an API key."))
        }
    }
    fn check(&self, provider: &Provider) -> Result<Checked, String> {
        tauri::async_runtime::block_on(claude::Claude::new(provider.get(KEY)).check()).map_err(|e| e.message())?;
        Ok(Checked { said: t("Claude's API takes the key.").into(), models: None })
    }
    fn text(&self, provider: &Provider, _model: &str) -> Option<Model> {
        Some(Model::Claude(claude::Claude::new(provider.get(KEY))))
    }
    fn api_name(&self) -> &'static str {
        "claude"
    }
}
