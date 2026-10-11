//! Composing through IBus, the input method framework GNOME runs (on X11
//! and on Wayland): Heyo becomes an IBus engine for as long as it composes.
//!
//! IBus is a bus of its own beside the session's. An engine is offered to
//! it as a component, here registered by the program that serves it
//! (`RegisterComponent`): nothing is installed, and nothing is left behind
//! when the connection ends. Applications never see the engine; they speak
//! to the daemon, which carries one engine, the session's "global engine",
//! from field to field.
//!
//! # Taking the user's engine's place, and giving it back
//!
//! [`compose`] notes which engine the session has, makes Heyo's the global
//! one, and looks at what the daemon then says of the field in front: its
//! program, what it can show, what it is for. Dropping the [`Session`]
//! gives the place back.
//!
//! How that has to be done follows from what the daemon does when a
//! component that was registered this way leaves (`_component_destroy_cb`
//! in its source, and so it behaves): it takes the engine away from the
//! field that has the focus, whichever engine that is, and puts none in its
//! place. Setting the user's engine first and leaving afterwards would
//! leave them with no input method at all. So Heyo leaves first, waits for
//! the daemon to have noticed, and then sets the engine over a second
//! connection that offers nothing: the engine the user had or, if the user
//! or the desktop chose another one meanwhile, that one.
//!
//! The engine to give back is written to a file before anything changes
//! and removed when it has been given back. A Heyo that is killed in
//! between leaves the session without an engine (the daemon's doing, as
//! above), and the file: [`recover`] reads it at the next start.
//!
//! GNOME Shell follows the global engine without acting on it: it sets one
//! when the user chooses an input source, and was not seen to at any other
//! time. Its own picture of the chosen source is not disturbed by an
//! engine it does not know, and when the user does choose another source
//! while Heyo composes, Heyo's engine is put away and composing is over.

mod engine;
mod wire;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use zbus::blocking::{connection, Connection, MessageIterator};
use zbus::zvariant::OwnedValue;

use super::composing::{Composing, Hearing, Stir};
use engine::{Said, Voice};

const IBUS: &str = "org.freedesktop.IBus";
const PATH: &str = "/org/freedesktop/IBus";
/// Heyo's engine, as IBus knows it.
const ENGINE: &str = "heyo";
const COMPONENT: &str = "org.freedesktop.IBus.Heyo";
const TITLE: &str = "Heyo";

/// How long the daemon gets to answer.
const ANSWER: Duration = Duration::from_secs(6);
/// How long the daemon gets to say which field has the focus, once it has
/// said that the engine is in place: it says so in the same breath.
const FOCUS: Duration = Duration::from_millis(1000);
/// How long a field gets to have the focus, where none has it when the
/// engine is in place.
const FIELD: Duration = Duration::from_millis(250);
/// How long the daemon gets to notice that Heyo's engine has left.
const LEAVING: Duration = Duration::from_millis(1500);

// --- Finding the bus --------------------------------------------------------------

/// The file in which the daemon of a display says where its bus is, as
/// libibus names it (`ibus_get_socket_path`): by machine, host and display.
fn address_file(config_home: &Path, machine: &str, wayland_display: Option<&str>, display: Option<&str>) -> PathBuf {
    let (host, number) = match (wayland_display, display) {
        (Some(wayland), _) => ("unix", wayland),
        (None, Some(display)) => {
            let (host, rest) = display.split_once(':').unwrap_or((display, "0"));
            (if host.is_empty() { "unix" } else { host }, rest.split('.').next().unwrap_or("0"))
        }
        (None, None) => ("unix", "0"),
    };
    config_home.join("ibus/bus").join(format!("{machine}-{host}-{number}"))
}

/// The address in such a file, and the daemon's process.
fn address_in(file: &str) -> Option<(String, u32)> {
    let value = |name: &str| file.lines().filter(|line| !line.starts_with('#')).find_map(|line| line.strip_prefix(name)).map(str::trim);
    Some((value("IBUS_ADDRESS=")?.to_owned(), value("IBUS_DAEMON_PID=")?.parse().ok()?))
}

/// Where the session's IBus is, if one is running.
fn address() -> Option<String> {
    let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    if let Some(address) = var("IBUS_ADDRESS") {
        return Some(address);
    }
    let file = var("IBUS_ADDRESS_FILE").map(PathBuf::from).or_else(|| {
        let machine = ["/var/lib/dbus/machine-id", "/etc/machine-id"].iter().find_map(|path| std::fs::read_to_string(path).ok())?;
        Some(address_file(&crate::paths::config_home(), machine.trim(), var("WAYLAND_DISPLAY").as_deref(), var("DISPLAY").as_deref()))
    })?;
    let (address, daemon) = address_in(&std::fs::read_to_string(file).ok()?)?;
    // A daemon that was killed leaves its file behind.
    Path::new(&format!("/proc/{daemon}")).exists().then_some(address)
}

/// Whether the environment names another framework for the toolkits: then
/// applications speak to that one, whether or not an IBus runs as well.
fn another_framework(modules: &[&str]) -> bool {
    modules.iter().any(|module| module.to_lowercase().contains("fcitx"))
}

/// Whether there is an IBus to compose through in this session.
pub(super) fn running() -> bool {
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    !another_framework(&[&var("GTK_IM_MODULE"), &var("QT_IM_MODULE"), &var("XMODIFIERS")]) && address().is_some()
}

// --- Speaking to the daemon -------------------------------------------------------

fn connect(address: &str) -> Result<Connection, String> {
    connection::Builder::address(address).map(|builder| builder.method_timeout(ANSWER)).and_then(connection::Builder::build).map_err(|e| format!("IBus could not be reached: {e}"))
}

fn call<B: serde::Serialize + zbus::zvariant::DynamicType>(bus: &Connection, method: &str, body: &B) -> Result<(), String> {
    bus.call_method(Some(IBUS), PATH, Some(IBUS), method, body).map(drop).map_err(|e| format!("IBus refused {method}: {e}"))
}

/// The session's engine at this moment, if it has one.
fn global_engine(bus: &Connection) -> Result<Option<String>, String> {
    match bus.call_method(Some(IBUS), PATH, Some("org.freedesktop.DBus.Properties"), "Get", &(IBUS, "GlobalEngine")) {
        Ok(reply) => {
            let value: OwnedValue = reply.body().deserialize().map_err(|e| e.to_string())?;
            Ok(wire::engine_name(&value))
        }
        // How the daemon says that there is none.
        Err(zbus::Error::MethodError(_, Some(message), _)) if message.contains("No global engine") => Ok(None),
        Err(e) => Err(format!("IBus did not say which engine is in use: {e}")),
    }
}

fn set_global_engine(bus: &Connection, name: &str) -> Result<(), String> {
    call(bus, "SetGlobalEngine", &name)
}

// --- The engine to give back, in a file -------------------------------------------

/// Notes the engine to give back. The file is made anew rather than
/// written through whatever may be standing at its path.
fn note(file: &Path, engine: &str) -> Result<(), String> {
    use std::io::Write;
    let _ = std::fs::remove_file(file);
    let written = std::fs::OpenOptions::new().write(true).create_new(true).open(file).and_then(|mut file| file.write_all(engine.as_bytes()));
    written.map_err(|e| format!("the input method to give back could not be noted in {}: {e}", file.display()))
}

fn noted(file: &Path) -> Option<String> {
    let engine = std::fs::read_to_string(file).ok()?;
    let engine = engine.trim();
    // An engine's name, not whatever else a file there may hold.
    (!engine.is_empty() && engine.len() < 200 && !engine.contains(char::is_whitespace)).then(|| engine.to_owned())
}

/// Gives the session the engine noted in `file` by a Heyo that did not live
/// to give it back, unless the session has found another since.
fn recover_at(address: Option<String>, file: &Path) {
    let Some(engine) = noted(file) else {
        let _ = std::fs::remove_file(file);
        return;
    };
    let given = address.ok_or_else(|| "no IBus is running".to_owned()).and_then(|address| {
        let bus = connect(&address)?;
        match global_engine(&bus)? {
            Some(name) if name != ENGINE => Ok(false),
            _ => set_global_engine(&bus, &engine).map(|()| true),
        }
    });
    match given {
        Ok(true) => eprintln!("heyo: the input method {engine} was given back, which an earlier Heyo had set aside"),
        Ok(false) => {}
        Err(e) => eprintln!("heyo: the input method {engine} could not be given back: {e}"),
    }
    let _ = std::fs::remove_file(file);
}

/// See [`super::composing::recover`].
pub(super) fn recover() {
    let file = crate::paths::input_method_set_aside();
    let _turn = Turn::take();
    if file.exists() {
        recover_at(address(), &file);
    }
}

/// Taking the session's engine's place and giving it back are one at a
/// time: a recording that begins as the last one ends waits for the engine
/// to have been given back, and notes the user's, not Heyo's own.
struct Turn;

static TAKEN: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

impl Turn {
    /// `None` if the turn before this one does not end.
    fn take() -> Option<Turn> {
        let (taken, freed) = &TAKEN;
        let mut taken = freed.wait_timeout_while(taken.lock().unwrap(), ANSWER, |taken| *taken).unwrap().0;
        (!std::mem::replace(&mut *taken, true)).then_some(Turn)
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        *TAKEN.0.lock().unwrap() = false;
        TAKEN.1.notify_all();
    }
}

// --- Composing --------------------------------------------------------------------

/// What the daemon has said of the engine it made last.
#[derive(Default)]
struct Seen {
    /// Whether anything has the focus, and whose it is (`None` inside: the
    /// daemon did not say).
    focus: Option<Option<String>>,
    /// What the field that has the focus can do, and what it is for. The
    /// daemon says these when they are other than it said last,
    capabilities: u32,
    purpose: u32,
    /// and has said them of the field that has the focus now.
    described: bool,
    /// The engine was put away, or the daemon is gone.
    over: bool,
    /// Composing has begun: what happens is passed on.
    hearing: bool,
}

impl Seen {
    /// Why nothing can be composed at present, going by what the daemon
    /// said: no application's field has the focus, or not one for it.
    fn refusal(&self) -> Option<&'static str> {
        match &self.focus {
            _ if self.over => Some("IBus put Heyo's engine away"),
            None => Some("IBus did not say which field has the focus"),
            Some(_) if !self.described => Some("IBus did not say what the field in front is"),
            Some(None) => Some("this IBus does not say whose field has the focus (it is older than 1.5.27)"),
            // With no field in front the daemon gives the focus to a
            // stand-in of its own.
            Some(Some(client)) if client == "fake" => Some("the window in front takes no input method"),
            _ if self.capabilities & engine::CAN_SHOW_COMPOSING == 0 => Some("the field in front does not show text being composed"),
            _ if matches!(self.purpose, engine::PASSWORD | engine::PIN) => Some("a password is being typed"),
            _ => None,
        }
    }

    fn in_a_field(&self) -> bool {
        self.refusal().is_none()
    }
}

#[derive(Default)]
struct Shared {
    seen: Mutex<Seen>,
    changed: Condvar,
}

impl Shared {
    /// Waits for what the daemon says to be `enough`, or for the engine to
    /// be over, and no longer than `within`.
    fn wait(&self, within: Duration, enough: impl Fn(&Seen) -> bool) -> std::sync::MutexGuard<'_, Seen> {
        self.changed.wait_timeout_while(self.seen.lock().unwrap(), within, |seen| !(seen.over || enough(seen))).unwrap().0
    }
}

impl Composing for Voice {
    fn show(&mut self, text: &str) -> Result<(), String> {
        self.composing(text)
    }

    fn commit(&mut self, text: &str) -> Result<(), String> {
        Voice::commit(self, text)
    }
}

/// What the engine's thread does with what the daemon said: notes it, and
/// once composing has begun passes on what its hearer is to know.
fn heard(said: Said, shared: &Shared, own_keys: &[u32], voice: &mut Voice, on: &mut Hearing) {
    let mut seen = shared.seen.lock().unwrap();
    let was_in_a_field = seen.in_a_field();
    let stir = match said {
        Said::Made => {
            let hearing = seen.hearing;
            *seen = Seen { hearing, ..Seen::default() };
            None
        }
        Said::FocusIn { client } => {
            seen.focus = Some(client);
            seen.described = false;
            None
        }
        Said::FocusOut => {
            seen.focus = None;
            None
        }
        Said::Capabilities(capabilities) => {
            seen.capabilities = capabilities;
            None
        }
        Said::Purpose(purpose) => {
            seen.purpose = purpose;
            None
        }
        Said::Described => {
            seen.described = true;
            None
        }
        Said::Reset => was_in_a_field.then_some(Stir::Reset),
        Said::Key { symbol } => (was_in_a_field && !own_keys.contains(&symbol)).then_some(Stir::Key),
        Said::PutAway | Said::Closed => {
            let already = std::mem::replace(&mut seen.over, true);
            (!already).then_some(Stir::Gone)
        }
    };
    // A field to compose in comes and goes with the focus, and with what
    // the daemon says the field is.
    let stir = stir.or(match (was_in_a_field, seen.in_a_field()) {
        (false, true) => Some(Stir::Entered),
        (true, false) => Some(Stir::Left),
        _ => None,
    });
    let hearing = seen.hearing;
    drop(seen);
    shared.changed.notify_all();
    if let (true, Some(stir)) = (hearing, stir) {
        on(stir, voice);
    }
}

/// Heyo's engine in the session's place, until dropped.
struct Session {
    /// Offers nothing: what is set over it outlives Heyo's leaving.
    control: Connection,
    /// Offers the engine. `None` once Heyo has left.
    serving: Option<Connection>,
    served: Option<std::thread::JoinHandle<()>>,
    shared: Arc<Shared>,
    voice: Voice,
    /// The engine the session had, and where that is noted.
    before: Option<String>,
    file: PathBuf,
    /// Until the engine has been given back.
    _turn: Turn,
}

impl Session {
    /// Makes Heyo's engine the session's and begins hearing, if there is a
    /// field to compose in.
    fn take_over(&mut self) -> Result<(), String> {
        let said_all = |seen: &Seen| seen.focus.is_some() && seen.described;
        set_global_engine(&self.control, ENGINE)?;
        let mut seen = self.shared.wait(FOCUS, said_all);
        // The first time a daemon makes this engine it has not asked yet
        // whether the engine wants to be told whose field it is. It knows
        // by the second time.
        if !seen.over && seen.focus == Some(None) {
            drop(seen);
            set_global_engine(&self.control, ENGINE)?;
            seen = self.shared.wait(FOCUS, said_all);
        }
        // A hotkey that was only just let go had taken the focus from the
        // window in front, which may not have it back yet.
        if seen.focus != Some(None) && !seen.in_a_field() {
            drop(seen);
            seen = self.shared.wait(FIELD, Seen::in_a_field);
        }
        match seen.refusal() {
            Some(why) => Err(why.to_owned()),
            None => {
                seen.hearing = true;
                Ok(())
            }
        }
    }

    /// Leaves, and gives the session an engine again: see the module's
    /// header for why in this order.
    fn give_back(&mut self) {
        self.shared.seen.lock().unwrap().hearing = false;
        // Somebody may have chosen another engine meanwhile: then that one.
        let chosen = global_engine(&self.control).ok().flatten().filter(|name| name != ENGINE);
        let engine = chosen.or(self.before.take());
        if let Some(serving) = self.serving.take() {
            // The daemon answers in order: what the engine said last has
            // been read by the time it answers this.
            let _ = serving.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "GetId", &());
            let _ = serving.close();
        }
        if let Some(served) = self.served.take() {
            let _ = served.join();
        }
        let until = Instant::now() + LEAVING;
        while Instant::now() < until && !matches!(global_engine(&self.control), Ok(None) | Err(_)) {
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Some(engine) = engine {
            if let Err(e) = set_global_engine(&self.control, &engine) {
                eprintln!("heyo: the input method {engine} could not be given back: {e}");
            }
        }
        let _ = std::fs::remove_file(&self.file);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.give_back();
    }
}

impl Composing for Session {
    fn show(&mut self, text: &str) -> Result<(), String> {
        self.voice.composing(text)
    }

    fn commit(&mut self, text: &str) -> Result<(), String> {
        self.voice.commit(text)
    }
}

/// Composes through the IBus at `address`, noting the engine to give back
/// in `file`.
fn compose_at(address: &str, file: &Path, own_keys: &[u32], mut on: Hearing) -> Result<Session, String> {
    let turn = Turn::take().ok_or("Heyo's engine is still in the session's place")?;
    let control = connect(address)?;
    let before = global_engine(&control)?.filter(|name| name != ENGINE);
    let serving = connect(address)?;
    let messages = MessageIterator::from(&serving);
    if let Some(before) = &before {
        note(file, before)?;
    }
    let shared = Arc::new(Shared::default());
    let voice = Voice::new(&serving);
    // From here on, whatever happens, the session gets its engine back.
    let mut session = Session { control, serving: Some(serving.clone()), served: None, shared: shared.clone(), voice: voice.clone(), before, file: file.to_owned(), _turn: turn };
    call(&serving, "RegisterComponent", &wire::component(COMPONENT, ENGINE, TITLE))?;
    let own_keys = own_keys.to_vec();
    let served = std::thread::Builder::new().name("ibus-engine".into()).spawn(move || {
        let mut speaking = voice.clone();
        engine::serve(messages, &serving, ENGINE, &voice, |said| heard(said, &shared, &own_keys, &mut speaking, &mut on));
    });
    session.served = Some(served.map_err(|e| format!("the engine could not be started: {e}"))?);
    session.take_over()?;
    Ok(session)
}

/// See [`super::composing::compose`].
pub(super) fn compose(own_keys: &[u32], on: Hearing) -> Result<Box<dyn Composing>, String> {
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    if another_framework(&[&var("GTK_IM_MODULE"), &var("QT_IM_MODULE"), &var("XMODIFIERS")]) {
        return Err("the session's input method is not IBus".into());
    }
    let address = address().ok_or("no IBus is running")?;
    Ok(Box::new(compose_at(&address, &crate::paths::input_method_set_aside(), own_keys, on)?))
}

#[cfg(test)]
mod tests {
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc::{channel, Receiver};

    use zbus::message::Type;
    use zbus::zvariant::{OwnedObjectPath, Structure, Value};

    use super::*;

    #[test]
    fn the_daemons_file_is_named_as_libibus_names_it() {
        let config = Path::new("/home/ada/.config");
        let file = |wayland, display| address_file(config, "7af66f30", wayland, display).to_string_lossy().into_owned();
        assert_eq!(file(None, Some(":0")), "/home/ada/.config/ibus/bus/7af66f30-unix-0");
        assert_eq!(file(None, Some(":1.0")), "/home/ada/.config/ibus/bus/7af66f30-unix-1");
        assert_eq!(file(None, Some("far:10.0")), "/home/ada/.config/ibus/bus/7af66f30-far-10");
        // A Wayland session's daemon is named by that display, also where an X server runs beside it.
        assert_eq!(file(Some("wayland-0"), Some(":0")), "/home/ada/.config/ibus/bus/7af66f30-unix-wayland-0");
        assert_eq!(file(None, None), "/home/ada/.config/ibus/bus/7af66f30-unix-0");
    }

    #[test]
    fn the_file_says_where_the_bus_is_and_whose_it_is() {
        let file = "# This file is created by ibus-daemon, please do not modify it.\n# IBUS_ADDRESS=unix:path=/old\nIBUS_ADDRESS=unix:path=/run/user/1000/ibus/dbus-rlOikjA7,guid=68ce3e39\nIBUS_DAEMON_PID=1393349\n";
        assert_eq!(address_in(file), Some(("unix:path=/run/user/1000/ibus/dbus-rlOikjA7,guid=68ce3e39".into(), 1393349)));
        assert_eq!(address_in("IBUS_ADDRESS=unix:path=/x\n"), None);
        assert_eq!(address_in(""), None);
    }

    #[test]
    fn another_framework_named_for_the_toolkits_is_not_ibus() {
        assert!(!another_framework(&["ibus", "ibus", "@im=ibus"]));
        assert!(!another_framework(&["", "", ""]));
        assert!(another_framework(&["fcitx", "fcitx", "@im=fcitx"]));
        assert!(another_framework(&["", "fcitx5", ""]));
        assert!(another_framework(&["", "", "@im=Fcitx"]));
    }

    #[test]
    fn nothing_is_composed_where_there_is_no_field_for_it() {
        let seen = |focus: Option<Option<&str>>, capabilities, purpose| Seen { focus: focus.map(|client| client.map(str::to_owned)), capabilities, purpose, described: true, over: false, hearing: false };
        assert!(seen(Some(Some("gtk3-im:gedit")), 41, 0).in_a_field());
        assert!(seen(Some(Some("gnome-shell")), 9, 10).in_a_field());
        let why = |seen: Seen| seen.refusal().unwrap_or_default();
        // No application has a field for an input method in front.
        assert!(why(seen(Some(Some("fake")), 41, 0)).contains("takes no input method"));
        // The daemon is too old to say whose field it is: it may be nobody's.
        assert!(why(seen(Some(None), 41, 0)).contains("older than 1.5.27"));
        assert!(why(seen(Some(Some("xim")), 8, 0)).contains("does not show"));
        assert!(why(seen(Some(Some("gtk3-im:gedit")), 0, 0)).contains("does not show"));
        assert!(why(seen(Some(Some("gtk3-im:gedit")), 41, engine::PASSWORD)).contains("password"));
        assert!(why(seen(Some(Some("gtk3-im:gedit")), 41, engine::PIN)).contains("password"));
        // The daemon has not said (yet), or took the engine away again.
        assert!(why(seen(None, 41, 0)).contains("did not say which field"));
        assert!(why(Seen { described: false, ..seen(Some(Some("gtk3-im:gedit")), 41, 0) }).contains("did not say what"));
        assert!(why(Seen { over: true, ..seen(Some(Some("gtk3-im:gedit")), 41, 0) }).contains("put Heyo's engine away"));
    }

    #[test]
    fn the_engine_to_give_back_is_noted_in_a_file_of_its_own_making() {
        let dir = std::env::temp_dir().join(format!("heyo-note-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (file, elsewhere) = (dir.join("note"), dir.join("elsewhere"));
        assert_eq!(noted(&file), None);
        note(&file, "libpinyin").unwrap();
        assert_eq!(noted(&file).as_deref(), Some("libpinyin"));
        note(&file, "xkb:us::eng").unwrap();
        assert_eq!(noted(&file).as_deref(), Some("xkb:us::eng"));
        // A link standing at the path is replaced, not written through.
        std::fs::write(&elsewhere, "untouched").unwrap();
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &file).unwrap();
        note(&file, "libpinyin").unwrap();
        assert_eq!(std::fs::read_to_string(&elsewhere).unwrap(), "untouched");
        // What is no engine's name is not given to IBus as one.
        std::fs::write(&file, "two words\n").unwrap();
        assert_eq!(noted(&file), None);
        std::fs::write(&file, "").unwrap();
        assert_eq!(noted(&file), None);
        // With no IBus to give it to, the note is cleared away all the same.
        note(&file, "libpinyin").unwrap();
        recover_at(None, &file);
        assert!(!file.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --- Against a daemon ---------------------------------------------------------

    /// An ibus-daemon of the test's own, with directories of its own: it
    /// neither finds nor disturbs the one of the session the test runs in.
    struct Daemon {
        process: Child,
        dir: PathBuf,
        address: String,
    }

    impl Daemon {
        fn start() -> Daemon {
            // Under a short path: the daemon's socket is made below it.
            let dir = crate::paths::runtime_dir().join(format!("heyo-ibus-test-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let process = Command::new("ibus-daemon")
                .args(["--single", "--panel", "disable", "--config", "disable", "--emoji-extension", "disable", "--cache", "none"])
                .env("XDG_CONFIG_HOME", dir.join("config"))
                .env("XDG_CACHE_HOME", dir.join("cache"))
                .env("XDG_RUNTIME_DIR", &dir)
                .env("DISPLAY", ":65000")
                .env_remove("WAYLAND_DISPLAY")
                .env_remove("IBUS_ADDRESS")
                .env_remove("IBUS_ADDRESS_FILE")
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("ibus-daemon is needed for this test");
            let mut daemon = Daemon { process, dir: dir.clone(), address: String::new() };
            let until = Instant::now() + Duration::from_secs(10);
            while daemon.address.is_empty() {
                assert!(Instant::now() < until, "ibus-daemon did not start");
                std::thread::sleep(Duration::from_millis(50));
                let files = std::fs::read_dir(dir.join("config/ibus/bus")).into_iter().flatten().flatten();
                if let Some((address, process)) = files.filter_map(|file| address_in(&std::fs::read_to_string(file.path()).ok()?)).next() {
                    assert_eq!(process, daemon.process.id());
                    daemon.address = address;
                }
            }
            assert!(daemon.address.contains(&*dir.to_string_lossy()), "{} is not the test's own daemon", daemon.address);
            daemon
        }
    }

    impl Drop for Daemon {
        fn drop(&mut self) {
            let _ = self.process.kill();
            let _ = self.process.wait();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// A text field as an application's toolkit presents one to IBus, and
    /// what it was sent, in the order it arrived: `composing …`, `commit …`,
    /// and `key not taken` for the answer to a key.
    struct Field {
        bus: Connection,
        path: OwnedObjectPath,
        sent: Receiver<String>,
    }

    const CONTEXT: &str = "org.freedesktop.IBus.InputContext";
    /// Shows text being composed, has a focus, knows the text around its cursor.
    const A_TEXT_FIELD: u32 = 1 | 8 | 32;
    const NOTHING: [&str; 0] = [];

    fn words(text: &Value<'_>) -> String {
        let fields = <&Structure>::try_from(text).unwrap().fields();
        match &fields[2] {
            Value::Str(text) => text.as_str().to_owned(),
            other => panic!("not a text: {other:?}"),
        }
    }

    impl Field {
        fn open(address: &str, capabilities: u32) -> Field {
            let bus = connect(address).unwrap();
            let messages = MessageIterator::from(&bus);
            let reply = bus.call_method(Some(IBUS), PATH, Some(IBUS), "CreateInputContext", &"heyo-test").unwrap();
            let path: OwnedObjectPath = reply.body().deserialize().unwrap();
            let (sender, sent) = channel();
            std::thread::spawn(move || {
                let mut showing = false;
                for message in messages.flatten() {
                    // The daemon signs what a field is sent "org.freedesktop.IBus",
                    // which zbus will not have for a sender: it panics on
                    // reading such a header. So a signal is known here by
                    // the shape of what it carries: CommitText a text,
                    // UpdatePreeditText a text, a cursor and whether to show it.
                    let body = message.body();
                    let said = match (message.message_type(), body.signature().to_string().trim_matches(['(', ')'])) {
                        (Type::Signal, "v") => match body.deserialize::<Value<'_>>().unwrap() {
                            Value::Structure(text) if text.fields().first() == Some(&Value::from("IBusText")) => format!("commit {}", words(&Value::Structure(text))),
                            _ => continue,
                        },
                        (Type::Signal, "vub") => {
                            let (text, _, visible) = body.deserialize::<(Value<'_>, u32, bool)>().unwrap();
                            let shown = if visible { words(&text) } else { String::new() };
                            // The daemon says "nothing" again whenever it tidies up.
                            if !std::mem::replace(&mut showing, !shown.is_empty()) && shown.is_empty() {
                                continue;
                            }
                            format!("composing {shown}")
                        }
                        (Type::MethodReturn, "b") => format!("key {}", if body.deserialize::<bool>().unwrap() { "taken" } else { "not taken" }),
                        _ => continue,
                    };
                    if sender.send(said).is_err() {
                        break;
                    }
                }
            });
            let field = Field { bus, path, sent };
            field.tell("SetCapabilities", &capabilities);
            field
        }

        fn tell<B: serde::Serialize + zbus::zvariant::DynamicType>(&self, method: &str, body: &B) {
            self.bus.call_method(Some(IBUS), self.path.as_str(), Some(CONTEXT), method, body).unwrap();
        }

        /// What the field is for: 8 is a password.
        fn purpose(&self, purpose: u32) {
            self.bus.call_method(Some(IBUS), self.path.as_str(), Some("org.freedesktop.DBus.Properties"), "Set", &(CONTEXT, "ContentType", Value::from((purpose, 0u32)))).unwrap();
        }

        /// A key goes down (or up) in the field; whether IBus kept it.
        fn key(&self, symbol: u32, state: u32) -> bool {
            self.bus.call_method(Some(IBUS), self.path.as_str(), Some(CONTEXT), "ProcessKeyEvent", &(symbol, 0u32, state)).unwrap().body().deserialize().unwrap()
        }

        /// What the field was sent since this was last asked.
        fn sent(&self) -> Vec<String> {
            let mut all = Vec::new();
            while let Ok(said) = self.sent.recv_timeout(Duration::from_millis(300)) {
                all.push(said);
            }
            all
        }
    }

    /// What was heard of the field, and what is said in answer to a key.
    fn hearing(heard: &Arc<Mutex<Vec<Stir>>>) -> Hearing {
        let heard = heard.clone();
        Box::new(move |stir, composing| {
            heard.lock().unwrap().push(stir);
            if stir == Stir::Key {
                composing.commit("[before the key]").unwrap();
                composing.show("").unwrap();
            }
        })
    }

    fn heard(heard: &Arc<Mutex<Vec<Stir>>>) -> Vec<Stir> {
        std::thread::sleep(Duration::from_millis(200));
        std::mem::take(&mut *heard.lock().unwrap())
    }

    /// `cargo test -- --ignored with_an_ibus_daemon`: starts an ibus-daemon
    /// of its own (see [`Daemon`]) and plays the application to it.
    #[test]
    #[ignore = "needs ibus-daemon and its xkb engines"]
    fn with_an_ibus_daemon_text_is_composed_and_the_users_engine_given_back() {
        const USERS: &str = "xkb:us::eng";
        const ANOTHER: &str = "xkb:gb::eng";
        const MENU: u32 = 0xff67;
        let daemon = Daemon::start();
        let file = daemon.dir.join("set-aside");
        let control = connect(&daemon.address).unwrap();
        let engine = || global_engine(&control).unwrap();
        let field = Field::open(&daemon.address, A_TEXT_FIELD);
        field.tell("FocusIn", &());
        set_global_engine(&control, USERS).unwrap();
        assert_eq!(engine().as_deref(), Some(USERS));
        let stirs = Arc::new(Mutex::new(Vec::new()));

        // Heyo's engine takes the place of the user's, which is noted first.
        let mut composing = compose_at(&daemon.address, &file, &[MENU], hearing(&stirs)).unwrap();
        assert_eq!(engine().as_deref(), Some(ENGINE));
        assert_eq!(noted(&file).as_deref(), Some(USERS));
        field.sent();

        // Text being composed is replaced whole; committed text is the field's.
        composing.show("Hello wor").unwrap();
        composing.show("Hello world").unwrap();
        composing.commit("Hello world.").unwrap();
        composing.show("").unwrap();
        composing.show("三明治 and 😀").unwrap();
        assert_eq!(field.sent(), ["composing Hello wor", "composing Hello world", "commit Hello world.", "composing ", "composing 三明治 and 😀"]);

        // A key of the user's: what is said about it reaches the field
        // before the key is handed back, and the key is never kept.
        assert!(!field.key(0x61, 0));
        assert_eq!(field.sent(), ["commit [before the key]", "composing ", "key not taken"]);
        assert_eq!(heard(&stirs), [Stir::Key]);
        // Letting it go, Shift by itself, and a key that is Heyo's own are not typing.
        assert!(!field.key(0x61, 1 << 30) && !field.key(0xffe1, 0) && !field.key(MENU, 0));
        assert_eq!((field.sent(), heard(&stirs)), (vec!["key not taken".to_owned(); 3], vec![]));

        // The field loses the focus: what was being composed is gone from
        // it, and nothing of it has become its text.
        composing.show("not left behind").unwrap();
        assert_eq!(field.sent(), ["composing not left behind"]);
        field.tell("FocusOut", &());
        assert_eq!(field.sent(), ["composing "]);
        assert_eq!(heard(&stirs), [Stir::Left]);
        field.tell("FocusIn", &());
        assert_eq!(heard(&stirs), [Stir::Entered]);
        // The user clicks in the field: the daemon passes that on, and
        // leaves it to the field to show no more.
        composing.show("clicked on").unwrap();
        assert_eq!(field.sent(), ["composing clicked on"]);
        field.tell("Reset", &());
        assert_eq!(field.sent(), NOTHING);
        assert_eq!(heard(&stirs), [Stir::Reset]);
        // The field turns out to be for a password, and then not: there is
        // a field to compose in again.
        field.purpose(engine::PASSWORD);
        assert_eq!(heard(&stirs), [Stir::Left]);
        field.purpose(0);
        assert_eq!(heard(&stirs), [Stir::Entered]);
        // A key in another field than the one composed in is nobody's business.
        let plain = Field::open(&daemon.address, 8);
        field.tell("FocusOut", &());
        plain.tell("FocusIn", &());
        assert!(!plain.key(0x61, 0));
        assert_eq!(plain.sent(), ["key not taken"]);
        plain.tell("FocusOut", &());
        field.tell("FocusIn", &());
        assert_eq!(heard(&stirs), [Stir::Left, Stir::Entered]);

        // Done: the user's engine is back, and the note gone.
        field.sent();
        drop(composing);
        assert_eq!(engine().as_deref(), Some(USERS));
        assert!(!file.exists());
        assert_eq!((field.sent(), heard(&stirs)), (vec![], vec![]));

        // Somebody chooses another engine while Heyo composes: that is the
        // end of composing, and the engine they chose is the one they keep.
        let mut composing = compose_at(&daemon.address, &file, &[], hearing(&stirs)).unwrap();
        composing.show("half a sen").unwrap();
        assert_eq!(field.sent(), ["composing half a sen"]);
        set_global_engine(&control, ANOTHER).unwrap();
        assert_eq!(heard(&stirs), [Stir::Left, Stir::Gone]);
        assert_eq!(field.sent(), ["composing "]);
        drop(composing);
        assert_eq!(engine().as_deref(), Some(ANOTHER));
        assert!(!file.exists());

        // Where there is nothing to compose in, the engine is back at once.
        let refused = |field: &Field| {
            let refused = compose_at(&daemon.address, &file, &[], hearing(&stirs)).err().expect("composing was not refused");
            assert_eq!((engine().as_deref(), file.exists()), (Some(ANOTHER), false), "{refused}");
            assert_eq!((field.sent(), heard(&stirs)), (vec![], vec![]));
            refused
        };
        field.purpose(engine::PASSWORD);
        assert!(refused(&field).contains("password"));
        field.purpose(0);
        field.tell("FocusOut", &());
        assert!(refused(&field).contains("takes no input method"));
        plain.tell("FocusIn", &());
        assert!(refused(&plain).contains("does not show"));
        plain.tell("FocusOut", &());

        // The window in front has the focus back a moment after composing
        // begins, as when a hotkey that began it is let go: that is in time.
        let (bus, path) = (field.bus.clone(), field.path.clone());
        let back = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            bus.call_method(Some(IBUS), path.as_str(), Some(CONTEXT), "FocusIn", &()).unwrap();
        });
        let mut composing = compose_at(&daemon.address, &file, &[], hearing(&stirs)).unwrap();
        back.join().unwrap();

        // Heyo is killed while it composes. What was being composed is
        // gone from the field, and the daemon leaves the session without
        // an engine:
        composing.show("the last words").unwrap();
        assert_eq!(field.sent(), ["composing the last words"]);
        composing.serving.take().unwrap().close().unwrap();
        std::mem::forget(composing);
        assert_eq!(field.sent(), ["composing "]);
        assert_eq!(engine(), None);
        // which the next Heyo finds noted, and gives back.
        assert_eq!(noted(&file).as_deref(), Some(ANOTHER));
        recover_at(Some(daemon.address.clone()), &file);
        assert_eq!((engine().as_deref(), file.exists()), (Some(ANOTHER), false));
        // Unless the session has an engine again by then: then it keeps it.
        note(&file, USERS).unwrap();
        recover_at(Some(daemon.address.clone()), &file);
        assert_eq!((engine().as_deref(), file.exists()), (Some(ANOTHER), false));
    }
}
