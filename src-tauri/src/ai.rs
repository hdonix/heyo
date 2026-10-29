//! What the extension runtime asks of a model (`host.ai.ask`): `AI.ask`,
//! and through it Heyo's own Ask AI, AI Chat and selected-text actions.
//!
//! The runtime is given neither the key nor the address of the model: it
//! sends the question, and the host asks the model the settings name
//! (`models`). Each question is answered on a task of its own, so the
//! runtime's other messages go on being read meanwhile. The answer is told
//! as it is written (`HostMessage::Progress`, with the request's number),
//! then the reply carries all of it, or what went wrong in words for the
//! user. `host.cancel` calls a question off, and so does the runtime's
//! going away: the connection to the model is closed, and nothing is
//! replied.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::models::{Model, Question};
use crate::runtime::{reply, reply_to};
use crate::wire::HostMessage;

/// The questions being answered, by the runtime process that asked
/// (`background`) and the number of its request.
static ASKING: Mutex<Option<HashMap<(bool, u64), tauri::async_runtime::JoinHandle<()>>>> = Mutex::new(None);

/// Asks the model the settings name; the reply to request `id` follows
/// when it has answered.
pub(crate) fn ask(app: &AppHandle, id: &Value, background: bool, question: Question) {
    let Some(number) = id.as_u64() else {
        return reply_to(app, background, &reply(id.clone(), Err::<Value, _>("a question needs the number of its request")));
    };
    let model = Model::from_settings(&app.state::<crate::SettingsState>().0.read().unwrap());
    let (app, id) = (app.clone(), id.clone());
    // Held until the task is known, which it must be before it can end.
    let mut asking = ASKING.lock().unwrap();
    let task = tauri::async_runtime::spawn(async move {
        let mut told = String::new();
        let mut tell = |text: &str| {
            if let Some(data) = progress(&mut told, text) {
                reply_to(&app, background, &HostMessage::Progress { id: number, data }.json());
            }
        };
        let answer = model.answer(&question, &mut tell).await;
        // Called off meanwhile: whoever asked has stopped waiting.
        if ASKING.lock().unwrap().get_or_insert_with(HashMap::new).remove(&(background, number)).is_some() {
            reply_to(&app, background, &reply(id, answer.map(Value::from).map_err(|e| e.message())));
        }
    });
    asking.get_or_insert_with(HashMap::new).insert((background, number), task);
}

/// Calls off the question of request `number`, if it is still being answered.
pub(crate) fn cancel(background: bool, number: u64) {
    let task = ASKING.lock().unwrap().get_or_insert_with(HashMap::new).remove(&(background, number));
    if let Some(task) = task {
        task.abort();
    }
}

/// A runtime process ended: nobody waits for its questions.
pub(crate) fn runtime_exited(background: bool) {
    let mut asking = ASKING.lock().unwrap();
    let asking = asking.get_or_insert_with(HashMap::new);
    let theirs: Vec<_> = asking.keys().filter(|(whose, _)| *whose == background).copied().collect();
    for key in theirs {
        if let Some(task) = asking.remove(&key) {
            task.abort();
        }
    }
}

/// What the runtime is told of the answer, which is now `text`, having
/// been told `told`: what follows it, or (should the beginning have
/// changed) all of it. Nothing when there is nothing new.
fn progress(told: &mut String, text: &str) -> Option<Value> {
    let data = match text.strip_prefix(told.as_str()) {
        Some("") => return None,
        Some(more) => json!({ "more": more }),
        None => json!({ "text": text }),
    };
    *told = text.to_owned();
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_is_told_a_piece_at_a_time() {
        let mut told = String::new();
        assert_eq!(progress(&mut told, "Ray"), Some(json!({ "more": "Ray" })));
        assert_eq!(progress(&mut told, "Rayleigh"), Some(json!({ "more": "leigh" })));
        assert_eq!(progress(&mut told, "Rayleigh"), None);
        // A beginning written anew is told whole.
        assert_eq!(progress(&mut told, "Mie"), Some(json!({ "text": "Mie" })));
        assert_eq!(progress(&mut told, "Mie scattering"), Some(json!({ "more": " scattering" })));
    }
}
