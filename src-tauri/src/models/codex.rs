//! OpenAI's models through Codex (`codex`), signed in with the user's
//! ChatGPT account or key: one of the adapters of `models`, through `cli.rs`.
//!
//! It is run once per question, as Codex 0.166's `codex exec --help` says:
//!
//! ```text
//! codex exec [--image <scratch>/image-1.png]… --json --ephemeral
//!       --skip-git-repo-check --ignore-user-config --ignore-rules --color never
//!       -C <scratch>/work -s read-only
//!       -c approval_policy='never' -c model_instructions_file='<scratch>/system.md'
//!       -c project_doc_max_bytes=0 -c web_search='disabled'
//!       -c include_permissions_instructions=false -c include_environment_context=false
//!       -c include_apps_instructions=false -c skills.include_instructions=false
//!       -c model_reasoning_effort='low'
//!       --disable shell_tool --disable unified_exec … [-m <model>]
//! ```
//!
//! - `exec` answers once and exits, reading the question from standard
//!   input when none is given on the command line; `--json` writes its
//!   events as JSON lines. Codex writes no part of a message before all of
//!   it: the answer comes whole (`item.completed`), not as it is written.
//! - `--image` attaches a picture to the question (written to a file).
//! - `--ephemeral`: no session file is kept.
//! - `--ignore-user-config`: `~/.codex/config.toml` is not read, so none
//!   of its MCP servers, plugins, profiles or hooks come along (its model
//!   neither: the setting names one, else Codex's default answers);
//!   `--ignore-rules` leaves out the execpolicy rules. The sign-in is still
//!   Codex's own.
//! - `-s read-only` with `approval_policy='never'`: nothing it could still
//!   run may write, and nothing waits for an approval nobody would give.
//! - `model_instructions_file` replaces Codex's own instructions (a coding
//!   agent's) with the system prompt; `project_doc_max_bytes=0` reads no
//!   AGENTS.md; the `include_*` and `skills.include_instructions` keys
//!   leave out what Codex would tell the model of its sandbox, this
//!   computer, its apps and the user's skills.
//! - `--disable`: every feature that is a tool or brings one (the shell,
//!   pictures from files, the browser, computer use, web search, apps,
//!   plugins, sub-agents…).
//!
//! What could not be turned off: Codex still offers the model `apply_patch`
//! (which the read-only sandbox refuses), the collaboration tools of its
//! newer models (sub-agents run with the same settings), and a JavaScript
//! `exec` that has no file system or network of its own and reaches only
//! those tools.

use std::ffi::OsString;
use std::path::Path;

use base64::Engine;
use serde_json::Value;

use super::cli::{self, Cli, Scratch};
use super::{ModelError, Question};

/// The features that are tools, or bring tools, the model is not to have.
const DISABLED: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "apps",
    "plugins",
    "multi_agent",
    "view_image",
    "image_generation",
    "browser_use",
    "browser_use_external",
    "computer_use",
    "in_app_browser",
    "hooks",
    "goals",
    "skill_search",
    "tool_suggest",
    "sleep_tool",
    "remote_plugin",
];

pub(super) async fn answer(cli: &Cli, program: &Path, scratch: &Scratch, question: &Question, on_text: &mut (dyn FnMut(&str) + Send)) -> Result<String, ModelError> {
    let failed = |e: std::io::Error| cli.failed(e.to_string());
    let system = scratch.file("system.md", cli::system(question).as_bytes()).map_err(failed)?;
    let mut images = Vec::new();
    for (number, image) in question.images.iter().enumerate() {
        let data = base64::engine::general_purpose::STANDARD.decode(image.data.trim()).map_err(|e| cli.failed(e.to_string()))?;
        let ending = image.media_type.strip_prefix("image/").unwrap_or("png");
        images.push(scratch.file(&format!("image-{}.{ending}", number + 1), &data).map_err(failed)?);
    }
    let args = arguments(&scratch.work(), &system, &images, &cli.model);

    let mut reading = Reading::default();
    let ran = cli::run(cli, program, scratch, args, cli::message(question).into_bytes(), |line| reading.line(line, on_text)).await?;
    if let Some(said) = reading.failed.filter(|_| !reading.completed) {
        return Err(cli.failure(&said));
    }
    match reading.text.trim() {
        "" if reading.completed => Err(ModelError::CliSaidNothing(cli.tool)),
        "" => Err(cli.ended_without_answer(&ran)),
        answer => Ok(answer.to_owned()),
    }
}

/// The command line, but for the program.
fn arguments(work: &Path, system: &Path, images: &[std::path::PathBuf], model: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["exec".into()];
    // First: an option that takes several values is ended by the next option.
    for image in images {
        args.extend(["--image".into(), image.into()]);
    }
    args.extend(["--json", "--ephemeral", "--skip-git-repo-check", "--ignore-user-config", "--ignore-rules", "--color", "never", "-C"].map(OsString::from));
    args.push(work.into());
    args.extend(["-s", "read-only"].map(OsString::from));
    let set = [
        "approval_policy='never'".to_owned(),
        format!("model_instructions_file={}", cli::toml_string(&system.to_string_lossy())),
        "project_doc_max_bytes=0".into(),
        "web_search='disabled'".into(),
        "include_permissions_instructions=false".into(),
        "include_environment_context=false".into(),
        "include_apps_instructions=false".into(),
        "skills.include_instructions=false".into(),
        // Heyo asks for short rewriting and explaining; answer quickly.
        "model_reasoning_effort='low'".into(),
    ];
    for value in set {
        args.extend(["-c".into(), value.into()]);
    }
    for feature in DISABLED {
        args.extend(["--disable".into(), (*feature).into()]);
    }
    if !model.is_empty() {
        args.extend(["-m".into(), model.into()]);
    }
    args
}

/// What has been read of the events so far.
#[derive(Default)]
struct Reading {
    /// The last message it wrote: the answer, once the turn is over.
    text: String,
    completed: bool,
    failed: Option<String>,
}

impl Reading {
    /// Takes in one line of output; false once there is nothing more to wait for.
    fn line(&mut self, line: &str, on_text: &mut (dyn FnMut(&str) + Send)) -> bool {
        let Ok(event) = serde_json::from_str::<Value>(line) else { return true };
        match event["type"].as_str().unwrap_or_default() {
            "item.updated" | "item.completed" if event["item"]["type"] == "agent_message" => {
                let text = event["item"]["text"].as_str().unwrap_or_default();
                if !text.trim().is_empty() && text != self.text {
                    self.text = text.to_owned();
                    on_text(&self.text);
                }
            }
            "turn.completed" => {
                self.completed = true;
                return false;
            }
            "turn.failed" => {
                self.failed = Some(event["error"]["message"].as_str().unwrap_or("turn failed").to_owned());
                return false;
            }
            // Also said of a connection it tries again; the end says whether it gave up.
            "error" => self.failed = event["message"].as_str().map(str::to_owned),
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
    use serde_json::json;

    /// What Codex 0.166.0 wrote when asked, with these very arguments, for
    /// one word on the colour of a red picture.
    const REAL: &str = include_str!("fixtures/codex.jsonl");

    fn codex(stub: &Stub, model: &str) -> Model {
        Model::Cli(Cli::new(Tool::Codex, &stub.path("codex"), model))
    }

    fn printing(lines: &str) -> Stub {
        let stub = Stub::new("codex", "");
        std::fs::write(stub.dir.join("out"), lines).unwrap();
        // It keeps the pictures it was given, which go with the scratch directory.
        stub.then("for arg in \"$@\"; do case \"$arg\" in *image-*) cp \"$arg\" \"$here/\";; esac; done\necho 'Reading prompt from stdin...' >&2\ncat \"$here/out\"")
    }

    fn events(messages: &[&str]) -> String {
        let mut lines = vec![json!({ "type": "thread.started", "thread_id": "t" }), json!({ "type": "turn.started" })];
        for (number, text) in messages.iter().enumerate() {
            lines.push(json!({ "type": "item.completed", "item": { "id": format!("item_{number}"), "type": "agent_message", "text": text } }));
        }
        lines.push(json!({ "type": "turn.completed", "usage": { "input_tokens": 10, "output_tokens": 1 } }));
        lines.iter().map(|line| format!("{line}\n")).collect()
    }

    #[test]
    fn its_real_output_is_read_and_the_picture_is_given_as_a_file() {
        let stub = printing(REAL);
        let question = Question { images: vec![PromptImage { media_type: "image/png".into(), data: "iVBORw0KGgo=".into() }], ..Question::material("Name the colour.", "") };
        let (answer, seen) = ask(&codex(&stub, ""), &question);
        assert_eq!(answer, Ok("Red".into()));
        assert_eq!(seen, ["Red"]);
        let args = stub.args();
        assert_eq!(args[..2], ["exec".to_owned(), "--image".to_owned()]);
        assert!(args[2].ends_with("image-1.png"), "{args:?}");
        assert_eq!(std::fs::read(stub.dir.join("image-1.png")).unwrap(), b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn it_is_run_without_tools_or_the_users_configuration_in_an_empty_directory() {
        let stub = printing(&events(&["Hallo Welt"]));
        let (answer, _) = ask(&codex(&stub, ""), &Question::material("Translate it into German.", "hello world"));
        assert_eq!(answer, Ok("Hallo Welt".into()));
        let args = stub.args();
        let (work, system) = (&args[9], args[15].strip_prefix("model_instructions_file='").unwrap().trim_end_matches('\''));
        assert!(work.ends_with("/work") && system.ends_with("/system.md"), "{args:?}");
        let mut expected: Vec<String> = [
            "exec", "--json", "--ephemeral", "--skip-git-repo-check", "--ignore-user-config", "--ignore-rules", "--color", "never", "-C", work, "-s", "read-only",
            "-c", "approval_policy='never'", "-c", &args[15], "-c", "project_doc_max_bytes=0", "-c", "web_search='disabled'",
            "-c", "include_permissions_instructions=false", "-c", "include_environment_context=false", "-c", "include_apps_instructions=false",
            "-c", "skills.include_instructions=false", "-c", "model_reasoning_effort='low'",
        ]
        .map(str::to_owned)
        .to_vec();
        expected.extend(DISABLED.iter().flat_map(|feature| ["--disable".to_owned(), feature.to_string()]));
        assert_eq!(args, expected);
        assert_eq!(stub.kept("cwd").trim(), work);
        // The question on its standard input only.
        assert_eq!(stub.stdin(), "<text>\nhello world\n</text>");
        assert!(!args.iter().any(|arg| arg.contains("hello world")));
        assert!(!Path::new(work).exists() && !Path::new(system).exists());
    }

    #[test]
    fn a_conversation_is_written_out_the_model_named_and_the_last_message_is_the_answer() {
        let stub = printing(&events(&["Let me count.", "3."]));
        let question = Question {
            prompt: "Three?".into(),
            history: Some(vec![Turn { role: "user".into(), content: "One?".into() }, Turn { role: "assistant".into(), content: "1.".into() }]),
            ..Question::default()
        };
        let (answer, seen) = ask(&codex(&stub, "gpt-5.6-sol"), &question);
        assert_eq!(answer, Ok("3.".into()));
        assert_eq!(seen, ["Let me count.", "3."]);
        assert_eq!(stub.args()[stub.args().len() - 2..], ["-m", "gpt-5.6-sol"]);
        assert!(stub.stdin().starts_with("The conversation so far:\n\n<conversation>\n<user>\nOne?\n</user>"));
    }

    #[test]
    fn what_goes_wrong_is_said_in_words_for_the_user() {
        let ask_one = |out: &str, more: &str| {
            let stub = printing(out).then(more);
            ask(&codex(&stub, ""), &Question::material("", "x")).0
        };
        let start = "{\"type\":\"thread.started\",\"thread_id\":\"t\"}\n{\"type\":\"turn.started\"}\n";
        let signed_out = format!("{start}{}\n{}\n", json!({ "type": "error", "message": "unexpected status 401 Unauthorized: Missing bearer or basic authentication in header" }), json!({ "type": "turn.failed", "error": { "message": "unexpected status 401 Unauthorized: Missing bearer or basic authentication in header" } }));
        assert_eq!(ask_one(&signed_out, ""), Err(ModelError::CliNotSignedIn(Tool::Codex)));
        let unknown = format!("{start}{}\n", json!({ "type": "turn.failed", "error": { "message": "The 'gpt-9' model does not exist." } }));
        assert_eq!(ask_one(&unknown, ""), Err(ModelError::CliFailed { tool: Tool::Codex, message: "The 'gpt-9' model does not exist.".into() }));
        // A connection tried again, then answered: the answer stands.
        let retried = format!("{start}{}\n{}", json!({ "type": "error", "message": "Reconnecting... 1/5" }), events(&["fine"]));
        assert_eq!(ask_one(&retried, ""), Ok("fine".into()));
        assert_eq!(ask_one(&events(&[]), ""), Err(ModelError::CliSaidNothing(Tool::Codex)));
        assert_eq!(ask_one("", "echo 'Error: Not logged in' >&2; exit 1"), Err(ModelError::CliNotSignedIn(Tool::Codex)));
        assert_eq!(ask_one("", "echo 'error: unexpected argument --ignore-rules found' >&2; exit 2"), Err(ModelError::CliFailed { tool: Tool::Codex, message: "error: unexpected argument --ignore-rules found".into() }));
        let missing = Model::Cli(Cli::new(Tool::Codex, "/nonexistent/codex", ""));
        assert_eq!(ask(&missing, &Question::material("", "x")).0, Err(ModelError::CliNotFound { tool: Tool::Codex, at: "/nonexistent/codex".into() }));
    }

    #[cfg(unix)]
    #[test]
    fn calling_an_answer_off_ends_codex_and_what_it_started() {
        let beginning = json!({ "type": "item.completed", "item": { "id": "item_0", "type": "agent_message", "text": "Once" } }).to_string();
        super::super::cli::tests::calling_off_ends_the_tool_and_its_children(Tool::Codex, "codex", &beginning);
    }
}
