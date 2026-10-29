//! Claude, through Anthropic's Messages API over HTTPS: one of the two
//! adapters of `models`. There is no SDK for Rust; the request and the
//! events of its answer are the API's own (streamed as server-sent events).

use serde_json::{json, Value};

use super::{ModelError, Question};

/// The Claude model that answers when Claude is the provider.
pub const CLAUDE_MODEL: &str = "claude-opus-5-5";

const API: &str = "https://api.anthropic.com";
const VERSION: &str = "2023-06-01";
/// If a safety classifier declines, the request is answered by the model
/// Anthropic recommends for it instead (`fallbacks: "default"`).
const FALLBACK: &str = "server-side-fallback-2026-07-01";
/// What an `ant auth login` token is sent with.
const OAUTH: &str = "oauth-2025-04-20";

/// What Claude is asked with.
#[derive(Clone, PartialEq)]
enum Credential {
    /// An API key (`x-api-key`).
    Key(String),
    /// A short-lived token of the user's own sign-in (`ant auth login`).
    Token(String),
}

#[derive(Clone, PartialEq)]
pub struct Claude {
    /// The key from the settings; empty: found where Anthropic's own tools look.
    key: String,
    /// Where the API is, and (for tests) what was found in place of the
    /// environment's credentials.
    #[cfg(test)]
    test: Option<(String, Option<Credential>)>,
}

/// The key stays out of anything printed.
impl std::fmt::Debug for Claude {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Claude").field("key", &if self.key.is_empty() { "none" } else { "set" }).finish()
    }
}

impl Claude {
    pub fn new(key: &str) -> Claude {
        Claude {
            key: key.trim().to_owned(),
            #[cfg(test)]
            test: None,
        }
    }

    fn base(&self) -> String {
        #[cfg(test)]
        if let Some((base, _)) = &self.test {
            return base.clone();
        }
        std::env::var("ANTHROPIC_BASE_URL").ok().filter(|url| !url.trim().is_empty()).unwrap_or_else(|| API.into()).trim_end_matches('/').to_owned()
    }

    /// The key from the settings; else, as Anthropic's SDKs find one,
    /// `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, or the profile of
    /// `ant auth login` (through `ant` itself, which refreshes its token).
    async fn credential(&self) -> Option<Credential> {
        if !self.key.is_empty() {
            return Some(Credential::Key(self.key.clone()));
        }
        #[cfg(test)]
        if let Some((_, found)) = &self.test {
            return found.clone();
        }
        let set = |name| std::env::var(name).ok().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        if let Some(key) = set("ANTHROPIC_API_KEY") {
            return Some(Credential::Key(key));
        }
        if let Some(token) = set("ANTHROPIC_AUTH_TOKEN") {
            return Some(Credential::Token(token));
        }
        let signed_in = tauri::async_runtime::spawn_blocking(|| {
            let output = std::process::Command::new("ant").args(["auth", "print-credentials", "--access-token"]).stdin(std::process::Stdio::null()).output().ok()?;
            let token = String::from_utf8(output.stdout).ok()?.trim().to_owned();
            (output.status.success() && !token.is_empty() && !token.contains(char::is_whitespace)).then_some(token)
        });
        signed_in.await.ok().flatten().map(Credential::Token)
    }

    pub async fn answer(&self, question: &Question, on_text: &mut (dyn FnMut(&str) + Send)) -> Result<String, ModelError> {
        let credential = self.credential().await.ok_or(ModelError::NotSetUp)?;
        let request = super::client(false).post(format!("{}/v1/messages", self.base())).header("content-type", "application/json").header("anthropic-version", VERSION);
        let request = match &credential {
            Credential::Key(key) => request.header("x-api-key", key).header("anthropic-beta", FALLBACK),
            Credential::Token(token) => request.bearer_auth(token).header("anthropic-beta", format!("{FALLBACK},{OAUTH}")),
        };
        let response = request.body(body(question).to_string()).send().await.map_err(|_| ModelError::ClaudeUnreachable)?;
        let status = response.status();
        if !status.is_success() {
            let said: Value = serde_json::from_str(&response.text().await.unwrap_or_default()).unwrap_or_default();
            let message = said["error"]["message"].as_str().unwrap_or(status.canonical_reason().unwrap_or_default()).to_owned();
            return Err(match status.as_u16() {
                401 => ModelError::InvalidKey,
                429 => ModelError::RateLimited,
                code => ModelError::Claude { status: code.to_string(), message },
            });
        }

        let (mut text, mut stop, mut failed) = (String::new(), None, None);
        let read = super::events(response, |data| {
            let Ok(event) = serde_json::from_str::<Value>(data) else { return true };
            match event["type"].as_str().unwrap_or_default() {
                // The text of every text block, in order: after a fallback
                // mid-answer, the other model goes on from what was written.
                "content_block_delta" if event["delta"]["type"] == "text_delta" => {
                    text.push_str(event["delta"]["text"].as_str().unwrap_or_default());
                    on_text(&text);
                }
                "message_delta" => {
                    if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                        stop = Some(reason.to_owned());
                    }
                }
                "message_stop" => return false,
                "error" => {
                    let error = &event["error"];
                    failed = Some(match error["type"].as_str() {
                        Some("rate_limit_error") => ModelError::RateLimited,
                        kind => ModelError::Claude { status: kind.unwrap_or("error").to_owned(), message: error["message"].as_str().unwrap_or_default().to_owned() },
                    });
                    return false;
                }
                _ => {}
            }
            true
        })
        .await;
        read.map_err(|_| ModelError::ClaudeUnreachable)?;
        if let Some(failure) = failed {
            return Err(failure);
        }
        // The fallback declined too, or there was none to ask.
        if stop.as_deref() == Some("refusal") {
            return Err(ModelError::Declined);
        }
        match text.trim() {
            "" => Err(ModelError::ClaudeSaidNothing),
            answer => Ok(answer.to_owned()),
        }
    }
}

/// The request for a question.
fn body(question: &Question) -> Value {
    // Text to work on is fenced, so that it reads as material and not as a
    // message; a conversation is sent as it is.
    let prompt = match question.history {
        Some(_) => question.prompt.clone(),
        None => format!("<text>\n{}\n</text>", question.prompt),
    };
    let content = match question.images.as_slice() {
        [] => json!(prompt),
        images => {
            let mut blocks: Vec<Value> = images.iter().map(|image| json!({ "type": "image", "source": { "type": "base64", "media_type": image.media_type, "data": image.data } })).collect();
            blocks.push(json!({ "type": "text", "text": prompt }));
            json!(blocks)
        }
    };
    let mut messages: Vec<Value> = question.history.iter().flatten().map(|turn| json!({ "role": turn.role, "content": turn.content })).collect();
    messages.push(json!({ "role": "user", "content": content }));
    // No temperature: this model takes none.
    json!({
        "model": CLAUDE_MODEL,
        "max_tokens": 64000,
        // Heyo asks for short rewriting and explaining; answer quickly.
        "output_config": { "effort": "low" },
        "fallbacks": "default",
        "system": question.system,
        "messages": messages,
        "stream": true,
    })
}

#[cfg(test)]
mod tests {
    use super::super::mock::{block_on, Reply, Server};
    use super::super::{Model, PromptImage, Turn};
    use super::*;

    impl Claude {
        /// Claude at `base`, with `found` where the environment's credentials would be.
        fn at(base: String, key: &str, found: Option<Credential>) -> Model {
            Model::Claude(Claude { key: key.into(), test: Some((base, found)) })
        }
    }

    /// The events of an answer: `pieces` of text, then why it stopped.
    fn sse(pieces: &[&str], stop: &str) -> String {
        let event = |name: &str, data: Value| format!("event: {name}\ndata: {data}\n\n");
        let mut out = event("message_start", json!({ "type": "message_start", "message": { "id": "msg_1", "type": "message", "model": CLAUDE_MODEL, "content": [] } }));
        out += &event("content_block_start", json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "thinking", "thinking": "" } }));
        out += &event("content_block_delta", json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "thinking_delta", "thinking": "" } }));
        out += &event("content_block_stop", json!({ "type": "content_block_stop", "index": 0 }));
        out += &event("ping", json!({ "type": "ping" }));
        out += &event("content_block_start", json!({ "type": "content_block_start", "index": 1, "content_block": { "type": "text", "text": "" } }));
        for piece in pieces {
            out += &event("content_block_delta", json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "text_delta", "text": piece } }));
        }
        out += &event("content_block_stop", json!({ "type": "content_block_stop", "index": 1 }));
        out += &event("message_delta", json!({ "type": "message_delta", "delta": { "stop_reason": stop }, "usage": { "output_tokens": 12 } }));
        out += &event("message_stop", json!({ "type": "message_stop" }));
        out
    }

    fn ask(model: &Model, question: &Question) -> (Result<String, ModelError>, Vec<String>) {
        let mut seen = Vec::new();
        let answer = block_on(model.answer(question, &mut |text| seen.push(text.to_owned())));
        (answer, seen)
    }

    #[test]
    fn text_to_work_on_is_fenced_and_the_answer_streams_as_it_is_written() {
        let server = Server::start(|_| (200, sse(&["Hallo", " Welt", "\n"], "end_turn")));
        let claude = Claude::at(server.url(""), "sk-test", None);
        let (answer, seen) = ask(&claude, &Question::material("Translate it into German.", "hello world"));
        assert_eq!(answer, Ok("Hallo Welt".into()));
        assert_eq!(seen, ["Hallo", "Hallo Welt", "Hallo Welt\n"]);

        let asked = &server.asked()[0];
        assert_eq!((asked.method.as_str(), asked.path.as_str()), ("POST", "/v1/messages"));
        assert_eq!(asked.header("x-api-key"), Some("sk-test"));
        assert_eq!(asked.header("anthropic-version"), Some("2023-06-01"));
        assert_eq!(asked.header("anthropic-beta"), Some("server-side-fallback-2026-07-01"));
        assert_eq!(asked.header("authorization"), None);
        let body: Value = serde_json::from_str(&asked.body).unwrap();
        assert_eq!(
            body,
            json!({
                "model": "claude-opus-5-5",
                "max_tokens": 64000,
                "output_config": { "effort": "low" },
                "fallbacks": "default",
                "system": "Translate it into German.",
                "messages": [{ "role": "user", "content": "<text>\nhello world\n</text>" }],
                "stream": true,
            })
        );
    }

    #[test]
    fn a_conversation_goes_as_it_is_with_its_pictures_before_the_question() {
        let server = Server::start(|_| (200, sse(&["A cat."], "end_turn")));
        let question = Question {
            system: "Be brief.".into(),
            prompt: "And this?".into(),
            history: Some(vec![Turn { role: "user".into(), content: "Hi".into() }, Turn { role: "assistant".into(), content: "Hello.".into() }]),
            images: vec![PromptImage { media_type: "image/png".into(), data: "iVBORw0KGgo=".into() }],
            // Claude is asked without one.
            temperature: Some(1.0),
        };
        assert_eq!(ask(&Claude::at(server.url(""), "sk-test", None), &question).0, Ok("A cat.".into()));
        let body: Value = serde_json::from_str(&server.asked()[0].body).unwrap();
        assert_eq!(
            body["messages"],
            json!([
                { "role": "user", "content": "Hi" },
                { "role": "assistant", "content": "Hello." },
                { "role": "user", "content": [
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo=" } },
                    { "type": "text", "text": "And this?" },
                ] },
            ])
        );
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn credentials_are_the_settings_key_else_what_anthropics_tools_find() {
        let server = Server::start(|_| (200, sse(&["ok"], "end_turn")));
        // A sign-in's token goes as a bearer, with the beta it needs.
        assert_eq!(ask(&Claude::at(server.url(""), "", Some(Credential::Token("tok".into()))), &Question::material("", "x")).0, Ok("ok".into()));
        let asked = &server.asked()[0];
        assert_eq!((asked.header("authorization"), asked.header("x-api-key")), (Some("Bearer tok"), None));
        assert_eq!(asked.header("anthropic-beta"), Some("server-side-fallback-2026-07-01,oauth-2025-04-20"));
        // The key of the settings comes first.
        assert_eq!(ask(&Claude::at(server.url(""), "sk-mine", Some(Credential::Token("tok".into()))), &Question::material("", "x")).0, Ok("ok".into()));
        assert_eq!(server.asked()[1].header("x-api-key"), Some("sk-mine"));
        // With none anywhere, nothing is sent and the user is told how to set one up.
        let (answer, _) = ask(&Claude::at(server.url(""), "", None), &Question::material("", "x"));
        assert_eq!(answer, Err(ModelError::NotSetUp));
        assert_eq!(server.asked().len(), 2);
    }

    #[test]
    fn a_declined_or_empty_answer_is_a_failure_in_words() {
        let declined = Server::start(|_| (200, sse(&["I can"], "refusal")));
        assert_eq!(ask(&Claude::at(declined.url(""), "k", None), &Question::material("", "x")).0, Err(ModelError::Declined));
        let empty = Server::start(|_| (200, sse(&[" "], "end_turn")));
        assert_eq!(ask(&Claude::at(empty.url(""), "k", None), &Question::material("", "x")).0, Err(ModelError::ClaudeSaidNothing));
    }

    #[test]
    fn the_apis_errors_are_worded_for_the_user() {
        let error = |status: u16, kind: &str, message: &str| {
            let (kind, message) = (kind.to_owned(), message.to_owned());
            let server = Server::start(move |_| (status, json!({ "type": "error", "error": { "type": kind, "message": message } }).to_string()));
            ask(&Claude::at(server.url(""), "k", None), &Question::material("", "x")).0.unwrap_err()
        };
        assert_eq!(error(401, "authentication_error", "invalid x-api-key"), ModelError::InvalidKey);
        assert_eq!(error(429, "rate_limit_error", "slow down"), ModelError::RateLimited);
        assert_eq!(error(400, "invalid_request_error", "max_tokens: too big"), ModelError::Claude { status: "400".into(), message: "max_tokens: too big".into() });
        assert_eq!(error(529, "overloaded_error", "Overloaded").to_string(), "Claude API error 529: Overloaded");

        // In the middle of an answer.
        let breaking = Server::start(|_| (200, format!("event: error\ndata: {}\n\n", json!({ "type": "error", "error": { "type": "overloaded_error", "message": "Overloaded" } }))));
        assert_eq!(ask(&Claude::at(breaking.url(""), "k", None), &Question::material("", "x")).0, Err(ModelError::Claude { status: "overloaded_error".into(), message: "Overloaded".into() }));

        assert_eq!(ask(&Claude::at("http://127.0.0.1:9".into(), "k", None), &Question::material("", "x")).0, Err(ModelError::ClaudeUnreachable));
    }

    #[test]
    fn an_answer_called_off_closes_the_connection() {
        let start = format!("event: content_block_delta\ndata: {}\n\n", json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "Once" } }));
        let server = Server::start(move |_| Reply::Stalls(start.clone()));
        let claude = Claude::at(server.url(""), "k", None);
        let (tx, rx) = std::sync::mpsc::channel();
        let asking = tauri::async_runtime::spawn(async move {
            let mut tell = move |text: &str| drop(tx.send(text.to_owned()));
            claude.answer(&Question::material("", "x"), &mut tell).await
        });
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), "Once");
        asking.abort();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while server.hung_up() == 0 {
            assert!(std::time::Instant::now() < deadline, "the server was not hung up on");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}
