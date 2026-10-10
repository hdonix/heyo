//! Questions the host itself asks in the palette.
//!
//! The palette already has a dialog, for the `ui.confirm` requests of
//! extensions (`confirmAlert`). The host asks through the same one: its
//! question goes to the palette looking like a request of the runtime's,
//! with an id from a range the runtime never reaches, and the reply is
//! taken out of the palette's messages (`runtime_send`) before the runtime
//! would see it. One question at a time; a new one answers the old with No.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::wire::event;

/// Where the host's ids start. The runtime counts its requests from 1.
const HOST_IDS: u32 = 1 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answer {
    Yes,
    /// The third button: yes, and do not ask again.
    Always,
    No,
}

pub(crate) struct Question {
    pub title: String,
    pub message: String,
    /// The buttons for `Answer::Yes` and `Answer::Always`.
    pub yes: &'static str,
    pub always: &'static str,
}

impl Question {
    /// As the palette's dialog takes it (`UiRequests["ui.confirm"]`).
    fn request(&self, id: u32) -> Value {
        json!({
            "type": "request",
            "id": id,
            "method": "ui.confirm",
            "params": {
                "title": self.title,
                "message": self.message,
                "primaryAction": { "title": self.yes },
                "alwaysAction": { "title": self.always },
                "dismissAction": { "title": "Cancel" },
            },
        })
    }
}

/// A question with two answers, for giving leave: Yes must be chosen, so
/// the dialog starts on the other button and a key meant for something else
/// does not give it.
pub(crate) struct Consent {
    pub title: String,
    pub message: String,
    pub yes: &'static str,
    pub no: &'static str,
}

impl Consent {
    fn request(&self, id: u32) -> Value {
        json!({
            "type": "request",
            "id": id,
            "method": "ui.confirm",
            "params": {
                "title": self.title,
                "message": self.message,
                "primaryAction": { "title": self.yes },
                "dismissAction": { "title": self.no },
                "cautious": true,
            },
        })
    }
}

type Then = Box<dyn FnOnce(Answer) + Send>;

struct Pending {
    id: u32,
    asked: Instant,
    then: Then,
}

static NEXT: AtomicU32 = AtomicU32::new(HOST_IDS);
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

/// How the palette's reply reads: `true`, `"always"`, or anything else for No.
fn read(result: Option<&Value>) -> Answer {
    match result {
        Some(Value::Bool(true)) => Answer::Yes,
        Some(Value::String(which)) if which == "always" => Answer::Always,
        _ => Answer::No,
    }
}

/// Keeps a question as the one being asked; returns its id and the one it
/// replaced, which is thereby answered No.
fn keep(then: Then) -> (u32, Option<Then>) {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let previous = PENDING.lock().unwrap().replace(Pending { id, asked: Instant::now(), then });
    (id, previous.map(|p| p.then))
}

/// Takes the question a reply is for, if it is the one being asked.
fn take(id: u32) -> Option<Then> {
    let mut pending = PENDING.lock().unwrap();
    if pending.as_ref().is_some_and(|p| p.id == id) {
        return pending.take().map(|p| p.then);
    }
    None
}

/// Asks in the palette, which must be showing. `then` runs once, on a
/// thread of its own, with the answer.
pub(crate) fn ask(app: &AppHandle, question: Question, then: impl FnOnce(Answer) + Send + 'static) {
    let (id, previous) = keep(Box::new(then));
    if let Some(previous) = previous {
        previous(Answer::No);
    }
    let _ = app.emit(event::RUNTIME, question.request(id));
}

/// Asks for leave in the palette, which must be showing. `then` runs once,
/// on a thread of its own, with whether it was given.
pub(crate) fn ask_consent(app: &AppHandle, consent: Consent, then: impl FnOnce(bool) + Send + 'static) {
    let (id, previous) = keep(Box::new(move |answer| then(answer != Answer::No)));
    if let Some(previous) = previous {
        previous(Answer::No);
    }
    let _ = app.emit(event::RUNTIME, consent.request(id));
}

/// Hears a reply the palette sent. True when it was for the host, and so
/// is not the runtime's to see.
pub(crate) fn answered(id: u32, result: Option<&Value>) -> bool {
    if id < HOST_IDS {
        return false;
    }
    // A reply to a question no longer asked (the palette was reopened) is
    // still the host's, and means nothing.
    if let Some(then) = take(id) {
        let answer = read(result);
        std::thread::spawn(move || then(answer));
    }
    true
}

/// Whether a question was put only just now. The palette may be told to
/// hide by a command that was finishing as the question went up; for that
/// moment it stays.
pub(crate) fn just_asked() -> bool {
    PENDING.lock().unwrap().as_ref().is_some_and(|p| p.asked.elapsed() < Duration::from_millis(1500))
}

/// The palette is going away: whatever is being asked is answered No.
pub(crate) fn dismiss() {
    let then = PENDING.lock().unwrap().take().map(|p| p.then);
    if let Some(then) = then {
        std::thread::spawn(move || then(Answer::No));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn a_reply_reads_as_an_answer() {
        assert_eq!(read(Some(&json!(true))), Answer::Yes);
        assert_eq!(read(Some(&json!("always"))), Answer::Always);
        for no in [json!(false), json!(null), json!("yes"), json!(1), json!({ "confirmed": true })] {
            assert_eq!(read(Some(&no)), Answer::No, "{no}");
        }
        assert_eq!(read(None), Answer::No);
    }

    #[test]
    fn the_question_is_one_the_palettes_dialog_can_show() {
        let question = Question { title: "Paste?".into(), message: "Two lines.".into(), yes: "Paste", always: "Always Paste" };
        let request = question.request(HOST_IDS + 4);
        assert_eq!(request["type"], "request");
        assert_eq!(request["method"], "ui.confirm");
        assert_eq!(request["id"], HOST_IDS + 4);
        assert_eq!(request["params"]["primaryAction"]["title"], "Paste");
        assert_eq!(request["params"]["alwaysAction"]["title"], "Always Paste");
        assert_eq!(request["params"]["dismissAction"]["title"], "Cancel");
    }

    #[test]
    fn leave_is_asked_for_with_two_answers_and_starts_on_no() {
        let consent = Consent { title: "Allow Tiles to move windows?".into(), message: "It will not ask again.".into(), yes: "Allow", no: "Deny" };
        let request = consent.request(HOST_IDS + 5);
        assert_eq!(request["method"], "ui.confirm");
        assert_eq!(request["params"]["primaryAction"]["title"], "Allow");
        assert_eq!(request["params"]["dismissAction"]["title"], "Deny");
        assert_eq!(request["params"]["cautious"], true);
        assert!(request["params"].get("alwaysAction").is_none());
    }

    // One test, because the question being asked is one for the whole process.
    #[test]
    fn a_question_is_answered_once_and_only_by_its_own_reply() {
        // The runtime's replies pass by untouched.
        assert!(!answered(7, Some(&json!(true))));

        let (tx, rx) = mpsc::channel();
        let first = tx.clone();
        let (id, replaced) = keep(Box::new(move |answer| first.send(("first", answer)).unwrap()));
        assert!(id >= HOST_IDS && replaced.is_none());
        assert!(just_asked());
        // A reply to some other host question is swallowed and changes nothing.
        assert!(answered(id + 100, Some(&json!(true))));
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        assert!(answered(id, Some(&json!("always"))));
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), ("first", Answer::Always));
        // Said twice, it is heard once.
        assert!(answered(id, Some(&json!(true))));
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        assert!(!just_asked());

        // A second question takes the place of an unanswered one.
        let second = tx.clone();
        let (old, _) = keep(Box::new(move |answer| second.send(("second", answer)).unwrap()));
        let (new, replaced) = keep(Box::new(move |answer| tx.send(("third", answer)).unwrap()));
        replaced.expect("the unanswered question")(Answer::No);
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), ("second", Answer::No));
        assert!(answered(old, Some(&json!(true))));
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        // The palette leaving answers No.
        dismiss();
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), ("third", Answer::No));
        assert!(answered(new, Some(&json!(true))));
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }
}
