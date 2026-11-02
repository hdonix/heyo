//! Claude Code on this computer, answering with the account it is signed
//! in to (`models/cli.rs`). What it and Codex share is here too.

use crate::i18n::{marked, t, tf};
use crate::models::cli::{self, Tool};
use crate::models::registry::{field, Can, Entry, Field, Provider, MODELS, PATH};
use crate::models::Model;

use super::{Checked, Known, ProviderKind, Status};

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
    fn can(&self, _provider: &Provider, need: Can) -> bool {
        tool_can(need)
    }
    fn goes(&self, _provider: &Provider) -> &'static str {
        marked("Runs on this computer, and sends what it is given to Anthropic.")
    }
    fn models(&self, _provider: &Provider) -> Vec<String> {
        ["", "opus", "sonnet", "haiku"].map(String::from).to_vec()
    }
    fn status(&self, _provider: &Provider, known: &Known) -> Status {
        tool_status(Tool::ClaudeCode, known)
    }
    fn found(&self, provider: &Provider) -> Option<Result<cli::Found, String>> {
        Some(cli::found(Tool::ClaudeCode, provider.get(PATH)))
    }
    fn check(&self, provider: &Provider) -> Result<Checked, String> {
        tool_check(Tool::ClaudeCode, provider)
    }
    fn text(&self, provider: &Provider, model: &str) -> Option<Model> {
        Some(Model::Cli(cli::Cli::new(Tool::ClaudeCode, provider.get(PATH), model)))
    }
    fn api_name(&self) -> &'static str {
        "claude-code"
    }
}

/// What a command-line tool can do: it runs here, but sends what it is
/// asked to its maker's service, so it is not private.
pub(super) fn tool_can(need: Can) -> bool {
    matches!(need, Can::Text | Can::Images | Can::Streaming)
}

pub(super) fn tool_status(tool: Tool, known: &Known) -> Status {
    let status = match &known.found {
        None => Status::fine(t("Checking…"), t("Checking…")),
        Some(_) => super::status(None, known),
    };
    let signed_in = tf("{0} answers with the account it is signed in to ({1}); no API key is needed.", &[&tool.name(), &tool.sign_in()]);
    Status { long: format!("{} {signed_in}", status.long), ..status }
}

/// Whether the tool is where the settings say, and answers.
pub(super) fn tool_check(tool: Tool, provider: &Provider) -> Result<Checked, String> {
    let found = cli::found(tool, provider.get(PATH))?;
    Ok(Checked { said: tf("Found {0} {1} at {2}.", &[&tool.name(), &found.version, &found.path]), models: None })
}
