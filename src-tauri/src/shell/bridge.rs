//! What the host and a `heyo-ui` process say to each other: JSON, one
//! message a line, over the process's stdin and stdout (`helper`).
//!
//! From the interface:
//!   `{ "id": 7, "invoke": "search", "args": { "query": "se" } }`
//!       a command, by the name and with the arguments the web view gives
//!       `invoke()`; answered with `{ "id": 7, "result": … }` or
//!       `{ "id": 7, "error": "…" }`
//!   `{ "window": "ready" | "focused" | "blurred" | "close" | "shown", … }`
//!       what happened to its window
//!   `{ "pong": 3 }`
//!       the answer to a ping
//!
//! To the interface:
//!   `{ "event": "heyo://shown", "payload": … }`
//!       an event, as the web view's `listen()` hears it
//!   `{ "window": "show" | "hide" | "focus" | "center" | "frame" | "title" | "overlay" | "close", … }`
//!       what to do with its window
//!   `{ "ping": 3 }`
//!
//! `ui/lib/host/` is the other end.

use serde_json::{json, Value};

/// A message from an interface process.
#[derive(Debug, PartialEq)]
pub(crate) enum Incoming {
    Invoke { id: Value, command: String, args: Value },
    Window { what: String, detail: Value },
    Pong(u64),
}

pub(crate) fn read(message: &Value) -> Result<Incoming, String> {
    if let Some(command) = message.get("invoke") {
        let command = command.as_str().ok_or("a command is named by a string")?.to_owned();
        let args = match message.get("args") {
            None | Some(Value::Null) => json!({}),
            Some(args @ Value::Object(_)) => args.clone(),
            Some(_) => return Err(format!("the arguments of {command} are given by name")),
        };
        return Ok(Incoming::Invoke { id: message.get("id").cloned().unwrap_or(Value::Null), command, args });
    }
    if let Some(what) = message.get("window") {
        let what = what.as_str().ok_or("what happened to a window is a word")?.to_owned();
        return Ok(Incoming::Window { what, detail: message.clone() });
    }
    if let Some(pong) = message.get("pong") {
        return Ok(Incoming::Pong(pong.as_u64().unwrap_or_default()));
    }
    Err(format!("not a message an interface sends: {message}"))
}

/// The answer to a command, under the id it was asked with.
pub(crate) fn reply(id: &Value, result: Result<Value, String>) -> Value {
    match result {
        Ok(result) => json!({ "id": id, "result": result }),
        Err(error) => json!({ "id": id, "error": error }),
    }
}

/// An event, its payload already written out as JSON.
pub(crate) fn event(name: &str, payload: &str) -> String {
    format!("{{\"event\":{},\"payload\":{}}}", Value::from(name), if payload.is_empty() { "null" } else { payload })
}

/// Something for the process to do with its window; `detail` is an object.
pub(crate) fn control(what: &str, detail: Value) -> Value {
    let mut message = match detail {
        Value::Object(fields) => fields,
        _ => Default::default(),
    };
    message.insert("window".into(), what.into());
    Value::Object(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_read_with_its_arguments() {
        assert_eq!(
            read(&json!({ "id": 7, "invoke": "search", "args": { "query": "se" } })),
            Ok(Incoming::Invoke { id: json!(7), command: "search".into(), args: json!({ "query": "se" }) })
        );
        // No arguments is no arguments, however that is written.
        for message in [json!({ "id": 1, "invoke": "hide" }), json!({ "id": 1, "invoke": "hide", "args": null })] {
            assert_eq!(read(&message), Ok(Incoming::Invoke { id: json!(1), command: "hide".into(), args: json!({}) }));
        }
        assert!(read(&json!({ "id": 1, "invoke": "search", "args": ["se"] })).is_err());
        assert!(read(&json!({ "id": 1, "invoke": 3 })).is_err());
    }

    #[test]
    fn what_happened_to_a_window_is_read() {
        assert_eq!(
            read(&json!({ "window": "shown", "ms": 12 })),
            Ok(Incoming::Window { what: "shown".into(), detail: json!({ "window": "shown", "ms": 12 }) })
        );
        assert_eq!(read(&json!({ "pong": 3 })), Ok(Incoming::Pong(3)));
        assert!(read(&json!({ "hello": 1 })).is_err());
        assert!(read(&json!("text")).is_err());
    }

    #[test]
    fn an_answer_goes_back_under_the_id_it_was_asked_with() {
        assert_eq!(reply(&json!(7), Ok(json!([1, 2]))), json!({ "id": 7, "result": [1, 2] }));
        assert_eq!(reply(&json!("a"), Err("no".into())), json!({ "id": "a", "error": "no" }));
        // Nothing to say is still an answer.
        assert_eq!(reply(&json!(2), Ok(Value::Null)), json!({ "id": 2, "result": null }));
    }

    #[test]
    fn an_event_is_one_line_whatever_it_carries() {
        let line = event("heyo://open-item", "\"two\\nlines\"");
        assert!(!line.contains('\n'));
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), json!({ "event": "heyo://open-item", "payload": "two\nlines" }));
        assert_eq!(serde_json::from_str::<Value>(&event("heyo://shown", "null")).unwrap(), json!({ "event": "heyo://shown", "payload": null }));
        assert_eq!(serde_json::from_str::<Value>(&event("heyo://shown", "")).unwrap()["payload"], Value::Null);
    }

    #[test]
    fn a_window_is_told_what_to_do_in_one_object() {
        assert_eq!(control("show", Value::Null), json!({ "window": "show" }));
        assert_eq!(control("frame", json!({ "x": 1, "logical": true })), json!({ "window": "frame", "x": 1, "logical": true }));
    }
}
