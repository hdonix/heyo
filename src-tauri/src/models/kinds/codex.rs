//! Codex on this computer, answering with the account it is signed in to
//! (`models/cli.rs`).

use crate::i18n::marked;
use crate::models::cli;
use crate::models::codex::{self, PROGRAM};
use crate::models::registry::{field, Can, Entry, Field, Provider, MODELS, PATH};
use crate::models::TextAdapter;

use super::claude_code::{tool_check, tool_status};
use super::{text_can, Checked, Known, ProviderKind, Status};

pub(crate) const WORD: &str = "codex";

const FIELDS: &[Field] = &[
    field(PATH, marked("Executable"), Entry::Path, "codex (from PATH)", ""),
    field(MODELS, marked("More Models"), Entry::List, "gpt-5.5", marked("Besides its default: models to offer, separated by commas. Heyo runs Codex without your config.toml, so its model setting does not apply here.")),
];

pub(super) struct Codex;

impl ProviderKind for Codex {
    fn word(&self) -> &'static str {
        WORD
    }
    fn title(&self) -> &'static str {
        "Codex"
    }
    fn icon(&self) -> &'static str {
        "code-xml"
    }
    fn about(&self) -> &'static str {
        marked("OpenAI's Codex on this computer, with the account it is signed in to: no API key is needed.")
    }
    fn fields(&self) -> &'static [Field] {
        FIELDS
    }
    fn can(&self, provider: &Provider, need: Can) -> bool {
        text_can(self, provider, need)
    }
    fn goes(&self, _provider: &Provider) -> &'static str {
        marked("Runs on this computer, and sends what it is given to OpenAI.")
    }
    fn models(&self, _provider: &Provider) -> Vec<String> {
        vec![String::new()]
    }
    fn status(&self, _provider: &Provider, known: &Known) -> Status {
        tool_status(&PROGRAM, known)
    }
    fn found(&self, provider: &Provider) -> Option<Result<cli::Found, String>> {
        Some(cli::found(&PROGRAM, provider.get(PATH)))
    }
    fn check(&self, provider: &Provider) -> Result<Checked, String> {
        tool_check(&PROGRAM, provider)
    }
    fn text(&self, provider: &Provider, model: &str) -> Option<Box<dyn TextAdapter>> {
        Some(Box::new(codex::Codex::new(provider.get(PATH), model)))
    }
    fn api_name(&self) -> &'static str {
        "codex"
    }
}
