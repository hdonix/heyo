//! Claude through Claude Code (`claude`), signed in with the user's
//! Claude account: one of the adapters of `models`, through `cli.rs`.
//!
//! It is run once per question, as Claude Code 2.1's `--help` says:
//!
//! ```text
//! claude -p --input-format stream-json --output-format stream-json --verbose
//!        --include-partial-messages --system-prompt-file <scratch>/system.md
//!        --tools "" --strict-mcp-config --safe-mode --disable-slash-commands
//!        --no-session-persistence --permission-mode dontAsk --permission-prompts none
//!        --effort low [--model <model>]
//! ```
//!
//! - `-p` answers once and exits; the question goes on standard input as
//!   one user message of the stream-json input (the Agent SDK's), which can
//!   hold pictures beside the text, and the answer comes as stream-json
//!   events, the text as the API's own `text_delta`s
//!   (`--include-partial-messages`; stream-json output needs `--verbose`).
//! - The system prompt replaces Claude Code's own (a coding agent's).
//! - `--tools ""`: no tools at all, so no file is read or written and
//!   nothing is run. `--strict-mcp-config` with no `--mcp-config` loads no
//!   MCP server; `--safe-mode` leaves out CLAUDE.md, skills, plugins, hooks,
//!   custom commands and agents; `--disable-slash-commands` the skills
//!   again. `dontAsk` with nobody to answer (`--permission-prompts none`)
//!   denies whatever would still ask.
//! - `--no-session-persistence`: nothing of the question is kept on disk.
//! - `--effort low`: Heyo asks for short rewriting and explaining, as it
//!   does of the API.
//!
//! What is not prevented: the user's settings file (`~/.claude/settings.json`)
//! is still read, for its environment and its model, which is how the
//! user's sign-in and choices apply; its hooks and MCP servers are not run
//! (`--safe-mode`, `--strict-mcp-config`). Policy settings an administrator
//! manages apply too, as they must.

use std::ffi::OsString;
use std::path::Path;

use serde_json::{json, Value};

use super::cli::{self, Cli, Scratch};
use super::{ModelError, Question};

pub(super) async fn answer(cli: &Cli, program: &Path, scratch: &Scratch, question: &Question, on_text: &mut (dyn FnMut(&str) + Send)) -> Result<String, ModelError> {
    let system = scratch.file("system.md", cli::system(question).as_bytes()).map_err(|e| cli.failed(e.to_string()))?;
    let args = arguments(&system, &cli.model);
    let mut stdin = input(question).to_string();
    stdin.push('\n');

    let mut reading = Reading::default();
    let ran = cli::run(cli, program, scratch, args, stdin.into_bytes(), |line| reading.line(line, on_text)).await?;
    if let Some(said) = reading.failed {
        return Err(cli.failure(&said));
    }
    match reading.end {
        Some(End::Declined) => Err(ModelError::Declined),
        _ if !reading.text.trim().is_empty() => Ok(reading.text.trim().to_owned()),
        Some(End::Answered) => Err(ModelError::CliSaidNothing(cli.tool)),
        None => Err(cli.ended_without_answer(&ran)),
    }
}

/// The command line, but for the program.
fn arguments(system: &Path, model: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--system-prompt-file"].map(OsString::from).to_vec();
    args.push(system.into());
    args.extend(
        ["--tools", "", "--strict-mcp-config", "--safe-mode", "--disable-slash-commands", "--no-session-persistence", "--permission-mode", "dontAsk", "--permission-prompts", "none", "--effort", "low"].map(OsString::from),
    );
    if !model.is_empty() {
        args.extend(["--model".into(), model.into()]);
    }
    args
}

/// The user message, as a line of stream-json input: the pictures, then the text.
fn input(question: &Question) -> Value {
    let mut content: Vec<Value> = question.images.iter().map(|image| json!({ "type": "image", "source": { "type": "base64", "media_type": image.media_type, "data": image.data } })).collect();
    content.push(json!({ "type": "text", "text": cli::message(question) }));
    json!({ "type": "user", "session_id": "", "parent_tool_use_id": null, "message": { "role": "user", "content": content } })
}

#[derive(Debug, PartialEq)]
enum End {
    Answered,
    Declined,
}

/// What has been read of the events so far.
#[derive(Default)]
struct Reading {
    /// The answer's text so far.
    text: String,
    /// The answer as a whole message, should no piece of it have come.
    whole: String,
    /// Why it stopped, once it has.
    stop: Option<String>,
    end: Option<End>,
    /// What it said went wrong.
    failed: Option<String>,
}

impl Reading {
    /// Takes in one line of output; false once there is nothing more to wait for.
    fn line(&mut self, line: &str, on_text: &mut (dyn FnMut(&str) + Send)) -> bool {
        let Ok(event) = serde_json::from_str::<Value>(line) else { return true };
        match event["type"].as_str().unwrap_or_default() {
            "stream_event" => {
                let event = &event["event"];
                match event["type"].as_str().unwrap_or_default() {
                    // A message of its own: the answer is the last one's text.
                    "message_start" => self.text.clear(),
                    "content_block_delta" if event["delta"]["type"] == "text_delta" => {
                        self.text.push_str(event["delta"]["text"].as_str().unwrap_or_default());
                        on_text(&self.text);
                    }
                    "message_delta" => self.stop = event["delta"]["stop_reason"].as_str().map(str::to_owned),
                    // The answer is whole unless it stopped to use a tool
                    // (which it has none of). What Claude Code does after
                    // it (a summary of the turn, seconds long) is not waited for.
                    "message_stop" => match self.stop.as_deref() {
                        Some("tool_use") => {}
                        Some("refusal") => {
                            self.end = Some(End::Declined);
                            return false;
                        }
                        _ if !self.text.trim().is_empty() => {
                            self.end = Some(End::Answered);
                            return false;
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            "assistant" => {
                let text: String = event["message"]["content"].as_array().into_iter().flatten().filter(|block| block["type"] == "text").filter_map(|block| block["text"].as_str()).collect();
                self.whole = text;
            }
            "result" => {
                if event["is_error"] == true || event["subtype"].as_str().is_some_and(|kind| kind != "success") {
                    let errors: Vec<&str> = event["errors"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                    let said = event["result"].as_str().filter(|said| !said.trim().is_empty()).map(str::to_owned).unwrap_or_else(|| errors.join(" "));
                    self.failed = Some(if said.is_empty() { event["subtype"].as_str().unwrap_or("error").to_owned() } else { said });
                } else {
                    if self.text.trim().is_empty() {
                        self.text = event["result"].as_str().map(str::to_owned).filter(|text| !text.trim().is_empty()).unwrap_or_else(|| std::mem::take(&mut self.whole));
                        if !self.text.trim().is_empty() {
                            on_text(&self.text);
                        }
                    }
                    self.end = Some(if event["stop_reason"] == "refusal" { End::Declined } else { End::Answered });
                }
                return false;
            }
            _ => {}
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::cli::tests::{ask, Stub};
    use super::super::cli::Tool;
    use super::super::{Model, PromptImage, Turn};
    use super::*;

    /// What Claude Code 2.1.302 wrote when asked, with these very
    /// arguments, for one word on the colour of a red picture.
    const REAL: &str = include_str!("fixtures/claude-code.jsonl");

    fn claude(stub: &Stub) -> Model {
        Model::Cli(Cli::new(Tool::ClaudeCode, &stub.path("claude"), ""))
    }

    /// A stub that prints `lines`.
    fn printing(lines: &[String]) -> Stub {
        let stub = Stub::new("claude", "");
        std::fs::write(stub.dir.join("out"), lines.join("\n") + "\n").unwrap();
        stub.then("cat \"$here/out\"")
    }

    fn stream(event: Value) -> String {
        json!({ "type": "stream_event", "event": event, "session_id": "s", "parent_tool_use_id": null }).to_string()
    }

    /// An answer as Claude Code streams it: `pieces` of text.
    fn answer_lines(pieces: &[&str], stop: &str) -> Vec<String> {
        let mut lines = vec![json!({ "type": "system", "subtype": "init", "tools": [], "mcp_servers": [] }).to_string()];
        lines.push(stream(json!({ "type": "message_start", "message": { "model": "claude-opus-4-8", "content": [] } })));
        lines.push(stream(json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "text", "text": "" } })));
        for piece in pieces {
            lines.push(stream(json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": piece } })));
        }
        lines.push(json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": pieces.concat() }] } }).to_string());
        lines.push(stream(json!({ "type": "content_block_stop", "index": 0 })));
        lines.push(stream(json!({ "type": "message_delta", "delta": { "stop_reason": stop } })));
        lines.push(stream(json!({ "type": "message_stop" })));
        lines.push(json!({ "type": "result", "subtype": "success", "is_error": false, "result": pieces.concat(), "stop_reason": stop }).to_string());
        lines
    }

    #[test]
    fn its_real_output_is_read_and_the_picture_goes_with_the_text() {
        let stub = printing(&REAL.lines().map(str::to_owned).collect::<Vec<_>>());
        let question = Question { images: vec![PromptImage { media_type: "image/png".into(), data: "iVBORw0KGgo=".into() }], ..Question::material("Name the colour.", "") };
        let (answer, seen) = ask(&claude(&stub), &question);
        assert_eq!(answer, Ok("Red".into()));
        assert_eq!(seen, ["Red"]);
        let given: Value = serde_json::from_str(stub.stdin().trim()).unwrap();
        assert_eq!(
            given["message"]["content"],
            json!([
                { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo=" } },
                { "type": "text", "text": "<text>\n\n</text>" },
            ])
        );
    }

    #[test]
    fn it_is_run_with_no_tools_and_none_of_the_configurations_extras_in_an_empty_directory() {
        let stub = printing(&answer_lines(&["Hallo", " Welt"], "end_turn"));
        let (answer, seen) = ask(&claude(&stub), &Question::material("Translate it into German.", "hello world"));
        assert_eq!(answer, Ok("Hallo Welt".into()));
        assert_eq!(seen, ["Hallo", "Hallo Welt"]);
        let args = stub.args();
        let system = &args[8];
        assert!(system.ends_with("system.md"), "{args:?}");
        assert_eq!(
            args,
            [
                "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--system-prompt-file", system,
                "--tools", "", "--strict-mcp-config", "--safe-mode", "--disable-slash-commands", "--no-session-persistence", "--permission-mode", "dontAsk", "--permission-prompts", "none", "--effort", "low",
            ]
        );
        // The question is on its standard input, and nowhere on the command line.
        let given: Value = serde_json::from_str(stub.stdin().trim()).unwrap();
        assert_eq!(given, json!({ "type": "user", "session_id": "", "parent_tool_use_id": null, "message": { "role": "user", "content": [{ "type": "text", "text": "<text>\nhello world\n</text>" }] } }));
        assert!(!args.iter().any(|arg| arg.contains("hello world")));
        // It ran in an empty directory, beside the system prompt, and both are gone.
        let cwd = stub.kept("cwd");
        assert!(cwd.trim().ends_with("/work"), "{cwd}");
        assert!(!Path::new(system).exists() && !Path::new(cwd.trim()).exists());
    }

    #[test]
    fn a_conversation_is_written_out_and_the_model_named() {
        let stub = printing(&answer_lines(&["3."], "end_turn"));
        let question = Question {
            system: "Count.".into(),
            prompt: "Three?".into(),
            history: Some(vec![Turn { role: "user".into(), content: "One?".into() }, Turn { role: "assistant".into(), content: "1.".into() }]),
            ..Question::default()
        };
        let model = Model::Cli(Cli::new(Tool::ClaudeCode, &stub.path("claude"), "sonnet"));
        assert_eq!(ask(&model, &question).0, Ok("3.".into()));
        assert_eq!(stub.args()[21..], ["--model", "sonnet"]);
        let given: Value = serde_json::from_str(stub.stdin().trim()).unwrap();
        let text = given["message"]["content"][0]["text"].as_str().unwrap();
        assert!(text.starts_with("The conversation so far:\n\n<conversation>\n<user>\nOne?\n</user>\n<assistant>\n1.\n</assistant>\n</conversation>") && text.ends_with("\n\nThree?"), "{text}");
    }

    #[test]
    fn the_answer_is_not_waited_past() {
        // After the answer, Claude Code summarizes the turn for seconds.
        let mut lines = answer_lines(&["Done"], "end_turn");
        lines.truncate(lines.len() - 1);
        let stub = printing(&lines).then("sleep 30");
        let started = std::time::Instant::now();
        assert_eq!(ask(&claude(&stub), &Question::material("", "x")).0, Ok("Done".into()));
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn what_goes_wrong_is_said_in_words_for_the_user() {
        let ask_one = |lines: Vec<String>, more: &str| {
            let stub = printing(&lines).then(more);
            ask(&claude(&stub), &Question::material("", "x")).0
        };
        // Signed out: Claude Code answers in the answer's place, and says it failed.
        let signed_out = vec![
            json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "Not logged in · Please run /login" }] }, "error": "authentication_failed" }).to_string(),
            json!({ "type": "result", "subtype": "success", "is_error": true, "result": "Not logged in · Please run /login" }).to_string(),
        ];
        assert_eq!(ask_one(signed_out, ""), Err(ModelError::CliNotSignedIn(Tool::ClaudeCode)));
        let api = vec![json!({ "type": "result", "subtype": "success", "is_error": true, "result": "API Error: 529 Overloaded" }).to_string()];
        assert_eq!(ask_one(api, ""), Err(ModelError::CliFailed { tool: Tool::ClaudeCode, message: "API Error: 529 Overloaded".into() }));
        let turns = vec![json!({ "type": "result", "subtype": "error_max_turns", "is_error": true, "errors": ["Reached maximum number of turns"] }).to_string()];
        assert_eq!(ask_one(turns, ""), Err(ModelError::CliFailed { tool: Tool::ClaudeCode, message: "Reached maximum number of turns".into() }));
        assert_eq!(ask_one(answer_lines(&["I can't"], "refusal"), ""), Err(ModelError::Declined));
        assert_eq!(ask_one(answer_lines(&[" "], "end_turn"), ""), Err(ModelError::CliSaidNothing(Tool::ClaudeCode)));
        // It ended with nothing on its standard output.
        assert_eq!(ask_one(vec![], "echo 'error: unknown option --safe-mode' >&2; exit 1"), Err(ModelError::CliFailed { tool: Tool::ClaudeCode, message: "error: unknown option --safe-mode".into() }));
        assert_eq!(ask_one(vec![], "exit 0"), Err(ModelError::CliSaidNothing(Tool::ClaudeCode)));
        // Not there at all.
        let missing = Model::Cli(Cli::new(Tool::ClaudeCode, "/nonexistent/claude", ""));
        assert_eq!(ask(&missing, &Question::material("", "x")).0, Err(ModelError::CliNotFound { tool: Tool::ClaudeCode, at: "/nonexistent/claude".into() }));
    }

    #[cfg(unix)]
    #[test]
    fn calling_an_answer_off_ends_claude_code_and_what_it_started() {
        let beginning = stream(json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "Once" } }));
        super::super::cli::tests::calling_off_ends_the_tool_and_its_children(Tool::ClaudeCode, "claude", &beginning);
    }
}
