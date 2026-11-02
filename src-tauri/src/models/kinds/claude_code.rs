//! Claude Code on this computer, answering with the account it is signed
//! in to (`models/cli.rs`). What it and Codex share is here too.

use crate::i18n::{marked, t, tf};
use crate::models::claude_code::{self, PROGRAM};
use crate::models::cli::{self, Program};
use crate::models::registry::{field, Can, Entry, Field, Provider, MODELS, PATH};
use crate::models::TextAdapter;

use super::{text_can, Checked, Known, ProviderKind, Status};

pub(crate) const WORD: &str = "claude-code";

const FIELDS: &[Field] = &[
    field(PATH, marked("Executable"), Entry::Path, "claude (from PATH)", ""),
    field(MODELS, marked("More Models"), Entry::List, "claude-opus-5-5", marked("Besides its default, opus, sonnet and haiku: a model's full name, separated by commas.")),
];

pub(super) struct ClaudeCode;

impl ProviderKind for ClaudeCode {
    fn word(&self) -> &'static str {
        WORD
    }
    fn title(&self) -> &'static str {
        "Claude Code"
    }
    fn icon(&self) -> &'static str {
        "square-terminal"
    }
    fn about(&self) -> &'static str {
        marked("Claude Code on this computer, with the account it is signed in to: no API key is needed.")
    }
    fn fields(&self) -> &'static [Field] {
        FIELDS
    }
    fn can(&self, provider: &Provider, need: Can) -> bool {
        text_can(self, provider, need)
    }
    fn goes(&self, _provider: &Provider) -> &'static str {
        marked("Runs on this computer, and sends what it is given to Anthropic.")
    }
    fn models(&self, _provider: &Provider) -> Vec<String> {
        ["", "opus", "sonnet", "haiku"].map(String::from).to_vec()
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
        Some(Box::new(claude_code::ClaudeCode::new(provider.get(PATH), model)))
    }
    fn api_name(&self) -> &'static str {
        "claude-code"
    }
}

/// How a command-line tool is. It runs here, but sends what it is asked
/// to its maker's service, so it is not private (its `can` says none).
pub(super) fn tool_status(program: &Program, known: &Known) -> Status {
    let status = match &known.found {
        None => Status::fine(t("Checking…"), t("Checking…")),
        Some(_) => super::status(None, known),
    };
    let signed_in = tf("{0} answers with the account it is signed in to ({1}); no API key is needed.", &[&program.name, &program.sign_in]);
    Status { long: format!("{} {signed_in}", status.long), ..status }
}

/// Whether the tool is where the settings say, and answers.
pub(super) fn tool_check(program: &'static Program, provider: &Provider) -> Result<Checked, String> {
    let found = cli::found(program, provider.get(PATH))?;
    Ok(Checked { said: tf("Found {0} {1} at {2}.", &[&program.name, &found.version, &found.path]), models: None })
}
