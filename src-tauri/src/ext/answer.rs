//! Answering one `ext.*` request: who asks, whether they may, whether this
//! session can, and then the thing itself.
//!
//! All that is consulted or changed on the way is handed in as a [`World`]:
//! Heyo's tables, and a port ([`Running`]) for what only the running
//! application can do, which is to ask the user, move its windows and start
//! its helpers. The desktop is `platform::desktop`'s. What is left to do
//! once the answer is known comes back with it, as [`Effect`]s. So a test
//! answers requests with tables of its own and a fake desktop, and needs no
//! application.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use serde_json::{json, Value};

use crate::activities::{self, Millis, Table};
use crate::clipboard::sources::{self, Shown};
use crate::clipboard::{self, store, ClipboardHistory, Entry, Kind};
use crate::confirm::Consent;
use crate::dictation;
use crate::extensions::{self, Declared};
use crate::files::Files;
use crate::permissions::{self, Caller, Granted, Identity, Permission};
use crate::platform::desktop::Supported as Abilities;
use crate::platform::system::Action;
use crate::platform::windows::{self, LastWindow, Layout, Rect, WindowInfo};
use crate::platform::{self, Support};
use crate::root::Index;
use crate::settings::Settings;
use crate::wire::{
    AiProvider, Capture, CaptureTarget, ErrorCode, ExtCaller, ExtError, ExtRequest, ExtWindow, MemoryItem, MemoryResult, Module, Presentation, RecognizedText, Subscription,
    Understood, WindowLayout,
};

pub(crate) type Answer = Result<Value, ExtError>;

/// What a request is answered from, and may change.
pub(crate) struct World<'a> {
    pub index: &'a RwLock<Index>,
    pub settings: &'a RwLock<Settings>,
    pub activities: &'a Mutex<Table>,
    pub subscriptions: &'a Mutex<Subscriptions>,
    pub granted: &'a Granted,
    pub history: &'a ClipboardHistory,
    pub files: &'a Arc<Files>,
    /// The window that was in front before the palette came up.
    pub last_window: &'a LastWindow,
    pub running: &'a dyn Running,
}

/// What only the running application can do. Each of these either has an
/// answer the request goes on with, or has its place in what the request
/// does; whatever can wait for the answer is an [`Effect`] instead.
pub(crate) trait Running {
    /// Puts a question up in the palette and waits for the answer.
    fn ask(&self, consent: Consent) -> bool;
    /// The monitors' rectangles, in physical pixels.
    fn monitors(&self) -> Result<Vec<Rect>, String>;
    fn hide_palette(&self);
    /// Takes a picture of the screen, into the extension's cache.
    fn capture(&self, extension: &str, target: CaptureTarget) -> Result<Capture, ExtError>;
    /// The text in an image file.
    fn recognize_text(&self, image: &Path) -> Result<String, String>;
    /// Starts a recording whose events go to `subscription`. Answers with
    /// the recording's number.
    fn listen(&self, subscription: u64, language: Option<String>, indicator: bool) -> Result<u64, ExtError>;
    /// What the words and the meaning of a query find together in the
    /// clipboard history, where the user has finding by meaning turned on.
    fn meant(&self, query: &str, kind: Option<Kind>) -> Option<store::Found>;
    /// Puts an entry of the history back on the clipboard.
    fn copy(&self, entry: &Entry) -> Result<(), String>;
    /// Pastes into the window in front.
    fn paste(&self, text: String, html: Option<String>) -> Result<(), ExtError>;
}

/// What an answer leaves to be done, before it is sent.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Effect {
    Notify { title: String, body: String },
    /// The table of activities changed: what that calls for, and the
    /// palette to be told.
    Activities(Vec<activities::Effect>),
    /// A recording ends, and its text comes to the one listening.
    StopListening(u64),
    /// A recording is discarded.
    CancelListening(u64),
}

pub(crate) struct Answered {
    pub answer: Answer,
    pub effects: Vec<Effect>,
}

/// Answers a request of an extension's, as the runtime wrote it, from the
/// runtime process that is (`background`) or is not the one for the menu
/// bar and the background. May wait for the user: call it off the thread
/// that reads the runtime's messages.
pub(crate) fn answer(world: &World, message: Value, background: bool) -> Answered {
    let mut effects = Vec::new();
    let answer = answered(world, message, background, &mut effects);
    Answered { answer, effects }
}

fn answered(world: &World, message: Value, background: bool, effects: &mut Vec<Effect>) -> Answer {
    let method = message["method"].as_str().unwrap_or_default().to_owned();
    let given: ExtCaller = serde_json::from_value(message.clone()).map_err(|_| ExtError::invalid(format!("{method} does not say which extension asks")))?;
    let request = ExtRequest::read(message).map_err(|e| ExtError::invalid(format!("Heyo cannot answer {method}: {e}")))?;
    let declared = extensions::declared(Path::new(&given.dir));
    let caller = caller(given, declared, background)?;
    if let Some(need) = permissions::needed(&request) {
        let switched_off = need.permission == Permission::Activity && !activities::enabled(&world.settings.read().unwrap(), &caller.name);
        world.granted.check(&caller, need, &method, switched_off, &|consent| world.running.ask(consent))?;
    }
    if let Some(ability) = ability(&request) {
        can(ability, supported())?;
    }
    carry_out(world, &caller, request, effects)
}

/// Who asks, once the manifest in the directory the request names has been
/// read: the name it gives must be that manifest's. Heyo's own commands are
/// the ones that come from no directory (`permissions::identity`).
fn caller(given: ExtCaller, declared: Option<Declared>, background: bool) -> Result<Caller, ExtError> {
    let route = given.via.unwrap_or_default();
    match permissions::identity(&given.extension, &given.dir) {
        Identity::Heyo => {
            return Ok(Caller { name: given.extension, title: "Heyo".into(), heyo: None, builtin: true, background, route, location: crate::extensions::BUILTIN.into() })
        }
        Identity::Pretender => return Err(ExtError::invalid(format!("{} is Heyo's own name, not that of the extension installed in {:?}", given.extension, given.dir))),
        Identity::Extension => {}
    }
    // The name becomes a directory's (for captures): it must be one name.
    let name = given.extension;
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
        return Err(ExtError::invalid(format!("{name:?} is not the name of an extension")));
    }
    match declared {
        Some(declared) if declared.name == name => Ok(Caller { name, title: declared.title, heyo: declared.heyo, builtin: false, background, route, location: given.dir }),
        _ => Err(ExtError::invalid(format!("{name} is not the extension installed in {:?}", given.dir))),
    }
}

// --- What this session can do ---------------------------------------------------

/// Something a session may be unable to do, without which a request
/// cannot be answered at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ability {
    /// Reaching other applications' windows.
    Windows,
    Capture,
    Dictation,
    Paste,
}

/// What a request needs the session to be able to do, if anything. Every
/// request is named, so that a new one has to say.
fn ability(request: &ExtRequest) -> Option<Ability> {
    use ExtRequest::*;
    match request {
        WindowsList | WindowsActive | WindowsActivate { .. } | WindowsClose { .. } | WindowsArrange { .. } | WindowsSetBounds { .. } | FrontmostApplication => Some(Ability::Windows),
        // The monitors are the toolkit's to tell, whatever the desktop
        // lets Heyo do with windows.
        WindowsMonitors => None,
        ScreenCapture { .. } => Some(Ability::Capture),
        DictationListen { .. } => Some(Ability::Dictation),
        ClipboardPaste { .. } => Some(Ability::Paste),
        // Work on what the extension supplies, or on what Heyo keeps.
        ScreenRecognizeText { .. } | MemorySearch { .. } | MemoryGet { .. } | ClipboardHistoryCopy { .. } | FilesSearch { .. } | AiProvider => None,
        // Each desktop does what it can of these, and says what it cannot.
        SystemRun { .. } | SystemAppearance | SystemNotify { .. } | SystemAudioDevices | SystemUseAudioDevice { .. } => None,
        // The palette's section is on every desktop.
        ActivityStart { .. } | ActivityUpdate { .. } | ActivityEnd { .. } | ActivityList | ActivityGet { .. } | ActivityPresentation | ActivityOnEvent => None,
        // Asking what is possible, and ending what one started.
        Unsubscribe { .. } | Supported { .. } | DictationInstalled | DictationStop { .. } => None,
    }
}

fn can(ability: Ability, supported: Abilities) -> Result<(), ExtError> {
    let (support, what) = match ability {
        Ability::Windows => (support(Module::Windows, supported), "Reaching other applications' windows"),
        Ability::Capture => (support(Module::Screen, supported), "Capturing the screen"),
        Ability::Dictation => (support(Module::Dictation, supported), "Dictation"),
        Ability::Paste => (supported.paste, "Pasting"),
    };
    match support {
        Support::None => Err(ExtError::unsupported(platform::unsupported(what))),
        _ => Ok(()),
    }
}

fn supported() -> Abilities {
    platform::desktop::desktop().supported()
}

/// How well the session supports a part of the API.
fn support(module: Module, supported: Abilities) -> Support {
    match module {
        Module::Windows => supported.windows,
        Module::Screen => supported.capture,
        // The recorder is PulseAudio's or ALSA's.
        Module::Dictation => {
            if cfg!(target_os = "linux") {
                Support::Full
            } else {
                Support::None
            }
        }
        // Where copies cannot be watched as they happen, the clipboard is
        // looked at once a second, and only its text.
        Module::Memory | Module::ClipboardHistory => match supported.clipboard_watch {
            Support::Full => Support::Full,
            _ => Support::Limited,
        },
        Module::Files | Module::System | Module::Ai => Support::Full,
        // The palette's section; the status bar is not drawn yet.
        Module::Activity => Support::Limited,
    }
}

// --- The requests ---------------------------------------------------------------

fn value(result: impl serde::Serialize) -> Answer {
    serde_json::to_value(result).map_err(|e| ExtError::from(e.to_string()))
}

/// Does what a request asks, for a caller who may ask it of a session that
/// can do it.
fn carry_out(world: &World, caller: &Caller, request: ExtRequest, effects: &mut Vec<Effect>) -> Answer {
    use ExtRequest::*;
    match request {
        Unsubscribe { subscription } => {
            let source = world.subscriptions.lock().unwrap().close(subscription, caller.background);
            if let Some(source) = source {
                stop(world, subscription, source, effects);
            }
        }
        Supported { module } => return value(support(module, supported())),

        WindowsList => return value(described(windows::list(None)?, working_window(world), &world.running.monitors()?)),
        WindowsActive => {
            let current = working_window(world);
            let all = described(windows::list(None)?, current, &world.running.monitors()?);
            return value(all.into_iter().find(|window| Some(window.id) == current));
        }
        WindowsActivate { id } => {
            // The palette makes way for the window, as when a command of
            // Heyo's switches. It is the user's and the command's in it: one
            // in the background leaves it be.
            if !caller.background {
                world.running.hide_palette();
            }
            windows::activate(id)?;
        }
        WindowsClose { id } => windows::close(id)?,
        WindowsArrange { id, layout } => windows::apply(id, layout_of(&layout)?, &world.running.monitors()?)?,
        WindowsSetBounds { id, bounds } => {
            if bounds.width <= 0 || bounds.height <= 0 {
                return Err(ExtError::invalid("A window needs a width and a height."));
            }
            windows::apply(id, Layout::Exact(bounds), &world.running.monitors()?)?;
        }
        WindowsMonitors => return value(world.running.monitors()?),
        FrontmostApplication => return Ok(frontmost_application(world)),

        ScreenCapture { target } => return value(world.running.capture(&caller.name, target)?),
        ScreenRecognizeText { path } => {
            let image = Path::new(&path);
            if !image.is_file() {
                return Err(ExtError::invalid(format!("{path} is not a file.")));
            }
            if !crate::ocr::installed() {
                return Err(ExtError::new(ErrorCode::Unavailable, "Text recognition is not set up yet. Download it in Heyo Settings → Screenshots."));
            }
            return value(RecognizedText { text: world.running.recognize_text(image)? });
        }

        DictationInstalled => return Ok(dictation::installed().into()),
        DictationListen { language, indicator } => {
            if let Some(language) = language.as_deref().filter(|language| !dictation::LANGUAGES.contains(language)) {
                return Err(ExtError::invalid(format!("Dictation does not know the language {language:?}.")));
            }
            // Open before the recording starts: what is heard at once has
            // somewhere to go.
            let subscription = world.subscriptions.lock().unwrap().open(caller.background);
            let recording = world.running.listen(subscription, language, indicator.unwrap_or(true)).inspect_err(|_| world.subscriptions.lock().unwrap().ended(subscription))?;
            world.subscriptions.lock().unwrap().attach(subscription, Source::Recording(recording));
            return value(Subscription { subscription });
        }
        DictationStop { subscription } => {
            let recording = world.subscriptions.lock().unwrap().recording(subscription, caller.background);
            effects.extend(recording.map(Effect::StopListening));
        }

        MemorySearch { text, sources, kinds, since, until, limit, by_meaning } => {
            // The clipboard is the one source there is.
            if sources.is_some_and(|sources| !sources.iter().any(|source| source == "clipboard")) {
                return value(MemoryResult { items: Vec::new(), understood: Understood::default(), relaxed: false });
            }
            let text = text.as_deref().unwrap_or_default();
            let one_kind = kinds.as_deref().and_then(|kinds| (kinds.len() == 1).then(|| kinds[0]));
            // Where there is nothing to add to what the words find (finding
            // by meaning is off, say), the words' answer it is.
            let meant = (by_meaning.unwrap_or(false) && !text.trim().is_empty()).then(|| world.running.meant(text, one_kind)).flatten();
            let found = meant.unwrap_or_else(|| world.history.search(text, one_kind));
            let understood = Understood {
                terms: found.query.terms,
                app: found.query.app,
                kind: found.query.kind,
                from: found.query.time.as_ref().map(|span| span.start),
                to: found.query.time.as_ref().map(|span| span.end),
            };
            let entries = sources::shown(found.entries, &found.by_meaning, &world.index.read().unwrap().apps);
            let items = chosen(entries, &Filter { kinds, since, until, limit });
            return value(MemoryResult { items, understood, relaxed: found.relaxed });
        }
        MemoryGet { id } => {
            let entry = world.history.get(entry_id(&id)?);
            let shown = entry.and_then(|entry| sources::shown(vec![entry], &[], &world.index.read().unwrap().apps).pop());
            return value(shown.as_ref().map(item));
        }
        ClipboardHistoryCopy { id } => {
            let entry = world.history.get(entry_id(&id)?).ok_or_else(|| ExtError::invalid(format!("There is no entry {id} in the clipboard history.")))?;
            world.running.copy(&entry)?;
        }

        FilesSearch { query, limit } => return Ok(first_files(value(world.files.search(&query))?, limit)),

        SystemRun { action } => {
            // A command of Heyo's has asked already, in its own words.
            if let Some((question, yes)) = confirmed(action).filter(|_| !caller.builtin) {
                world.granted.confirm_each_time(caller, question.into(), yes, &|consent| world.running.ask(consent))?;
            }
            return value(platform::system::run(action)?);
        }
        SystemAppearance => {
            let dark = platform::system::dark().map_err(ExtError::unsupported)?;
            return Ok(if dark { "dark" } else { "light" }.into());
        }
        SystemNotify { title, body } => effects.push(Effect::Notify { title, body: body.unwrap_or_default() }),
        SystemAudioDevices => return value(platform::system::audio_devices()?),
        SystemUseAudioDevice { kind, name } => platform::system::use_audio_device(kind, &name)?,

        ClipboardPaste { text, html } => world.running.paste(text, html)?,

        AiProvider => return value(provider(&world.settings.read().unwrap().ai())),

        ActivityStart { attributes, content, announce, launch_type, seq } => {
            let source = activities::Source { extension: caller.name.clone(), title: caller.title.clone(), location: caller.location.clone() };
            // A command the user ran, or the handler of an action they chose.
            let user = launch_type.as_deref() != Some("background");
            let id = attributes.id.clone();
            change(world, effects, |table, now| table.start(source, attributes, content, announce.unwrap_or(true), user, seq, now))?;
            return Ok(json!({ "id": id }));
        }
        ActivityUpdate { id, content, alert, seq } => change(world, effects, |table, now| table.update(&caller.name, &id, content, alert, seq, now))?,
        ActivityEnd { id, final_content, keep } => change(world, effects, |table, now| table.end(&caller.name, &id, final_content, keep.as_deref(), now))?,
        ActivityList => return value(table(world).ids(&caller.name)),
        ActivityGet { id } => return Ok(table(world).get(&caller.name, &id).is_some().into()),
        ActivityPresentation => return value(presentation()),
        ActivityOnEvent => {
            let subscription = world.subscriptions.lock().unwrap().open(caller.background);
            table(world).listen(&caller.name, subscription);
            world.subscriptions.lock().unwrap().attach(subscription, Source::Activities);
            return value(Subscription { subscription });
        }
    }
    Ok(Value::Null)
}

fn table<'a>(world: &World<'a>) -> MutexGuard<'a, Table> {
    world.activities.lock().unwrap_or_else(|e| e.into_inner())
}

/// Changes the table of activities, and leaves what the change calls for
/// to be done.
fn change(world: &World, effects: &mut Vec<Effect>, f: impl FnOnce(&mut Table, Millis) -> Result<Vec<activities::Effect>, ExtError>) -> Result<(), ExtError> {
    let called_for = f(&mut table(world), activities::now())?;
    effects.push(Effect::Activities(called_for));
    Ok(())
}

/// How activities show here: in the palette's "In Progress" section, with
/// one notification per activity for what the user must hear of. No
/// status bar presenter yet, so nothing is expanded and no buttons show
/// outside the palette.
fn presentation() -> Presentation {
    Presentation { support: Support::None, compact_characters: activities::COMPACT_CHARACTERS as u32, expanded: false, buttons: false, version: 1 }
}

// --- Windows ---------------------------------------------------------------------

/// The window the user is working in: the one in front, or the one that
/// was before the palette came up.
fn working_window(world: &World) -> Option<windows::Window> {
    windows::active_external().or(*world.last_window.0.lock().unwrap())
}

/// Which monitor a rectangle is on: the one that holds its centre, else the first.
fn monitor_of(bounds: Rect, monitors: &[Rect]) -> Option<u32> {
    let (x, y) = (bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
    let holds = |m: &Rect| x >= m.x && x < m.x + m.width && y >= m.y && y < m.y + m.height;
    monitors.iter().position(holds).or((!monitors.is_empty()).then_some(0)).map(|index| index as u32)
}

/// The windows as extensions are told of them. `current` is the window the
/// user is working in, which with the palette in front is not the one the
/// desktop calls active.
fn described(windows: Vec<WindowInfo>, current: Option<windows::Window>, monitors: &[Rect]) -> Vec<ExtWindow> {
    windows
        .into_iter()
        .map(|window| ExtWindow {
            active: Some(window.id) == current,
            monitor: window.bounds.and_then(|bounds| monitor_of(bounds, monitors)),
            id: window.id,
            title: window.title,
            class: window.class,
            bounds: window.bounds,
        })
        .collect()
}

/// The application of the window the user is working in, as Raycast's API
/// tells of one; null when there is no such window.
fn frontmost_application(world: &World) -> Value {
    let current = working_window(world);
    let Some(window) = current.and_then(|id| windows::list(Some(id)).ok()?.into_iter().find(|w| w.id == id)) else {
        return Value::Null;
    };
    let class = window.class.to_lowercase();
    let index = world.index.read().unwrap();
    // A window of a program nothing installed describes still has a name.
    match index.apps.iter().find(|a| a.window_classes().contains(&class)) {
        Some(entry) => crate::host::application(entry),
        None => json!({ "name": window.class, "localizedName": window.class, "path": "" }),
    }
}

/// The layout an extension names, or the part of the work area it gives.
fn layout_of(layout: &WindowLayout) -> Result<Layout, ExtError> {
    match layout {
        WindowLayout::Named(name) => {
            windows::LAYOUTS.iter().find(|(id, ..)| id == name).map(|(.., layout)| *layout).ok_or_else(|| ExtError::invalid(format!("There is no layout called {name:?}.")))
        }
        WindowLayout::Fraction { fraction: [x, y, width, height] } => {
            let fits = |from: i32, length: i32| from >= 0 && length >= 1 && from + length <= 12;
            if !fits(*x, *width) || !fits(*y, *height) {
                return Err(ExtError::invalid("A fraction is x, y, width and height in twelfths of the work area, and stays inside it."));
            }
            Ok(Layout::Fraction(*x, *y, *width, *height))
        }
    }
}

// --- Subscriptions ---------------------------------------------------------------

/// What a subscription is to, and so what stops when it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    /// A recording, by its number.
    Recording(u64),
    /// The events of the extension's activities.
    Activities,
}

struct Open {
    /// Which runtime process subscribed, and so is sent the events.
    background: bool,
    /// Absent until the source has started.
    source: Option<Source>,
}

/// The subscriptions that are open.
pub(crate) struct Subscriptions {
    last: u64,
    open: BTreeMap<u64, Open>,
}

impl Subscriptions {
    pub(crate) const fn new() -> Subscriptions {
        Subscriptions { last: 0, open: BTreeMap::new() }
    }

    fn open(&mut self, background: bool) -> u64 {
        self.last += 1;
        self.open.insert(self.last, Open { background, source: None });
        self.last
    }

    /// Says what a subscription is to, once that has started. If the
    /// subscription is gone already, nothing is left to stop.
    fn attach(&mut self, id: u64, source: Source) {
        if let Some(open) = self.open.get_mut(&id) {
            open.source = Some(source);
        }
    }

    /// The process a subscription's events go to, while it is open.
    pub(crate) fn listener(&self, id: u64) -> Option<bool> {
        self.open.get(&id).map(|open| open.background)
    }

    /// The recording it is to, for the process that subscribed.
    fn recording(&self, id: u64, background: bool) -> Option<u64> {
        match self.open.get(&id).filter(|open| open.background == background)?.source? {
            Source::Recording(recording) => Some(recording),
            Source::Activities => None,
        }
    }

    /// The source is done: no more events, and nothing to stop.
    pub(crate) fn ended(&mut self, id: u64) {
        self.open.remove(&id);
    }

    /// The process that subscribed wants no more: returns what to stop.
    fn close(&mut self, id: u64, background: bool) -> Option<Source> {
        if self.listener(id) != Some(background) {
            return None;
        }
        self.open.remove(&id).and_then(|open| open.source)
    }

    /// A runtime process is gone, and all it subscribed to with it.
    fn close_all(&mut self, background: bool) -> Vec<(u64, Source)> {
        let ids: Vec<u64> = self.open.iter().filter(|(_, open)| open.background == background).map(|(id, _)| *id).collect();
        ids.into_iter().filter_map(|id| Some((id, self.open.remove(&id)?.source?))).collect()
    }
}

/// Stops what a subscription that has ended was to.
fn stop(world: &World, subscription: u64, source: Source, effects: &mut Vec<Effect>) {
    match source {
        Source::Recording(recording) => effects.push(Effect::CancelListening(recording)),
        Source::Activities => table(world).unlisten(subscription),
    }
}

/// A runtime process ended: what its commands subscribed to stops.
pub(crate) fn runtime_exited(world: &World, background: bool) -> Vec<Effect> {
    let mut effects = Vec::new();
    let closed = world.subscriptions.lock().unwrap().close_all(background);
    for (subscription, source) in closed {
        stop(world, subscription, source, &mut effects);
    }
    effects
}

// --- Memory and the clipboard history ------------------------------------------

/// An entry of the clipboard history as extensions are told of it. Copies
/// marked as secrets never became entries, so none can be told.
fn item(shown: &Shown) -> MemoryItem {
    let entry = &shown.entry;
    MemoryItem {
        id: format!("clipboard:{}", entry.id),
        source: "clipboard",
        kind: entry.kind,
        text: entry.text.clone(),
        image: entry.image.clone(),
        width: entry.width,
        height: entry.height,
        app: entry.app.clone(),
        app_name: shown.app_name.clone(),
        app_icon: shown.app_icon.as_ref().map(|icon| icon.to_string_lossy().into_owned()),
        at: entry.copied_at,
        first_at: Some(entry.first_copied_at),
        times_copied: Some(entry.times_copied),
        pinned: entry.pinned,
        by_meaning: shown.by_meaning.then_some(true),
    }
}

/// The entry an item's id names.
fn entry_id(id: &str) -> Result<u64, ExtError> {
    id.strip_prefix("clipboard:").and_then(|number| number.parse().ok()).ok_or_else(|| ExtError::invalid(format!("{id:?} is not the id of something Heyo remembers.")))
}

#[derive(Default)]
struct Filter {
    kinds: Option<Vec<Kind>>,
    /// Milliseconds since the Unix epoch; `until` is not included.
    since: Option<u64>,
    until: Option<u64>,
    limit: Option<u32>,
}

/// How many items a search gives: 50 unless asked, never more than Heyo's
/// own view of the history shows. There is no asking for everything.
fn limit(asked: Option<u32>) -> usize {
    (asked.unwrap_or(50) as usize).min(clipboard::SHOWN)
}

/// What of the entries found is given to the extension.
fn chosen(entries: Vec<Shown>, filter: &Filter) -> Vec<MemoryItem> {
    entries
        .iter()
        .filter(|shown| filter.kinds.as_ref().is_none_or(|kinds| kinds.contains(&shown.entry.kind)))
        .filter(|shown| filter.since.is_none_or(|since| shown.entry.copied_at >= since))
        .filter(|shown| filter.until.is_none_or(|until| shown.entry.copied_at < until))
        .take(limit(filter.limit))
        .map(item)
        .collect()
}

// --- Files, the system, the model ------------------------------------------------

/// The files found, the first `limit` of them, with whether the index is
/// still being built and how many files and folders it holds.
fn first_files(found: Value, limit: Option<u32>) -> Value {
    let mut files = found["files"].as_array().cloned().unwrap_or_default();
    if let Some(limit) = limit {
        files.truncate(limit as usize);
    }
    json!({ "files": files, "scanning": found["scanning"].as_bool().unwrap_or(false), "total": found["total"].as_u64().unwrap_or(0) })
}

/// What the user is asked before an action that ends the session, and the
/// button that says yes. Whatever an extension was allowed, these are
/// confirmed every time.
fn confirmed(action: Action) -> Option<(&'static str, &'static str)> {
    match action {
        Action::Restart => Some(("Restart the computer?", "Restart")),
        Action::ShutDown => Some(("Shut the computer down?", "Shut Down")),
        Action::LogOut => Some(("Log out?", "Log Out")),
        _ => None,
    }
}

/// Which model answers, from the settings as the helpers are given them.
fn provider(ai: &Value) -> AiProvider {
    match ai["provider"].as_str() {
        Some("local") => AiProvider { kind: "local", model: ai["localModel"].as_str().unwrap_or_default().to_owned(), local: true },
        _ => AiProvider { kind: "claude", model: String::new(), local: false },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::desktop::fake::Fake;
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const LEFT: Rect = Rect { x: 0, y: 0, width: 1920, height: 1080 };
    const RIGHT: Rect = Rect { x: 1920, y: 0, width: 2560, height: 1440 };

    /// The running application as a test has it: the user answers as told,
    /// and what was asked of it is noted.
    struct Scripted {
        /// What the user says to a question.
        says: Cell<bool>,
        monitors: RefCell<Vec<Rect>>,
        /// The recording `listen` starts, or why it does not.
        recording: RefCell<Result<u64, ExtError>>,
        /// "ask Restart the computer?", "hide palette", … in the order asked.
        asked: RefCell<Vec<String>>,
    }

    impl Scripted {
        fn note(&self, what: String) {
            self.asked.borrow_mut().push(what);
        }
    }

    impl Running for Scripted {
        fn ask(&self, consent: Consent) -> bool {
            self.note(format!("ask {}", consent.title));
            self.says.get()
        }

        fn monitors(&self) -> Result<Vec<Rect>, String> {
            Ok(self.monitors.borrow().clone())
        }

        fn hide_palette(&self) {
            self.note("hide palette".into());
        }

        fn capture(&self, extension: &str, target: CaptureTarget) -> Result<Capture, ExtError> {
            self.note(format!("capture {target:?} for {extension}"));
            Ok(Capture { path: format!("/cache/{extension}/captures/1.png"), width: 4, height: 3 })
        }

        fn recognize_text(&self, image: &Path) -> Result<String, String> {
            self.note(format!("recognize {}", image.display()));
            Ok("text".into())
        }

        fn listen(&self, subscription: u64, language: Option<String>, indicator: bool) -> Result<u64, ExtError> {
            self.note(format!("listen for {subscription} in {} {}", language.as_deref().unwrap_or("the language set"), if indicator { "shown" } else { "unseen" }));
            self.recording.borrow().clone()
        }

        fn meant(&self, query: &str, _kind: Option<Kind>) -> Option<store::Found> {
            self.note(format!("meant {query}"));
            None
        }

        fn copy(&self, entry: &Entry) -> Result<(), String> {
            self.note(format!("copy {}", entry.id));
            Ok(())
        }

        fn paste(&self, text: String, html: Option<String>) -> Result<(), ExtError> {
            self.note(format!("paste {text}{}", if html.is_some() { " with html" } else { "" }));
            Ok(())
        }
    }

    /// Who a request says it is from.
    #[derive(Clone)]
    struct From {
        extension: String,
        dir: String,
        via: Option<&'static str>,
        background: bool,
    }

    impl From {
        /// One of Heyo's own commands.
        fn heyo() -> From {
            From { extension: "@heyo".into(), dir: String::new(), via: None, background: false }
        }

        fn in_background(&self) -> From {
            From { background: true, ..self.clone() }
        }

        fn through_raycast(&self) -> From {
            From { via: Some("raycast"), ..self.clone() }
        }
    }

    /// A world for a test: tables of its own, a fake desktop, a scripted
    /// application, and a directory to install extensions in.
    struct Bench {
        index: RwLock<Index>,
        settings: RwLock<Settings>,
        activities: Mutex<Table>,
        subscriptions: Mutex<Subscriptions>,
        granted: Granted,
        history: ClipboardHistory,
        files: Arc<Files>,
        last_window: LastWindow,
        app: Scripted,
        desktop: &'static Fake,
        dir: PathBuf,
    }

    impl Drop for Bench {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    impl Bench {
        /// With these windows on the desktop (id, class), the first in
        /// front, and a user who says yes.
        fn new(windows: &[(windows::Window, &str)]) -> Bench {
            static BENCHES: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!("heyo-answer-test-{}-{}", std::process::id(), BENCHES.fetch_add(1, Ordering::Relaxed)));
            Bench {
                index: RwLock::default(),
                settings: RwLock::new(Settings::default()),
                activities: Mutex::default(),
                subscriptions: Mutex::new(Subscriptions::new()),
                granted: Granted::kept(|| None),
                history: ClipboardHistory::in_memory(),
                files: Arc::default(),
                last_window: LastWindow::default(),
                app: Scripted { says: Cell::new(true), monitors: RefCell::new(vec![LEFT, RIGHT]), recording: RefCell::new(Ok(7)), asked: RefCell::default() },
                desktop: Fake::with_windows(windows),
                dir,
            }
        }

        fn world(&self) -> World<'_> {
            World {
                index: &self.index,
                settings: &self.settings,
                activities: &self.activities,
                subscriptions: &self.subscriptions,
                granted: &self.granted,
                history: &self.history,
                files: &self.files,
                last_window: &self.last_window,
                running: &self.app,
            }
        }

        /// Installs an extension whose manifest is this.
        fn install_manifest(&self, folder: &str, manifest: Value) -> From {
            let dir = self.dir.join(folder);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
            From { extension: manifest["name"].as_str().unwrap_or_default().into(), dir: dir.to_string_lossy().into_owned(), via: None, background: false }
        }

        /// Installs an extension written for Heyo that lists these permissions.
        fn install(&self, name: &str, permissions: &[&str]) -> From {
            let title = format!("{}{}", name[..1].to_uppercase(), &name[1..]);
            self.install_manifest(name, json!({ "name": name, "title": title, "heyo": { "apiVersion": 1, "permissions": permissions } }))
        }

        /// Every request of a test goes through here, so none reaches the
        /// desktop the test runs on.
        fn send(&self, message: Value, background: bool) -> Answered {
            self.desktop.during(|| answer(&self.world(), message, background))
        }

        fn ask(&self, from: &From, method: &str, params: Value) -> Answered {
            let mut message = json!({ "type": "request", "id": 1, "method": method, "extension": from.extension, "dir": from.dir });
            if !params.is_null() {
                message["params"] = params;
            }
            if let Some(via) = from.via {
                message["via"] = via.into();
            }
            self.send(message, from.background)
        }

        fn result(&self, from: &From, method: &str, params: Value) -> Value {
            self.ask(from, method, params).answer.unwrap_or_else(|error| panic!("{method} failed: {}", error.message))
        }

        fn error(&self, from: &From, method: &str, params: Value) -> ExtError {
            self.ask(from, method, params).answer.expect_err(method)
        }

        /// What was asked of the application since the last look.
        fn asked(&self) -> Vec<String> {
            self.app.asked.take()
        }
    }

    const QUESTION: &str = "ask Allow Tiles to see, switch, move and close the windows of other applications?";

    #[test]
    fn a_request_is_the_extensions_whose_manifest_is_where_it_says() {
        let bench = Bench::new(&[(1, "firefox")]);
        let tiles = bench.install("tiles", &["windows"]);
        assert_eq!(bench.result(&tiles, "ext.windows.list", Value::Null)[0]["id"], 1);
        // It is called by the title its manifest gives.
        assert_eq!(bench.asked(), [QUESTION]);

        // The name of another extension than the one installed there, or of none.
        let notes = From { extension: "notes".into(), ..tiles.clone() };
        assert_eq!(bench.error(&notes, "ext.windows.list", Value::Null).code, ErrorCode::Invalid);
        let nowhere = From { dir: bench.dir.join("nowhere").to_string_lossy().into_owned(), ..tiles.clone() };
        assert_eq!(bench.error(&nowhere, "ext.windows.list", Value::Null).code, ErrorCode::Invalid);
        // A name that would not stay inside the cache directory, whatever
        // the manifest there says.
        for (folder, name) in [("a", ""), ("b", "../tiles"), ("c", "a/b"), ("d", ".hidden"), ("e", "a\\b")] {
            let odd = bench.install_manifest(folder, json!({ "name": name, "heyo": { "permissions": ["windows"] } }));
            assert_eq!(bench.error(&odd, "ext.windows.list", Value::Null).code, ErrorCode::Invalid, "{name:?}");
        }
        // One that does not say who asks is nobody's.
        let anonymous = bench.send(json!({ "type": "request", "id": 1, "method": "ext.supported", "params": { "module": "files" } }), false);
        assert_eq!(anonymous.answer.unwrap_err().message, "ext.supported does not say which extension asks");
        assert_eq!(bench.asked(), Vec::<String>::new());
    }

    #[test]
    fn heyos_own_commands_come_from_no_directory_and_are_never_asked_about() {
        let bench = Bench::new(&[(1, "firefox")]);
        let heyo = From::heyo();
        assert_eq!(bench.result(&heyo, "ext.windows.list", Value::Null)[0]["id"], 1);
        assert_eq!(bench.result(&heyo.in_background(), "ext.windows.list", Value::Null)[0]["id"], 1);
        assert_eq!(bench.asked(), Vec::<String>::new());

        // An extension that calls itself by Heyo's name is not Heyo, whatever
        // its manifest says, and is nobody else either.
        let pretender = bench.install_manifest("@heyo", json!({ "name": "@heyo", "heyo": { "permissions": ["windows"] } }));
        assert_eq!(bench.error(&pretender, "ext.windows.list", Value::Null).code, ErrorCode::Invalid);
        assert_eq!(bench.error(&From { dir: ".".into(), ..heyo.clone() }, "ext.windows.list", Value::Null).code, ErrorCode::Invalid);
        // Nor is one that says it comes from nowhere.
        let tiles = bench.install("tiles", &["windows"]);
        assert_eq!(bench.error(&From { dir: String::new(), ..tiles }, "ext.windows.list", Value::Null).code, ErrorCode::Invalid);
        assert_eq!(bench.asked(), Vec::<String>::new());
    }

    #[test]
    fn a_request_heyo_does_not_know_is_refused() {
        let bench = Bench::new(&[]);
        let heyo = From::heyo();
        let unknown = bench.error(&heyo, "ext.windows.teleport", json!({ "id": 1 }));
        assert_eq!(unknown.code, ErrorCode::Invalid);
        assert!(unknown.message.starts_with("Heyo cannot answer ext.windows.teleport"), "{}", unknown.message);
        // One it knows, without what it takes.
        assert_eq!(bench.error(&heyo, "ext.windows.activate", json!({})).code, ErrorCode::Invalid);
        // One that takes nothing may come with an empty set of parameters.
        assert_eq!(bench.result(&heyo, "ext.windows.list", json!({})), json!([]));
        assert_eq!(bench.desktop.asked(), Vec::<String>::new());
    }

    #[test]
    fn what_a_manifest_does_not_list_is_refused_without_asking() {
        let bench = Bench::new(&[(1, "firefox")]);
        let notes = bench.install("notes", &["memory.read"]);
        let refused = bench.error(&notes, "ext.windows.close", json!({ "id": 1 }));
        assert_eq!(refused.code, ErrorCode::Denied);
        assert!(refused.message.contains("needs the \"windows\" permission"), "{}", refused.message);
        // An extension from the store lists nothing, and reaches windows
        // only through what Raycast's API has for them.
        let store = bench.install_manifest("store", json!({ "name": "store", "title": "Store" }));
        assert_eq!(bench.error(&store, "ext.windows.close", json!({ "id": 1 })).code, ErrorCode::Denied);
        assert_eq!(bench.error(&store.through_raycast(), "ext.memory.search", json!({})).code, ErrorCode::Denied);
        assert_eq!(bench.asked(), Vec::<String>::new());
        assert_eq!(bench.result(&store.through_raycast(), "ext.windows.close", json!({ "id": 1 })), Value::Null);
        assert_eq!(bench.asked(), ["ask Allow Store to see, switch, move and close the windows of other applications?"]);
        // Nothing was done for the ones refused.
        assert_eq!(bench.desktop.asked(), ["close 1"]);
    }

    #[test]
    fn the_user_is_asked_once_and_what_they_allow_is_kept() {
        let bench = Bench::new(&[(1, "firefox")]);
        let tiles = bench.install("tiles", &["windows", "system"]);
        assert_eq!(bench.result(&tiles, "ext.windows.list", Value::Null)[0]["id"], 1);
        assert_eq!(bench.asked(), [QUESTION]);
        // Allowed from then on, for every request that needs it, and in the
        // background, where nobody could be asked.
        bench.app.says.set(false);
        bench.result(&tiles, "ext.windows.list", Value::Null);
        bench.result(&tiles, "ext.windows.close", json!({ "id": 1 }));
        bench.result(&tiles.in_background(), "ext.windows.list", Value::Null);
        assert_eq!(bench.asked(), Vec::<String>::new());
        // One permission is not another, nor one extension another.
        assert_eq!(bench.error(&tiles, "ext.system.run", json!({ "action": "lock-screen" })).code, ErrorCode::Denied);
        let other = bench.install("other", &["windows"]);
        assert_eq!(bench.error(&other, "ext.windows.list", Value::Null).code, ErrorCode::Denied);
        assert_eq!(bench.asked().len(), 2);
    }

    #[test]
    fn a_no_is_told_the_extension_and_holds_for_a_while() {
        let bench = Bench::new(&[(1, "firefox")]);
        let tiles = bench.install("tiles", &["windows"]);
        bench.app.says.set(false);
        let refused = bench.error(&tiles, "ext.windows.list", Value::Null);
        assert_eq!((refused.code, refused.message.as_str()), (ErrorCode::Denied, "Tiles was not allowed to see, switch, move and close the windows of other applications."));
        // Asking again at once does not put the question back up.
        bench.app.says.set(true);
        assert_eq!(bench.error(&tiles, "ext.windows.list", Value::Null).code, ErrorCode::Denied);
        assert_eq!(bench.asked(), [QUESTION]);
        assert_eq!(bench.desktop.asked(), Vec::<String>::new());
    }

    #[test]
    fn nobody_is_asked_from_the_background() {
        let bench = Bench::new(&[(1, "firefox")]);
        let tiles = bench.install("tiles", &["windows"]);
        let refused = bench.error(&tiles.in_background(), "ext.windows.list", Value::Null);
        assert_eq!(refused.code, ErrorCode::Denied);
        assert!(refused.message.contains("Open one of its commands in Heyo to be asked"), "{}", refused.message);
        assert_eq!(bench.asked(), Vec::<String>::new());
    }

    #[test]
    fn windows_are_told_with_the_one_the_user_works_in_and_their_monitor() {
        let bench = Bench::new(&[(1, "firefox"), (2, "Code"), (3, "org.gnome.Ptyxis")]);
        bench.desktop.windows.lock().unwrap()[1].bounds = Some(Rect { x: 2000, y: 100, width: 800, height: 600 });
        bench.desktop.windows.lock().unwrap()[2].bounds = Some(Rect { x: -9000, y: 0, width: 10, height: 10 });
        let heyo = From::heyo();
        // Most recently used first, as the desktop lists them. Where it does
        // not say where a window is, neither does Heyo.
        assert_eq!(
            bench.result(&heyo, "ext.windows.list", Value::Null),
            json!([
                { "id": 1, "title": "firefox 1", "class": "firefox", "active": true },
                { "id": 2, "title": "Code 2", "class": "Code", "active": false, "bounds": { "x": 2000, "y": 100, "width": 800, "height": 600 }, "monitor": 1 },
                // Off every monitor: the first.
                { "id": 3, "title": "org.gnome.Ptyxis 3", "class": "org.gnome.Ptyxis", "active": false, "bounds": { "x": -9000, "y": 0, "width": 10, "height": 10 }, "monitor": 0 },
            ])
        );
        assert_eq!(bench.result(&heyo, "ext.windows.active", Value::Null)["id"], 1);
        assert_eq!(bench.result(&heyo, "ext.windows.monitors", Value::Null), json!([LEFT, RIGHT]));
        // With no monitor known, a window is on none.
        bench.app.monitors.borrow_mut().clear();
        assert!(bench.result(&heyo, "ext.windows.list", Value::Null)[1].get("monitor").is_none());
    }

    #[test]
    fn the_window_the_user_works_in_is_the_one_before_the_palette() {
        let bench = Bench::new(&[(1, "firefox"), (2, "Code")]);
        let heyo = From::heyo();
        // With the palette in front the desktop calls no window of another
        // application active: the one remembered is it.
        *bench.desktop.heyo_in_front.lock().unwrap() = true;
        assert_eq!(bench.result(&heyo, "ext.windows.active", Value::Null), Value::Null);
        assert_eq!(bench.result(&heyo, "ext.frontmostApplication", Value::Null), Value::Null);
        *bench.last_window.0.lock().unwrap() = Some(2);
        assert_eq!(bench.result(&heyo, "ext.windows.active", Value::Null)["id"], 2);
        let active: Vec<bool> = bench.result(&heyo, "ext.windows.list", Value::Null).as_array().unwrap().iter().map(|window| window["active"] == true).collect();
        assert_eq!(active, [false, true]);
        // A window of a program nothing installed describes still has a name.
        assert_eq!(bench.result(&heyo, "ext.frontmostApplication", Value::Null), json!({ "name": "Code", "localizedName": "Code", "path": "" }));

        // Otherwise it is the one in front.
        *bench.desktop.heyo_in_front.lock().unwrap() = false;
        let firefox = crate::apps::App {
            id: "org.mozilla.firefox.desktop".into(),
            name: "Firefox".into(),
            generic_name: None,
            comment: None,
            keywords: Vec::new(),
            exec: "firefox %u".into(),
            icon: None,
            terminal: false,
            working_dir: None,
            path: "/usr/share/applications/org.mozilla.firefox.desktop".into(),
            startup_wm_class: None,
        };
        *bench.index.write().unwrap() = Index::new(vec![firefox], Vec::new(), Vec::new(), Vec::new());
        let frontmost = bench.result(&heyo, "ext.frontmostApplication", Value::Null);
        assert_eq!((frontmost["name"].as_str(), frontmost["bundleId"].as_str()), (Some("Firefox"), Some("org.mozilla.firefox.desktop")));
    }

    #[test]
    fn the_desktop_is_asked_to_do_what_an_extension_asked() {
        let bench = Bench::new(&[(1, "firefox"), (2, "Code")]);
        let tiles = bench.install("tiles", &["windows"]);
        bench.result(&tiles, "ext.windows.arrange", json!({ "id": 2, "layout": "right-half" }));
        bench.result(&tiles, "ext.windows.arrange", json!({ "id": 2, "layout": { "fraction": [3, 0, 6, 12] } }));
        bench.result(&tiles, "ext.windows.setBounds", json!({ "id": 2, "bounds": { "x": 10, "y": 20, "width": 300, "height": 200 } }));
        bench.result(&tiles, "ext.windows.close", json!({ "id": 1 }));
        assert_eq!(
            bench.desktop.asked(),
            ["arrange 2 Fraction(6, 0, 6, 12)", "arrange 2 Fraction(3, 0, 6, 12)", "arrange 2 Exact(Rect { x: 10, y: 20, width: 300, height: 200 })", "close 1"]
        );
        // What the desktop says went wrong is passed on.
        let gone = bench.error(&tiles, "ext.windows.activate", json!({ "id": 1 }));
        assert_eq!((gone.code, gone.message.as_str()), (ErrorCode::Failed, "the window is gone"));
    }

    #[test]
    fn a_layout_is_one_of_the_named_or_a_part_of_the_work_area() {
        let bench = Bench::new(&[(1, "firefox")]);
        let heyo = From::heyo();
        // Every layout the specification names is one the host has.
        let named = [
            "left-half", "right-half", "top-half", "bottom-half", "top-left", "top-right", "bottom-left", "bottom-right", "first-third", "center-third", "last-third",
            "first-two-thirds", "last-two-thirds", "almost-maximize", "maximize", "center", "next-display",
        ];
        for name in named {
            bench.result(&heyo, "ext.windows.arrange", json!({ "id": 1, "layout": name }));
        }
        assert_eq!(named.len(), windows::LAYOUTS.len());
        assert_eq!(bench.desktop.asked()[0], "arrange 1 Fraction(0, 0, 6, 12)");
        assert_eq!(bench.desktop.asked()[14], "arrange 1 Maximize");

        // What is no layout, or no size for a window, is not put to the desktop.
        assert_eq!(bench.error(&heyo, "ext.windows.arrange", json!({ "id": 1, "layout": "sideways" })).code, ErrorCode::Invalid);
        for outside in [[-1, 0, 6, 12], [0, 0, 0, 12], [8, 0, 6, 12], [0, 6, 6, 7], [0, 0, 13, 1]] {
            assert_eq!(bench.error(&heyo, "ext.windows.arrange", json!({ "id": 1, "layout": { "fraction": outside } })).code, ErrorCode::Invalid, "{outside:?}");
        }
        for (width, height) in [(0, 200), (300, 0), (-300, 200)] {
            let bounds = json!({ "x": 0, "y": 0, "width": width, "height": height });
            assert_eq!(bench.error(&heyo, "ext.windows.setBounds", json!({ "id": 1, "bounds": bounds })).code, ErrorCode::Invalid);
        }
        assert_eq!(bench.desktop.asked().len(), named.len());
    }

    #[test]
    fn the_palette_makes_way_for_a_window_unless_a_background_command_asks() {
        let bench = Bench::new(&[(1, "firefox"), (2, "Code")]);
        let heyo = From::heyo();
        bench.result(&heyo, "ext.windows.activate", json!({ "id": 2 }));
        assert_eq!(bench.asked(), ["hide palette"]);
        // The palette is the user's, and the command's that is in it.
        bench.result(&heyo.in_background(), "ext.windows.activate", json!({ "id": 1 }));
        assert_eq!(bench.asked(), Vec::<String>::new());
        assert_eq!(bench.desktop.asked(), ["activate 2", "activate 1"]);
    }

    #[test]
    fn a_session_that_cannot_reach_windows_says_so_for_each_request() {
        let bench = Bench::new(&[(1, "firefox")]);
        *bench.desktop.supported.lock().unwrap() = Some(Abilities::NOTHING);
        let heyo = From::heyo();
        let bounds = json!({ "x": 0, "y": 0, "width": 300, "height": 200 });
        let requests = [
            ("ext.windows.list", Value::Null),
            ("ext.windows.active", Value::Null),
            ("ext.windows.activate", json!({ "id": 1 })),
            ("ext.windows.close", json!({ "id": 1 })),
            ("ext.windows.arrange", json!({ "id": 1, "layout": "center" })),
            ("ext.windows.setBounds", json!({ "id": 1, "bounds": bounds })),
            ("ext.frontmostApplication", Value::Null),
        ];
        for (method, params) in requests {
            assert_eq!(bench.error(&heyo, method, params).code, ErrorCode::Unsupported, "{method}");
        }
        // Nothing was tried, and the palette stayed.
        assert_eq!((bench.desktop.asked(), bench.asked()), (Vec::new(), Vec::new()));
        // The monitors are not the desktop's to refuse.
        assert_eq!(bench.result(&heyo, "ext.windows.monitors", Value::Null), json!([LEFT, RIGHT]));
        assert_eq!(bench.result(&heyo, "ext.supported", json!({ "module": "windows" })), "none");
    }

    #[test]
    fn the_screen_and_pasting_are_for_sessions_that_can() {
        let bench = Bench::new(&[(1, "firefox")]);
        let heyo = From::heyo();
        // The usual fake pastes and cannot read the screen.
        assert_eq!(bench.error(&heyo, "ext.screen.capture", json!({ "target": "screen" })).code, ErrorCode::Unsupported);
        assert_eq!(bench.result(&heyo, "ext.clipboard.paste", json!({ "text": "Hello" })), Value::Null);
        assert_eq!(bench.asked(), ["paste Hello"]);

        *bench.desktop.supported.lock().unwrap() = Some(Abilities { capture: Support::Limited, ..Abilities::NOTHING });
        assert_eq!(bench.error(&heyo, "ext.clipboard.paste", json!({ "text": "Hello" })).code, ErrorCode::Unsupported);
        let capture = bench.result(&heyo, "ext.screen.capture", json!({ "target": { "window": 1 } }));
        assert_eq!(capture, json!({ "path": "/cache/@heyo/captures/1.png", "width": 4, "height": 3 }));
        assert_eq!(bench.asked(), ["capture Window { window: 1 } for @heyo"]);

        // An area the user selects is theirs to give by selecting it; the
        // rest of the screen is asked about.
        let shots = bench.install("shots", &["screen.capture"]);
        bench.result(&shots, "ext.screen.capture", json!({ "target": "selection" }));
        assert_eq!(bench.asked(), ["capture Whole(Selection) for shots"]);
        bench.result(&shots, "ext.screen.capture", json!({ "target": "screen" }));
        assert_eq!(bench.asked(), ["ask Allow Shots to take pictures of the screen without you selecting what?", "capture Whole(Screen) for shots"]);

        // Text is read out of a file there is.
        assert_eq!(bench.error(&heyo, "ext.screen.recognizeText", json!({ "path": bench.dir.join("none.png") })).code, ErrorCode::Invalid);
    }

    #[test]
    fn each_part_of_the_api_says_how_well_the_session_supports_it() {
        let bench = Bench::new(&[]);
        let anyone = bench.install("anyone", &[]);
        let support = |module: &str| bench.result(&anyone, "ext.supported", json!({ "module": module }));
        assert_eq!((support("windows"), support("screen")), (json!("full"), json!("none")));
        // The history is kept either way; without a watcher it is polled.
        assert_eq!((support("clipboardHistory"), support("memory")), (json!("limited"), json!("limited")));
        assert_eq!((support("files"), support("system"), support("ai")), (json!("full"), json!("full"), json!("full")));
        assert_eq!(support("dictation"), if cfg!(target_os = "linux") { "full" } else { "none" });
        assert_eq!(support("activity"), "limited");

        *bench.desktop.supported.lock().unwrap() = Some(Abilities { capture: Support::Limited, clipboard_watch: Support::Full, ..Abilities::NOTHING });
        assert_eq!((support("windows"), support("screen"), support("memory")), (json!("none"), json!("limited"), json!("full")));
        // How activities show is told to whoever asks.
        assert_eq!(
            bench.result(&anyone, "ext.activity.presentation", Value::Null),
            json!({ "support": "none", "compactCharacters": 10, "expanded": false, "buttons": false, "version": 1 })
        );
        assert_eq!(bench.asked(), Vec::<String>::new());
    }

    #[test]
    fn what_ends_the_session_is_confirmed_every_time() {
        let bench = Bench::new(&[]);
        let power = bench.install("power", &["system"]);
        // What does not end it needs the permission, once, and no more.
        bench.result(&power, "ext.system.run", json!({ "action": "lock-screen" }));
        bench.result(&power, "ext.system.run", json!({ "action": "volume-up" }));
        assert_eq!(bench.asked().len(), 1);

        for (action, question, done) in [("restart", "Restart the computer?", "system Restart"), ("shut-down", "Shut the computer down?", "system ShutDown"), ("log-out", "Log out?", "system LogOut")] {
            *bench.desktop.asked.lock().unwrap() = Vec::new();
            bench.app.says.set(false);
            assert_eq!(bench.error(&power, "ext.system.run", json!({ "action": action })).code, ErrorCode::Cancelled);
            assert_eq!(bench.desktop.asked(), Vec::<String>::new());
            bench.app.says.set(true);
            bench.result(&power, "ext.system.run", json!({ "action": action }));
            assert_eq!(bench.asked(), [format!("ask {question}"), format!("ask {question}")]);
            assert_eq!(bench.desktop.asked(), [done]);
            // Nobody is there to confirm it in the background.
            assert_eq!(bench.error(&power.in_background(), "ext.system.run", json!({ "action": action })).code, ErrorCode::Denied);
            // A command of Heyo's has asked already, in its own words.
            bench.result(&From::heyo(), "ext.system.run", json!({ "action": action }));
            assert_eq!(bench.asked(), Vec::<String>::new());
            assert_eq!(bench.desktop.asked(), [done, done]);
        }
    }

    #[test]
    fn a_notification_and_the_sound_devices_are_the_desktops() {
        let bench = Bench::new(&[]);
        let heyo = From::heyo();
        let told = bench.ask(&heyo, "ext.system.notify", json!({ "title": "Done", "body": "3 files" }));
        assert_eq!((told.answer, told.effects), (Ok(Value::Null), vec![Effect::Notify { title: "Done".into(), body: "3 files".into() }]));
        let told = bench.ask(&heyo, "ext.system.notify", json!({ "title": "Done" }));
        assert_eq!(told.effects, [Effect::Notify { title: "Done".into(), body: String::new() }]);
        assert_eq!(bench.result(&heyo, "ext.system.audioDevices", Value::Null), json!([]));
        bench.result(&heyo, "ext.system.useAudioDevice", json!({ "kind": "input", "name": "mic" }));
        assert_eq!(bench.desktop.asked(), ["audio devices", "use Input mic"]);
    }

    #[test]
    fn the_model_is_said_without_its_key() {
        let bench = Bench::new(&[]);
        let writer = bench.install("writer", &["ai"]);
        bench.settings.write().unwrap().anthropic_api_key = "sk-secret".into();
        assert_eq!(bench.result(&writer, "ext.ai.provider", Value::Null), json!({ "kind": "claude", "model": "", "local": false }));
        {
            let mut settings = bench.settings.write().unwrap();
            settings.ai_provider = "local".into();
            settings.local_model_name = "qwen3".into();
        }
        assert_eq!(bench.result(&writer, "ext.ai.provider", Value::Null), json!({ "kind": "local", "model": "qwen3", "local": true }));
        // Listing it is enough.
        assert_eq!(bench.asked(), Vec::<String>::new());
    }

    fn start(id: &str, launch_type: Option<&str>) -> Value {
        json!({ "attributes": { "id": id, "name": "Build", "icon": "hammer-16" }, "content": { "title": "Building" }, "launchType": launch_type, "seq": 1 })
    }

    #[test]
    fn only_a_command_the_user_ran_starts_an_activity() {
        let bench = Bench::new(&[]);
        let builder = bench.install("builder", &["activity"]);
        // One run on a schedule cannot, in whichever process it runs.
        let refused = bench.ask(&builder, "ext.activity.start", start("build", Some("background")));
        assert_eq!((refused.answer.unwrap_err().code, refused.effects), (ErrorCode::Denied, Vec::new()));
        assert_eq!(bench.result(&builder, "ext.activity.list", Value::Null), json!([]));

        let started = bench.ask(&builder.in_background(), "ext.activity.start", start("build", Some("userInitiated")));
        assert_eq!(started.answer, Ok(json!({ "id": "build" })));
        // The user is told, the palette being away or not.
        let [Effect::Activities(called_for)] = &started.effects[..] else { panic!("{:?}", started.effects) };
        assert!(matches!(&called_for[..], [activities::Effect::Notify { key, .. }] if key == "builder/build"), "{called_for:?}");
        // A command that does not say how it was launched was run by the user.
        assert_eq!(bench.result(&builder, "ext.activity.start", start("tests", None)), json!({ "id": "tests" }));
        assert_eq!(bench.result(&builder, "ext.activity.list", Value::Null), json!(["build", "tests"]));

        // Once started, one in the background may go on with it.
        let updated = bench.ask(&builder, "ext.activity.start", start("build", Some("background")));
        assert_eq!((updated.answer, updated.effects), (Ok(json!({ "id": "build" })), vec![Effect::Activities(Vec::new())]));
        let updated = bench.ask(&builder, "ext.activity.update", json!({ "id": "build", "content": { "subtitle": "linking" }, "seq": 3 }));
        assert_eq!((updated.answer, updated.effects), (Ok(Value::Null), vec![Effect::Activities(Vec::new())]));

        // An extension has its own and no other's.
        let other = bench.install("other", &["activity"]);
        assert_eq!(bench.result(&other, "ext.activity.get", json!({ "id": "build" })), false);
        assert_eq!(bench.error(&other, "ext.activity.end", json!({ "id": "build" })).code, ErrorCode::Invalid);
        assert_eq!(bench.result(&builder, "ext.activity.get", json!({ "id": "build" })), true);
        bench.result(&builder, "ext.activity.end", json!({ "id": "build", "keep": "none" }));
        assert_eq!(bench.result(&builder, "ext.activity.list", Value::Null), json!(["tests"]));
        // Listing the permission is enough.
        assert_eq!(bench.asked(), Vec::<String>::new());
    }

    #[test]
    fn activities_turned_off_in_the_settings_are_refused() {
        let bench = Bench::new(&[]);
        let builder = bench.install("builder", &["activity"]);
        bench.settings.write().unwrap().activities_off = vec!["builder".into()];
        let off = bench.error(&builder, "ext.activity.start", start("build", None));
        assert_eq!(off.code, ErrorCode::Denied);
        assert!(off.message.contains("turned off in Heyo Settings"), "{}", off.message);
        // For that extension alone, until they are off for all: Heyo's too.
        assert_eq!(bench.result(&From::heyo(), "ext.activity.start", start("build", None)), json!({ "id": "build" }));
        bench.settings.write().unwrap().live_activities = false;
        assert_eq!(bench.error(&From::heyo(), "ext.activity.list", Value::Null).code, ErrorCode::Denied);
        // How they would show can still be asked.
        assert_eq!(bench.result(&builder, "ext.activity.presentation", Value::Null)["version"], 1);
    }

    #[test]
    fn a_command_listens_for_its_activities_until_it_says_or_its_process_ends() {
        let bench = Bench::new(&[]);
        let builder = bench.install("builder", &["activity"]);
        let listeners = || bench.activities.lock().unwrap().listeners_of("builder");
        let first = bench.result(&builder, "ext.activity.onEvent", Value::Null)["subscription"].as_u64().unwrap();
        let second = bench.result(&builder.in_background(), "ext.activity.onEvent", Value::Null)["subscription"].as_u64().unwrap();
        assert_ne!(first, second);
        assert_eq!(listeners(), [first, second]);
        // Its events go to the process it runs in.
        let listener = |subscription| bench.subscriptions.lock().unwrap().listener(subscription);
        assert_eq!((listener(first), listener(second)), (Some(false), Some(true)));

        // The other process cannot end it; its own can, once.
        bench.result(&builder.in_background(), "ext.unsubscribe", json!({ "subscription": first }));
        assert_eq!(listeners(), [first, second]);
        let ended = bench.ask(&builder, "ext.unsubscribe", json!({ "subscription": first }));
        assert_eq!((ended.answer, ended.effects), (Ok(Value::Null), Vec::new()));
        assert_eq!(listeners(), [second]);
        assert_eq!(listener(first), None);
        bench.result(&builder, "ext.unsubscribe", json!({ "subscription": first }));

        // A process that ends takes its subscriptions with it, and only its own.
        let third = bench.result(&builder, "ext.activity.onEvent", Value::Null)["subscription"].as_u64().unwrap();
        assert_eq!(runtime_exited(&bench.world(), true), Vec::new());
        assert_eq!(listeners(), [third]);
        assert_eq!(runtime_exited(&bench.world(), false), Vec::new());
        assert_eq!(listeners(), Vec::<u64>::new());
    }

    #[test]
    fn a_recording_is_subscribed_to_and_ends_with_its_subscription() {
        let bench = Bench::new(&[]);
        let heyo = From::heyo();
        if cfg!(not(target_os = "linux")) {
            assert_eq!(bench.error(&heyo, "ext.dictation.listen", json!({})).code, ErrorCode::Unsupported);
            return;
        }
        let subscription = bench.result(&heyo, "ext.dictation.listen", json!({ "language": "en" }))["subscription"].as_u64().unwrap();
        assert_eq!(bench.asked(), [format!("listen for {subscription} in en shown")]);
        // Stopping it is for the process that listens: its text comes as an event.
        assert_eq!(bench.ask(&heyo.in_background(), "ext.dictation.stop", json!({ "subscription": subscription })).effects, Vec::new());
        assert_eq!(bench.ask(&heyo, "ext.dictation.stop", json!({ "subscription": subscription })).effects, [Effect::StopListening(7)]);
        assert_eq!(bench.subscriptions.lock().unwrap().listener(subscription), Some(false));
        // Ending the subscription discards the recording.
        assert_eq!(bench.ask(&heyo, "ext.unsubscribe", json!({ "subscription": subscription })).effects, [Effect::CancelListening(7)]);
        assert_eq!(bench.ask(&heyo, "ext.unsubscribe", json!({ "subscription": subscription })).effects, Vec::new());
        assert_eq!(bench.ask(&heyo, "ext.dictation.stop", json!({ "subscription": subscription })).effects, Vec::new());

        // So does the end of the process that listened.
        *bench.app.recording.borrow_mut() = Ok(8);
        bench.result(&heyo.in_background(), "ext.dictation.listen", json!({ "indicator": false }));
        assert_eq!(bench.asked().last().unwrap(), &format!("listen for {} in the language set unseen", subscription + 1));
        assert_eq!(runtime_exited(&bench.world(), false), Vec::new());
        assert_eq!(runtime_exited(&bench.world(), true), [Effect::CancelListening(8)]);
        assert_eq!(runtime_exited(&bench.world(), true), Vec::new());

        // A recording that does not start leaves no subscription behind.
        *bench.app.recording.borrow_mut() = Err(ExtError::from("A recording is already in progress."));
        assert_eq!(bench.error(&heyo, "ext.dictation.listen", json!({})).message, "A recording is already in progress.");
        assert_eq!(bench.subscriptions.lock().unwrap().listener(subscription + 2), None);
        // Nor is one started in a language dictation does not know.
        bench.asked();
        assert_eq!(bench.error(&heyo, "ext.dictation.listen", json!({ "language": "tlh" })).code, ErrorCode::Invalid);
        assert_eq!(bench.asked(), Vec::<String>::new());
    }

    #[test]
    fn the_clipboard_history_is_read_for_those_allowed() {
        let bench = Bench::new(&[]);
        bench.history.record_text("a note about lunch".into(), Some("firefox".into()), 90);
        bench.history.record_text("https://example.com".into(), None, 90);
        let reader = bench.install("reader", &["memory.read"]);
        let found = bench.result(&reader, "ext.memory.search", json!({}));
        assert_eq!(bench.asked(), ["ask Allow Reader to search what Heyo remembers, which includes your clipboard history?"]);
        let mut ids: Vec<&str> = found["items"].as_array().unwrap().iter().map(|item| item["id"].as_str().unwrap()).collect();
        ids.sort();
        assert_eq!(ids, ["clipboard:1", "clipboard:2"]);
        assert_eq!((&found["understood"], &found["relaxed"]), (&json!({ "terms": [] }), &json!(false)));

        let found = bench.result(&reader, "ext.memory.search", json!({ "text": "lunch" }));
        assert_eq!(found["understood"]["terms"], json!(["lunch"]));
        let [note] = &found["items"].as_array().unwrap()[..] else { panic!("{found}") };
        // The window class stands for an application nothing installed describes.
        assert_eq!((&note["id"], &note["source"], &note["kind"], &note["text"], &note["appName"]), (&json!("clipboard:1"), &json!("clipboard"), &json!("text"), &json!("a note about lunch"), &json!("firefox")));
        assert_eq!(bench.result(&reader, "ext.memory.search", json!({ "kinds": ["link"] }))["items"][0]["text"], "https://example.com");
        assert_eq!(bench.result(&reader, "ext.memory.search", json!({ "limit": 1 }))["items"].as_array().unwrap().len(), 1);
        // The clipboard is the one source there is.
        assert_eq!(bench.result(&reader, "ext.memory.search", json!({ "sources": ["notes"] }))["items"], json!([]));
        assert_eq!(bench.result(&reader, "ext.memory.search", json!({ "sources": ["notes", "clipboard"], "text": "lunch" }))["items"].as_array().unwrap().len(), 1);
        assert_eq!(bench.asked(), Vec::<String>::new());

        // What the words mean is asked for only when there are words, and
        // where that has nothing to add the words' answer it is.
        bench.result(&reader, "ext.memory.search", json!({ "byMeaning": true }));
        assert_eq!(bench.asked(), Vec::<String>::new());
        let found = bench.result(&reader, "ext.memory.search", json!({ "text": "lunch", "byMeaning": true }));
        assert_eq!(bench.asked(), ["meant lunch"]);
        assert_eq!(found["items"][0]["id"], "clipboard:1");

        assert_eq!(bench.result(&reader, "ext.memory.get", json!({ "id": "clipboard:2" }))["text"], "https://example.com");
        assert_eq!(bench.result(&reader, "ext.memory.get", json!({ "id": "clipboard:9" })), Value::Null);
        assert_eq!(bench.error(&reader, "ext.memory.get", json!({ "id": "9" })).code, ErrorCode::Invalid);

        // Putting an entry back on the clipboard is another permission.
        assert_eq!(bench.error(&reader, "ext.clipboardHistory.copy", json!({ "id": "clipboard:1" })).code, ErrorCode::Denied);
        let heyo = From::heyo();
        assert_eq!(bench.error(&heyo, "ext.clipboardHistory.copy", json!({ "id": "clipboard:9" })).code, ErrorCode::Invalid);
        bench.result(&heyo, "ext.clipboardHistory.copy", json!({ "id": "clipboard:1" }));
        assert_eq!(bench.asked(), ["copy 1"]);
    }

    #[test]
    fn a_subscription_whose_source_has_not_started_leaves_nothing_to_stop() {
        let mut subscriptions = Subscriptions::new();
        let (starting, recording, done) = (subscriptions.open(false), subscriptions.open(false), subscriptions.open(false));
        subscriptions.attach(recording, Source::Recording(7));
        assert_eq!((subscriptions.recording(starting, false), subscriptions.recording(recording, false)), (None, Some(7)));
        // A source that is done leaves nothing to stop, nor to attach to.
        subscriptions.ended(done);
        subscriptions.attach(done, Source::Recording(9));
        assert_eq!(subscriptions.listener(done), None);
        assert_eq!(subscriptions.close(starting, false), None);
        assert_eq!(subscriptions.listener(starting), None);
        let starting = subscriptions.open(false);
        assert_eq!(subscriptions.close_all(false), [(recording, Source::Recording(7))]);
        assert_eq!((subscriptions.listener(starting), subscriptions.listener(recording)), (None, None));
    }

    fn entry(id: u64, kind: &str, text: Option<&str>, at: u64) -> Shown {
        let image = (kind == "image").then(|| format!("/data/clipboard/images/{id}.png"));
        let size = image.as_ref().map(|_| 640);
        let entry = serde_json::from_value(json!({
            "id": id, "kind": kind, "text": text, "image": image, "width": size, "height": size,
            "app": "firefox", "copiedAt": at, "firstCopiedAt": at / 2, "timesCopied": 3, "pinned": id == 2, "hash": id,
        }))
        .unwrap();
        Shown { entry, app_name: Some("Firefox".into()), app_icon: Some("/usr/share/icons/firefox.png".into()), by_meaning: id == 5 }
    }

    #[test]
    fn an_entry_of_the_history_is_told_as_something_remembered() {
        assert_eq!(
            item(&entry(2, "link", Some("https://example.com"), 1_700_000_000_000)),
            MemoryItem {
                id: "clipboard:2".into(),
                source: "clipboard",
                kind: Kind::Link,
                text: Some("https://example.com".into()),
                image: None,
                width: None,
                height: None,
                app: Some("firefox".into()),
                app_name: Some("Firefox".into()),
                app_icon: Some("/usr/share/icons/firefox.png".into()),
                at: 1_700_000_000_000,
                first_at: Some(850_000_000_000),
                times_copied: Some(3),
                pinned: true,
                by_meaning: None,
            }
        );
        // All that Heyo's own view of the history shows of an entry is told.
        let image = serde_json::to_value(item(&entry(5, "image", None, 10))).unwrap();
        assert_eq!(
            image,
            json!({
                "id": "clipboard:5", "source": "clipboard", "kind": "image", "image": "/data/clipboard/images/5.png", "width": 640, "height": 640,
                "app": "firefox", "appName": "Firefox", "appIcon": "/usr/share/icons/firefox.png",
                "at": 10, "firstAt": 5, "timesCopied": 3, "pinned": false, "byMeaning": true,
            })
        );
        // An entry from nowhere known, found by its words.
        let plain = serde_json::to_value(item(&Shown { app_name: None, app_icon: None, ..entry(1, "text", Some("a"), 2) })).unwrap();
        assert!(["appName", "appIcon", "byMeaning", "width", "image"].iter().all(|field| plain.get(field).is_none()));

        assert_eq!(entry_id("clipboard:412").unwrap(), 412);
        for other in ["412", "clipboard:", "clipboard:x", "ext:notes:3", ""] {
            assert_eq!(entry_id(other).unwrap_err().code, ErrorCode::Invalid, "{other:?}");
        }
    }

    #[test]
    fn a_search_gives_what_was_asked_for_and_never_everything() {
        let entries = || vec![entry(4, "text", Some("d"), 400), entry(3, "link", Some("c"), 300), entry(2, "image", None, 200), entry(1, "text", Some("a"), 100)];
        let ids = |filter: Filter| chosen(entries(), &filter).into_iter().map(|item| item.id).collect::<Vec<_>>();
        assert_eq!(ids(Filter::default()), ["clipboard:4", "clipboard:3", "clipboard:2", "clipboard:1"]);
        assert_eq!(ids(Filter { kinds: Some(vec![Kind::Text]), ..Filter::default() }), ["clipboard:4", "clipboard:1"]);
        assert_eq!(ids(Filter { kinds: Some(vec![Kind::Link, Kind::Image]), ..Filter::default() }), ["clipboard:3", "clipboard:2"]);
        // From `since` up to, not including, `until`.
        assert_eq!(ids(Filter { since: Some(200), until: Some(400), ..Filter::default() }), ["clipboard:3", "clipboard:2"]);
        assert_eq!(ids(Filter { limit: Some(1), ..Filter::default() }), ["clipboard:4"]);
        assert_eq!(ids(Filter { limit: Some(0), ..Filter::default() }), Vec::<String>::new());

        // As many as Heyo's own view shows, which is all a search finds.
        assert_eq!((limit(None), limit(Some(7)), limit(Some(300)), limit(Some(100_000))), (50, 7, 300, 300));
        assert_eq!(limit(Some(u32::MAX)), clipboard::SHOWN);
        let many = || (0..400).map(|n| entry(n, "text", Some("x"), n)).collect::<Vec<_>>();
        assert_eq!(chosen(many(), &Filter::default()).len(), 50);
        assert_eq!(chosen(many(), &Filter { limit: Some(u32::MAX), ..Filter::default() }).len(), 300);
    }

    #[test]
    fn files_are_told_by_path_and_no_more_of_them_than_asked() {
        let found = json!({ "files": [{ "path": "/home/u/a.txt", "isDirectory": false }, { "path": "/home/u/b", "isDirectory": true }], "scanning": true, "total": 90_000 });
        assert_eq!(first_files(found.clone(), None), json!({ "files": found["files"], "scanning": true, "total": 90_000 }));
        assert_eq!(first_files(found.clone(), Some(1))["files"], json!([{ "path": "/home/u/a.txt", "isDirectory": false }]));
        // How many there are to search is not how many were asked for.
        assert_eq!(first_files(found.clone(), Some(1))["total"], 90_000);
        assert_eq!(first_files(json!({}), Some(3)), json!({ "files": [], "scanning": false, "total": 0 }));
    }
}
