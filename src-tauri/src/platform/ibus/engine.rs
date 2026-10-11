//! Being an engine: answering what the daemon asks of one, and saying what
//! an engine says.
//!
//! The daemon asks the component's factory for an engine by name and is
//! given an object path; from then on it calls that object: a field has
//! the focus, it lost it, a key was pressed there. The engine answers by
//! emitting signals from the same path: this text is being composed, this
//! text is the field's. There is one engine for the whole session (IBus's
//! "global engine"), which the daemon carries from field to field.
//!
//! The calls are answered here by hand, one at a time in the order they
//! arrive, rather than by an object server: a key must be answered after
//! what it made the engine say, never before, and that order is the whole
//! of what an input method promises.

use std::sync::{Arc, Mutex};

use zbus::blocking::{Connection, MessageIterator};
use zbus::message::{Flags, Message, Type};
use zbus::zvariant::{ObjectPath, Value};

use super::wire;

const FACTORY: &str = "org.freedesktop.IBus.Factory";
const ENGINE: &str = "org.freedesktop.IBus.Engine";
const SERVICE: &str = "org.freedesktop.IBus.Service";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// A key went up, not down.
const RELEASE: u32 = 1 << 30;
/// When the field lets go of the text being composed (it loses the focus,
/// the user clicks in it), the text vanishes, rather than staying there as
/// the field's own. IBus offers both.
const DROPPED_WHEN_LET_GO: u32 = 0;

/// What the daemon told the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Said {
    /// It made the engine anew: nothing said before is about this one.
    Made,
    /// A field has the focus. `client` names whose it is, as the program's
    /// toolkit gave it ("gtk3-im:gedit"); "fake" stands for no field at
    /// all. A daemon older than 1.5.27 does not say.
    FocusIn { client: Option<String> },
    FocusOut,
    /// What the field can do (`CAN_SHOW_COMPOSING` among it), where that
    /// is other than the daemon said last.
    Capabilities(u32),
    /// What the field is for (a password is `PASSWORD`), likewise.
    Purpose(u32),
    /// The daemon has said all it had to say of the field that got the
    /// focus.
    Described,
    /// The field dropped what was being composed: the user clicked in it.
    Reset,
    /// A key went down in the field.
    Key { symbol: u32 },
    /// The engine was put away: the session has another.
    PutAway,
    /// The daemon is gone, or the connection to it.
    Closed,
}

/// The field shows text being composed in place.
pub(super) const CAN_SHOW_COMPOSING: u32 = 1;
/// The purposes of a field in which nothing is composed.
pub(super) const PASSWORD: u32 = 8;
pub(super) const PIN: u32 = 9;

/// The engine's voice: what is being composed, and what is the field's.
#[derive(Clone)]
pub(super) struct Voice {
    connection: Connection,
    /// The path of the engine the daemon made last.
    path: Arc<Mutex<Option<String>>>,
}

impl Voice {
    pub(super) fn new(connection: &Connection) -> Voice {
        Voice { connection: connection.clone(), path: Arc::default() }
    }

    fn say<B: serde::Serialize + zbus::zvariant::DynamicType>(&self, signal: &str, body: &B) -> Result<(), String> {
        let path = self.path.lock().unwrap().clone().ok_or("IBus has not asked for the engine")?;
        self.connection.emit_signal(None::<&str>, path.as_str(), ENGINE, signal, body).map_err(|e| e.to_string())
    }

    /// Shows `text` as what is being composed, in place of what was shown;
    /// none hides it.
    pub(super) fn composing(&self, text: &str) -> Result<(), String> {
        let end = text.chars().count() as u32;
        self.say("UpdatePreeditText", &(wire::text(text, true), end, !text.is_empty(), DROPPED_WHEN_LET_GO))
    }

    /// Makes `text` the field's.
    pub(super) fn commit(&self, text: &str) -> Result<(), String> {
        self.say("CommitText", &(wire::text(text, false),))
    }
}

/// Shift, Ctrl, Alt, Super and the like, the locks, and the keys that
/// choose a level or a group of the layout: pressed alone they type nothing.
fn modifier(symbol: u32) -> bool {
    matches!(symbol, 0xffe1..=0xffee | 0xfe01..=0xfe13 | 0xff7e | 0xff7f)
}

/// What a call to the engine tells, if anything.
fn told(interface: &str, member: &str, message: &Message) -> Option<Said> {
    let body = message.body();
    match (interface, member) {
        (ENGINE, "FocusIn") => Some(Said::FocusIn { client: None }),
        (ENGINE, "FocusInId") => body.deserialize::<(&str, &str)>().ok().map(|(_, client)| Said::FocusIn { client: Some(client.to_owned()) }),
        // An engine that is switched off has no field either.
        (ENGINE, "FocusOut" | "FocusOutId" | "Disable") => Some(Said::FocusOut),
        (ENGINE, "Reset") => Some(Said::Reset),
        (ENGINE, "SetCapabilities") => body.deserialize::<u32>().ok().map(Said::Capabilities),
        (ENGINE, "ProcessKeyEvent") => {
            let (symbol, _code, state) = body.deserialize::<(u32, u32, u32)>().ok()?;
            (state & RELEASE == 0 && !modifier(symbol)).then_some(Said::Key { symbol })
        }
        (PROPERTIES, "Set") => {
            let (_, name, value) = body.deserialize::<(&str, &str, Value<'_>)>().ok()?;
            let Value::Structure(pair) = value else { return None };
            match (name, pair.fields()) {
                ("ContentType", [Value::U32(purpose), _]) => Some(Said::Purpose(*purpose)),
                _ => None,
            }
        }
        (SERVICE, "Destroy") => Some(Said::PutAway),
        _ => None,
    }
}

/// How a call is answered.
enum Answer {
    Nothing,
    /// To a key: whether the engine kept it from the field. It never does.
    NotTaken,
    Path(String),
    /// A property: the daemon is told which field has the focus by name
    /// (`FocusId`), and the engine does not ask for the text around the
    /// cursor (`ActiveSurroundingText`).
    Property(bool),
    Properties,
    Unknown,
}

fn answer(connection: &Connection, message: &Message, answer: Answer) {
    if message.primary_header().flags().contains(Flags::NoReplyExpected) {
        return;
    }
    let header = message.header();
    let sent = match answer {
        Answer::Nothing => connection.reply(&header, &()),
        Answer::NotTaken => connection.reply(&header, &false),
        Answer::Path(path) => match ObjectPath::try_from(path) {
            Ok(path) => connection.reply(&header, &path),
            Err(e) => Err(e.into()),
        },
        Answer::Property(value) => connection.reply(&header, &Value::from(value)),
        Answer::Properties => connection.reply(&header, &std::collections::HashMap::from([("FocusId", Value::from(true)), ("ActiveSurroundingText", Value::from(false))])),
        Answer::Unknown => connection.reply_error(&header, "org.freedesktop.DBus.Error.UnknownMethod", &"Not something this engine does"),
    };
    if let Err(e) = sent {
        eprintln!("heyo: could not answer IBus: {e}");
    }
}

/// Asks the daemon something, anything, without waiting for the answer;
/// the number the answer will bear. What the daemon says of a field that
/// gets the focus (what it can do, what it is for) it says in the same
/// breath, but only what differs from what it said last, so there is no
/// telling when it has finished: except that an answer to a question asked
/// after the first of it comes after the last.
fn ask(connection: &Connection) -> Option<std::num::NonZeroU32> {
    const BUS: &str = "org.freedesktop.DBus";
    let question = Message::method_call("/org/freedesktop/DBus", "GetId").and_then(|call| call.destination(BUS)?.interface(BUS)?.build(&())).ok()?;
    connection.send(&question).ok()?;
    Some(question.primary_header().serial_num())
}

/// Serves the factory and the engines it makes, named `name`, until the
/// connection ends: every call is answered, and `on` hears what the daemon
/// told the engine it made last. `on` returns before the call is answered,
/// so what it has `voice` say reaches the field first.
pub(super) fn serve(messages: MessageIterator, connection: &Connection, name: &str, voice: &Voice, mut on: impl FnMut(Said)) {
    let mut made = 0u32;
    // The question whose answer says that a field has been described.
    let mut asked = None;
    for message in messages {
        let Ok(message) = message else { break };
        match message.message_type() {
            Type::MethodCall => {}
            Type::MethodReturn | Type::Error if asked.is_some() => {
                if message.header().reply_serial() == asked {
                    asked = None;
                    on(Said::Described);
                }
                continue;
            }
            _ => continue,
        }
        let header = message.header();
        let (Some(path), Some(member)) = (header.path().map(|path| path.as_str().to_owned()), header.member().map(|member| member.as_str().to_owned())) else { continue };
        let interface = header.interface().map(|interface| interface.as_str().to_owned()).unwrap_or_default();
        if interface == FACTORY && member == "CreateEngine" {
            if message.body().deserialize::<&str>().ok() != Some(name) {
                answer(connection, &message, Answer::Unknown);
                continue;
            }
            made += 1;
            let path = format!("/org/freedesktop/IBus/Engine/{made}");
            *voice.path.lock().unwrap() = Some(path.clone());
            on(Said::Made);
            answer(connection, &message, Answer::Path(path));
            continue;
        }
        // An engine made earlier is the daemon's to put away, no more.
        let current = voice.path.lock().unwrap().as_deref() == Some(path.as_str());
        if current {
            if let Some(said) = told(&interface, &member, &message) {
                let focus_in = matches!(said, Said::FocusIn { .. });
                on(said);
                if focus_in {
                    asked = ask(connection);
                }
            }
        }
        let known = match (interface.as_str(), member.as_str()) {
            (ENGINE, "ProcessKeyEvent") => Answer::NotTaken,
            (PROPERTIES, "Get") => match message.body().deserialize::<(&str, &str)>() {
                Ok((_, "FocusId")) => Answer::Property(true),
                Ok((_, "ActiveSurroundingText")) => Answer::Property(false),
                _ => Answer::Unknown,
            },
            (PROPERTIES, "GetAll") => Answer::Properties,
            (ENGINE | SERVICE | PROPERTIES | "org.freedesktop.DBus.Peer", _) => Answer::Nothing,
            _ => Answer::Unknown,
        };
        answer(connection, &message, known);
    }
    on(Said::Closed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_that_type_nothing_by_themselves_are_told_from_the_rest() {
        // Shift, Ctrl, Caps Lock, Alt, Super; AltGr; Num Lock.
        for symbol in [0xffe1, 0xffe4, 0xffe5, 0xffe9, 0xffea, 0xffec, 0xfe03, 0xff7f] {
            assert!(modifier(symbol), "{symbol:#x}");
        }
        // a, Return, BackSpace, Escape, Menu, F13, Left.
        for symbol in [0x61, 0xff0d, 0xff08, 0xff1b, 0xff67, 0xffca, 0xff51] {
            assert!(!modifier(symbol), "{symbol:#x}");
        }
    }
}
