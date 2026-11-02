//! What every text adapter is held to, run against each: the mock servers
//! for the adapters over HTTP (`mock.rs`), stand-ins of the command-line
//! tools (`cli::tests::Stub`), and the scripted fake (`fake.rs`).
//!
//! - Streaming: the text so far is told as it grows, and the answer is
//!   the last of it, trimmed.
//! - Cancellation: dropping a step lets go of what it asked (the
//!   connection is closed, the tool ended).
//! - Failures: each class the adapter can fail with comes out as that
//!   class, in words.
//! - Tool calls, for the adapters whose protocol has them: a call comes
//!   back, and its result goes on to the answer.

use std::any::Any;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::fake::Scripted;
use super::mock::{block_on, Reply as Answer, Server};
use super::registry::Can;
use super::text::{Conversation, Part, Reply, ToolCall, ToolResult, ToolSpec, Turn};
use super::{claude, openai, Model, ModelError, Question};

/// What keeps a subject's server or stand-in there while it is asked.
type Kept = Box<dyn Any>;

/// A step begun and waiting, with whether it has let go of what it asked.
type Stalled = (Model, Box<dyn Fn() -> bool>, Kept);

/// The classes of failure, by name.
fn class(error: &ModelError) -> &'static str {
    match error {
        ModelError::NotSetUp(_) => "not set up",
        ModelError::Auth(_) => "auth",
        ModelError::RateLimited(_) => "rate limited",
        ModelError::Unreachable(_) => "unreachable",
        ModelError::Refused(_) => "refused",
        ModelError::Failed(_) => "failed",
        ModelError::Unusable(_) => "unusable",
    }
}

/// An adapter, as each contract needs it set up.
struct Subject {
    name: &'static str,
    /// It answering `pieces` of text.
    answering: fn(&[&str]) -> (Model, Kept),
    /// It having begun an answer ("Once") and then waiting; with whether
    /// it has let go of what it asked.
    stalling: fn() -> Stalled,
    /// It failing with each class it can.
    failing: fn() -> Vec<(&'static str, Model, Kept)>,
}

fn subjects() -> Vec<Subject> {
    let mut all = vec![claude_api(), openai_compatible(), fake()];
    #[cfg(unix)]
    all.extend([claude_code(), codex()]);
    all
}

fn ask(model: &Model, question: &Question) -> (Result<String, ModelError>, Vec<String>) {
    let mut seen = Vec::new();
    let answer = block_on(model.answer(question, &mut |text| seen.push(text.to_owned())));
    (answer, seen)
}

#[test]
fn every_adapter_tells_its_answer_as_it_grows() {
    for subject in subjects() {
        let (model, _kept) = (subject.answering)(&["Hallo", " Welt", "\n"]);
        let (answer, seen) = ask(&model, &Question::material("Translate.", "hello world"));
        assert_eq!(answer, Ok("Hallo Welt".into()), "{}", subject.name);
        assert!(!seen.is_empty() && seen.iter().all(|text| !text.is_empty()), "{}: {seen:?}", subject.name);
        assert_eq!(seen.last().unwrap().trim(), "Hallo Welt", "{}", subject.name);
        // What streams says more than once.
        if model.can(Can::Streaming) && subject.name != "Codex" {
            assert!(seen.len() > 1, "{}: {seen:?}", subject.name);
        }
    }
}

#[test]
fn every_adapter_lets_go_when_its_step_is_dropped() {
    for subject in subjects() {
        let (model, let_go, _kept) = (subject.stalling)();
        let (tx, rx) = std::sync::mpsc::channel();
        let asking = tauri::async_runtime::spawn(async move {
            let mut tell = move |text: &str| drop(tx.send(text.to_owned()));
            model.answer(&Question::material("", "x"), &mut tell).await
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), "Once", "{}", subject.name);
        asking.abort();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !let_go() {
            assert!(Instant::now() < deadline, "{} did not let go", subject.name);
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
fn every_adapter_fails_in_a_class_and_in_words() {
    for subject in subjects() {
        for (expected, model, _kept) in (subject.failing)() {
            let failure = ask(&model, &Question::material("", "x")).0.expect_err(expected);
            assert_eq!(class(&failure), expected, "{}: {failure:?}", subject.name);
            assert!(!failure.to_string().trim().is_empty(), "{}", subject.name);
        }
    }
}

#[test]
fn an_adapter_says_what_it_can_be_given_and_refuses_what_it_cannot() {
    let picture = Question { images: vec![super::PromptImage { media_type: "image/png".into(), data: "iVBORw0KGgo=".into() }], ..Question::material("", "x") };
    let mut tools = Conversation::of(&Question::material("", "x"));
    tools.tools.push(clock());
    for subject in subjects() {
        let (model, _kept) = (subject.answering)(&["ok"]);
        if !model.can(Can::Images) {
            assert!(ask(&model, &picture).0.is_err(), "{} took a picture", subject.name);
        }
        if !model.can(Can::ToolCalls) {
            assert!(block_on(model.step(&tools, &mut |_| {})).is_err(), "{} took tools", subject.name);
        }
    }
}

// --- Tool calls -------------------------------------------------------------------

fn clock() -> ToolSpec {
    ToolSpec { name: "clock".into(), description: "The time in a zone.".into(), input_schema: json!({ "type": "object", "properties": { "zone": { "type": "string" } }, "required": ["zone"] }) }
}

/// Asks what time it is, has the clock called, gives its result, and
/// returns the conversation and the final answer.
fn round_trip(model: &Model) -> (Conversation, Reply) {
    let mut conversation = Conversation { system: "Be brief.".into(), turns: vec![Turn::User(vec![Part::Text("What time is it?".into())])], tools: vec![clock()], temperature: None };
    let mut seen = Vec::new();
    let first = block_on(model.step(&conversation, &mut |text| seen.push(text.to_owned()))).unwrap();
    assert_eq!(first.text, "Let me look.");
    assert_eq!(seen.last().map(String::as_str), Some("Let me look."));
    assert_eq!(first.calls.len(), 1);
    let call = first.calls[0].clone();
    assert_eq!((call.name.as_str(), &call.input), ("clock", &json!({ "zone": "UTC" })));
    conversation.turns.push(Turn::Assistant(first));
    conversation.turns.push(Turn::Results(vec![ToolResult { call: call.id, content: "12:00".into(), failed: false }]));
    let last = block_on(model.step(&conversation, &mut |_| {})).unwrap();
    (conversation, last)
}

#[test]
fn claude_calls_a_tool_and_goes_on_from_its_result() {
    let event = claude::tests::event;
    let block = |index: usize, start: Value, deltas: Vec<Value>| {
        let mut out = event("content_block_start", json!({ "type": "content_block_start", "index": index, "content_block": start }));
        for delta in deltas {
            out += &event("content_block_delta", json!({ "type": "content_block_delta", "index": index, "delta": delta }));
        }
        out + &event("content_block_stop", json!({ "type": "content_block_stop", "index": index }))
    };
    let calling = [
        event("message_start", json!({ "type": "message_start", "message": { "id": "msg_1", "content": [] } })),
        block(0, json!({ "type": "thinking", "thinking": "" }), vec![json!({ "type": "signature_delta", "signature": "c2ln" })]),
        block(1, json!({ "type": "text", "text": "" }), vec![json!({ "type": "text_delta", "text": "Let me look." })]),
        block(2, json!({ "type": "tool_use", "id": "toolu_1", "name": "clock", "input": {} }), vec![json!({ "type": "input_json_delta", "partial_json": "{\"zone\":" }), json!({ "type": "input_json_delta", "partial_json": " \"UTC\"}" })]),
        event("message_delta", json!({ "type": "message_delta", "delta": { "stop_reason": "tool_use" } })),
        event("message_stop", json!({ "type": "message_stop" })),
    ]
    .concat();
    let server = Server::start(move |asked| if asked.body.contains("tool_result") { (200, claude::tests::sse(&["It is noon."], "end_turn")) } else { (200, calling.clone()) });
    let model = claude::Claude::at(server.url(""), "k", None);
    let (_, last) = round_trip(&model);
    assert_eq!(last, Reply { text: "It is noon.".into(), calls: vec![], replay: last.replay.clone() });

    let asked: Vec<Value> = server.asked().iter().map(|asked| serde_json::from_str(&asked.body).unwrap()).collect();
    let tools = json!([{ "name": "clock", "description": "The time in a zone.", "input_schema": clock().input_schema }]);
    assert_eq!((&asked[0]["tools"], &asked[1]["tools"]), (&tools, &tools));
    assert_eq!(asked[0]["messages"], json!([{ "role": "user", "content": "What time is it?" }]));
    // The turn goes back as the API wrote it, its thinking and all, and the
    // result in a message of the user's.
    assert_eq!(
        asked[1]["messages"],
        json!([
            { "role": "user", "content": "What time is it?" },
            { "role": "assistant", "content": [
                { "type": "thinking", "thinking": "", "signature": "c2ln" },
                { "type": "text", "text": "Let me look." },
                { "type": "tool_use", "id": "toolu_1", "name": "clock", "input": { "zone": "UTC" } },
            ] },
            { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": "toolu_1", "content": "12:00" }] },
        ])
    );
}

#[test]
fn an_openai_compatible_model_calls_a_tool_and_goes_on_from_its_result() {
    let chunk = |delta: Value| format!("data: {}\n\n", json!({ "choices": [{ "index": 0, "delta": delta }] }));
    let calling = [
        chunk(json!({ "role": "assistant", "content": "Let me look." })),
        chunk(json!({ "tool_calls": [{ "index": 0, "id": "call_1", "type": "function", "function": { "name": "clock", "arguments": "" } }] })),
        chunk(json!({ "tool_calls": [{ "index": 0, "function": { "arguments": "{\"zone\":" } }] })),
        chunk(json!({ "tool_calls": [{ "index": 0, "function": { "arguments": " \"UTC\"}" } }] })),
        format!("data: {}\n\ndata: [DONE]\n\n", json!({ "choices": [{ "index": 0, "delta": {}, "finish_reason": "tool_calls" }] })),
    ]
    .concat();
    let server = Server::start(move |asked| if asked.body.contains("\"tool\"") { (200, openai::tests::sse(&["It is noon."])) } else { (200, calling.clone()) });
    let model = Model::local(&server.url("/v1"), "qwen3");
    let (_, last) = round_trip(&model);
    assert_eq!(last, Reply { text: "It is noon.".into(), calls: vec![], replay: None });

    let asked: Vec<Value> = server.asked().iter().map(|asked| serde_json::from_str(&asked.body).unwrap()).collect();
    let tools = json!([{ "type": "function", "function": { "name": "clock", "description": "The time in a zone.", "parameters": clock().input_schema } }]);
    assert_eq!((&asked[0]["tools"], &asked[1]["tools"]), (&tools, &tools));
    // A conversation, not text to work on.
    assert_eq!(asked[0]["temperature"], json!(0.5));
    assert_eq!(
        asked[1]["messages"],
        json!([
            { "role": "system", "content": "Be brief." },
            { "role": "user", "content": "What time is it?" },
            { "role": "assistant", "content": "Let me look.", "tool_calls": [{ "id": "call_1", "type": "function", "function": { "name": "clock", "arguments": "{\"zone\":\"UTC\"}" } }] },
            { "role": "tool", "tool_call_id": "call_1", "content": "12:00" },
        ])
    );
}

#[test]
fn the_fake_answers_by_its_script_and_keeps_what_it_was_asked() {
    let call = ToolCall { id: "1".into(), name: "clock".into(), input: json!({ "zone": "UTC" }) };
    let fake = Scripted::new(vec![Ok(Reply { text: "Let me look.".into(), calls: vec![call], replay: None }), Ok(Reply { text: "It is noon.".into(), ..Reply::default() })], &[Can::Streaming, Can::ToolCalls]);
    let (conversation, last) = round_trip(&Model::of(fake.clone()));
    assert_eq!(last.text, "It is noon.");
    assert_eq!(fake.asked().last(), Some(&conversation));
}

// --- The subjects -----------------------------------------------------------------

fn claude_api() -> Subject {
    Subject {
        name: "Claude API",
        answering: |pieces| {
            let sse = claude::tests::sse(pieces, "end_turn");
            let server = Server::start(move |_| (200, sse.clone()));
            (claude::Claude::at(server.url(""), "k", None), Box::new(server))
        },
        stalling: || {
            let start = claude::tests::event("content_block_delta", json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "Once" } }));
            let server = std::sync::Arc::new(Server::start(move |_| Answer::Stalls(start.clone())));
            let watching = server.clone();
            (claude::Claude::at(server.url(""), "k", None), Box::new(move || watching.hung_up() > 0), Box::new(server))
        },
        failing: || {
            let status = |status: u16| {
                let server = Server::start(move |_| (status, json!({ "type": "error", "error": { "type": "error", "message": "no" } }).to_string()));
                (claude::Claude::at(server.url(""), "k", None), Box::new(server) as Kept)
            };
            let streamed = |sse: String| {
                let server = Server::start(move |_| (200, sse.clone()));
                (claude::Claude::at(server.url(""), "k", None), Box::new(server) as Kept)
            };
            let (auth, auth_kept) = status(401);
            let (limited, limited_kept) = status(429);
            let (failed, failed_kept) = status(500);
            let (refused, refused_kept) = streamed(claude::tests::sse(&["I can"], "refusal"));
            vec![
                ("not set up", claude::Claude::at("http://127.0.0.1:9".into(), "", None), Box::new(())),
                ("auth", auth, auth_kept),
                ("rate limited", limited, limited_kept),
                ("unreachable", claude::Claude::at("http://127.0.0.1:9".into(), "k", None), Box::new(())),
                ("refused", refused, refused_kept),
                ("failed", failed, failed_kept),
            ]
        },
    }
}

fn openai_compatible() -> Subject {
    Subject {
        name: "OpenAI-compatible",
        answering: |pieces| {
            let sse = openai::tests::sse(pieces);
            let server = Server::start(move |_| (200, sse.clone()));
            (Model::local(&server.url("/v1"), "tiny"), Box::new(server))
        },
        stalling: || {
            let start = format!("data: {}\n\n", json!({ "choices": [{ "delta": { "content": "Once" } }] }));
            let server = std::sync::Arc::new(Server::start(move |_| Answer::Stalls(start.clone())));
            let watching = server.clone();
            (Model::local(&server.url("/v1"), "tiny"), Box::new(move || watching.hung_up() > 0), Box::new(server))
        },
        failing: || {
            let status = |status: u16| {
                let server = Server::start(move |_| (status, json!({ "error": { "message": "no" } }).to_string()));
                (Model::local(&server.url("/v1"), "tiny"), Box::new(server) as Kept)
            };
            let listing_none = Server::start(|_| (200, json!({ "data": [] }).to_string()));
            let (auth, auth_kept) = status(401);
            let (limited, limited_kept) = status(429);
            let (failed, failed_kept) = status(500);
            vec![
                ("not set up", Model::local(&listing_none.url("/v1"), ""), Box::new(listing_none)),
                ("auth", auth, auth_kept),
                ("rate limited", limited, limited_kept),
                ("unreachable", Model::local("http://127.0.0.1:9/v1", "tiny"), Box::new(())),
                ("failed", failed, failed_kept),
            ]
        },
    }
}

fn fake() -> Subject {
    Subject {
        name: "Scripted",
        answering: |pieces| (Model::of(Scripted::new(vec![Ok(Reply { text: pieces.concat().trim().to_owned(), ..Reply::default() })], &[Can::Streaming, Can::ToolCalls])), Box::new(())),
        stalling: || {
            // Its stand-in for a connection: a step that never ends, which
            // lets go of this when it is dropped.
            #[derive(Debug)]
            struct Stalls(std::sync::Arc<()>);
            impl super::TextAdapter for Stalls {
                fn can(&self, need: Can) -> bool {
                    need == Can::Streaming
                }
                fn model(&self) -> String {
                    String::new()
                }
                fn step<'a>(&'a self, _: &'a Conversation, on_text: &'a mut (dyn FnMut(&str) + Send)) -> super::text::Step<'a> {
                    let held = self.0.clone();
                    Box::pin(async move {
                        on_text("Once");
                        std::future::pending::<()>().await;
                        drop(held);
                        unreachable!()
                    })
                }
            }
            let held = std::sync::Arc::new(());
            let watching = std::sync::Arc::downgrade(&held);
            let model = Model::of(Stalls(held));
            // While the step is answering, it holds a copy of its own.
            (model, Box::new(move || watching.strong_count() < 2), Box::new(()))
        },
        failing: || {
            let failing = |error: ModelError| Model::of(Scripted::new(vec![Err(error)], &[]));
            vec![
                ("not set up", failing(ModelError::NotSetUp("x".into())), Box::new(())),
                ("refused", failing(ModelError::Refused("x".into())), Box::new(())),
                // Asked past its script.
                ("failed", Model::of(Scripted::new(vec![], &[])), Box::new(())),
            ]
        },
    }
}

#[cfg(unix)]
fn stalled_tool(program: &'static str, model: fn(&str) -> Model, beginning: String) -> Stalled {
    let stub = super::cli::tests::Stub::new(program, &format!("echo $$ > \"$here/tool\"\nprintf '%s\\n' '{beginning}'\nsleep 300"));
    let model = model(&stub.path(program));
    let dir = stub.dir.clone();
    let let_go = move || std::fs::read_to_string(dir.join("tool")).ok().and_then(|pid| pid.trim().parse().ok()).is_some_and(|pid| !super::cli::tests::alive(pid));
    (model, Box::new(let_go), Box::new(stub))
}

#[cfg(unix)]
fn claude_code() -> Subject {
    use super::claude_code::{tests, ClaudeCode};
    fn at(path: &str) -> Model {
        Model::of(ClaudeCode::new(path, ""))
    }
    Subject {
        name: "Claude Code",
        answering: |pieces| {
            let stub = tests::printing(&tests::answer_lines(pieces, "end_turn"));
            (at(&stub.path("claude")), Box::new(stub))
        },
        stalling: || stalled_tool("claude", at, tests::stream(json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "Once" } }))),
        failing: || {
            let printing = |lines: Vec<String>| {
                let stub = tests::printing(&lines);
                (at(&stub.path("claude")), Box::new(stub) as Kept)
            };
            let (auth, auth_kept) = printing(vec![json!({ "type": "result", "subtype": "success", "is_error": true, "result": "Not logged in · Please run /login" }).to_string()]);
            let (refused, refused_kept) = printing(tests::answer_lines(&["I can't"], "refusal"));
            let (failed, failed_kept) = printing(vec![json!({ "type": "result", "subtype": "success", "is_error": true, "result": "API Error: 529 Overloaded" }).to_string()]);
            vec![("not set up", at("/nonexistent/claude"), Box::new(())), ("auth", auth, auth_kept), ("refused", refused, refused_kept), ("failed", failed, failed_kept)]
        },
    }
}

#[cfg(unix)]
fn codex() -> Subject {
    use super::codex::{tests, Codex};
    fn at(path: &str) -> Model {
        Model::of(Codex::new(path, ""))
    }
    Subject {
        name: "Codex",
        answering: |pieces| {
            let stub = tests::printing(&tests::events(&[&pieces.concat()]));
            (at(&stub.path("codex")), Box::new(stub))
        },
        stalling: || stalled_tool("codex", at, json!({ "type": "item.completed", "item": { "id": "item_0", "type": "agent_message", "text": "Once" } }).to_string()),
        failing: || {
            let printing = |out: String| {
                let stub = tests::printing(&out);
                (at(&stub.path("codex")), Box::new(stub) as Kept)
            };
            let turn_failed = |message: &str| format!("{}\n", json!({ "type": "turn.failed", "error": { "message": message } }));
            let (auth, auth_kept) = printing(turn_failed("unexpected status 401 Unauthorized: Missing bearer or basic authentication in header"));
            let (failed, failed_kept) = printing(turn_failed("The 'gpt-9' model does not exist."));
            vec![("not set up", at("/nonexistent/codex"), Box::new(())), ("auth", auth, auth_kept), ("failed", failed, failed_kept)]
        },
    }
}
