//! What commands ask of the host: the `ext.*` requests behind `@heyo/api`
//! (docs/extension-api.md). Heyo's own commands ask this way too, as the
//! extension `@heyo`, which needs no permission.
//!
//! A request says which extension it comes from. Before it is answered the
//! extension's manifest and the user's grants are consulted
//! (`permissions.rs`), which may put a question up in the palette; so each
//! request is answered on a thread of its own, and the runtime's other
//! messages go on being read meanwhile. Failures carry a code.
//!
//! Some requests subscribe: the answer is a number, and events with that
//! number follow until the subscription ends by itself (a recording is
//! over), is ended by the runtime (`ext.unsubscribe`, which the runtime
//! also sends for a command that closes), or the runtime's process is gone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::activities;
use crate::clipboard::sources::Shown;
use crate::clipboard::{self, ClipboardHistory, Kind};
use crate::dictation::{self, Heard, NotListening};
use crate::extensions::{self, Declared};
use crate::permissions::{self, Caller, Identity};
use crate::platform::capture::{Area, Shot};
use crate::platform::desktop::Supported as Abilities;
use crate::platform::system::Action;
use crate::platform::windows::{self, Layout, Rect, WindowInfo};
use crate::platform::{self, Support};
use crate::screenshot::{self, Picked};
use crate::wire::{
    AiProvider, Capture, CaptureTarget, CaptureWhole, ErrorCode, ExtCaller, ExtError, ExtRequest, ExtWindow, HostMessage, MemoryItem, MemoryResult, Module, RecognizedText,
    Subscription, Understood, WindowLayout,
};

type Answer = Result<Value, ExtError>;

/// Answers a request of an extension's, from the runtime process that is
/// (`background`) or is not the one for the menu bar and the background.
pub(crate) fn handle(app: &AppHandle, message: Value, background: bool) {
    let id = message["id"].clone();
    let app = app.clone();
    // The answer may wait for the user, or for a recording to start.
    std::thread::spawn(move || {
        let reply = reply(id, answer(&app, message, background));
        crate::runtime::reply_to(&app, background, &reply);
    });
}

fn reply(id: Value, answer: Answer) -> Value {
    match answer {
        Ok(result) => json!({ "type": "reply", "id": id, "result": result }),
        Err(error) => json!({ "type": "reply", "id": id, "error": error }),
    }
}

fn answer(app: &AppHandle, message: Value, background: bool) -> Answer {
    let method = message["method"].as_str().unwrap_or_default().to_owned();
    let given: ExtCaller = serde_json::from_value(message.clone()).map_err(|_| ExtError::invalid(format!("{method} does not say which extension asks")))?;
    let request = ExtRequest::read(message).map_err(|e| ExtError::invalid(format!("Heyo cannot answer {method}: {e}")))?;
    let declared = extensions::declared(Path::new(&given.dir));
    let caller = caller(given, declared, background)?;
    if let Some(need) = permissions::needed(&request) {
        permissions::check(app, &caller, need, &method)?;
    }
    run(app, &caller, request)
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

fn value(result: impl serde::Serialize) -> Answer {
    serde_json::to_value(result).map_err(|e| ExtError::from(e.to_string()))
}

fn supported() -> Abilities {
    platform::desktop::desktop().supported()
}

fn run(app: &AppHandle, caller: &Caller, request: ExtRequest) -> Answer {
    use ExtRequest::*;
    match request {
        Unsubscribe { subscription } => unsubscribe(subscription, caller.background),
        Supported { module } => return value(support(module, supported())),

        WindowsList => {
            need_windows()?;
            return value(described(windows::list(None)?, crate::working_window(app), &crate::monitors(app)?));
        }
        WindowsActive => {
            need_windows()?;
            let current = crate::working_window(app);
            let all = described(windows::list(None)?, current, &crate::monitors(app)?);
            return value(all.into_iter().find(|window| Some(window.id) == current));
        }
        WindowsActivate { id } => {
            need_windows()?;
            // The palette makes way for the window, as when a command of Heyo's switches.
            if !caller.background {
                let _ = crate::main_window(app).hide();
            }
            windows::activate(id)?;
        }
        WindowsClose { id } => {
            need_windows()?;
            windows::close(id)?;
        }
        WindowsArrange { id, layout } => {
            need_windows()?;
            windows::apply(id, layout_of(&layout)?, &crate::monitors(app)?)?;
        }
        WindowsSetBounds { id, bounds } => {
            need_windows()?;
            if bounds.width <= 0 || bounds.height <= 0 {
                return Err(ExtError::invalid("A window needs a width and a height."));
            }
            windows::apply(id, Layout::Exact(bounds), &crate::monitors(app)?)?;
        }
        WindowsMonitors => return value(crate::monitors(app)?),
        FrontmostApplication => {
            need_windows()?;
            return Ok(frontmost_application(app));
        }

        ScreenCapture { target } => return value(capture(app, caller, target)?),
        ScreenRecognizeText { path } => {
            let image = Path::new(&path);
            if !image.is_file() {
                return Err(ExtError::invalid(format!("{path} is not a file.")));
            }
            if !crate::ocr::installed() {
                return Err(ExtError::new(ErrorCode::Unavailable, "Text recognition is not set up yet. Download it in Heyo Settings → Screenshots."));
            }
            return value(RecognizedText { text: crate::ocr::recognize(app, image)? });
        }

        DictationInstalled => return Ok(dictation::installed().into()),
        DictationListen { language, indicator } => return value(listen(app, caller, language, indicator.unwrap_or(true))?),
        DictationStop { subscription } => {
            // Looked up first: stopping may tell the listener at once, which
            // looks the subscription up itself.
            let recording = SUBSCRIPTIONS.lock().unwrap().token(subscription, caller.background);
            if let Some(recording) = recording {
                dictation::stop_listening(app, recording);
            }
        }

        MemorySearch { text, sources, kinds, since, until, limit, by_meaning } => {
            // The clipboard is the one source there is.
            if sources.is_some_and(|sources| !sources.iter().any(|source| source == "clipboard")) {
                return value(MemoryResult { items: Vec::new(), understood: Understood::default(), relaxed: false });
            }
            let one_kind = kinds.as_deref().and_then(|kinds| (kinds.len() == 1).then(|| kinds[0]));
            let found = clipboard::find(app, text.as_deref().unwrap_or_default(), one_kind, by_meaning.unwrap_or(false));
            let understood = Understood {
                terms: found.query.terms,
                app: found.query.app,
                kind: found.query.kind,
                from: found.query.time.as_ref().map(|span| span.start),
                to: found.query.time.as_ref().map(|span| span.end),
            };
            let items = chosen(found.entries, &Filter { kinds, since, until, limit });
            return value(MemoryResult { items, understood, relaxed: found.relaxed });
        }
        MemoryGet { id } => return value(clipboard::one(app, entry_id(&id)?).as_ref().map(item)),
        ClipboardHistoryCopy { id } => {
            let entry = app.state::<ClipboardHistory>().get(entry_id(&id)?).ok_or_else(|| ExtError::invalid(format!("There is no entry {id} in the clipboard history.")))?;
            clipboard::copy_entry(app, &entry)?;
        }

        FilesSearch { query, limit } => {
            let found = value(app.state::<std::sync::Arc<crate::files::Files>>().search(&query))?;
            return Ok(first_files(found, limit));
        }

        SystemRun { action } => {
            // A command of Heyo's has asked already, in its own words.
            if let Some((question, yes)) = confirmed(action).filter(|_| !caller.builtin) {
                permissions::confirm_each_time(app, caller, question.into(), yes)?;
            }
            return value(platform::system::run(action)?);
        }
        SystemAppearance => {
            let dark = platform::system::dark().map_err(ExtError::unsupported)?;
            return Ok(if dark { "dark" } else { "light" }.into());
        }
        SystemNotify { title, body } => crate::notify(&title, body.as_deref().unwrap_or_default()),
        SystemAudioDevices => return value(platform::system::audio_devices()?),
        SystemUseAudioDevice { kind, name } => platform::system::use_audio_device(kind, &name)?,

        ClipboardPaste { text, html } => paste_and_restore(app, text, html)?,

        AiProvider => return value(provider(&app.state::<crate::SettingsState>().0.read().unwrap().ai())),

        ActivityStart { attributes, content, announce, launch_type, seq } => {
            let source = activities::Source { extension: caller.name.clone(), title: caller.title.clone(), location: caller.location.clone() };
            // A command the user ran, or the handler of an action they chose.
            let user = launch_type.as_deref() != Some("background");
            let id = attributes.id.clone();
            activities::change(app, |table, now| table.start(source, attributes, content, announce.unwrap_or(true), user, seq, now).map(|effects| ((), effects)))?;
            return Ok(json!({ "id": id }));
        }
        ActivityUpdate { id, content, alert, seq } => {
            activities::change(app, |table, now| table.update(&caller.name, &id, content, alert, seq, now).map(|effects| ((), effects)))?;
        }
        ActivityEnd { id, final_content, keep } => {
            activities::change(app, |table, now| table.end(&caller.name, &id, final_content, keep.as_deref(), now).map(|effects| ((), effects)))?;
        }
        ActivityList => return value(activities::read(app, |table, _| table.ids(&caller.name))),
        ActivityGet { id } => return Ok(activities::read(app, |table, _| table.get(&caller.name, &id).is_some()).into()),
        ActivityPresentation => return value(presentation()),
        ActivityOnEvent => {
            let subscription = SUBSCRIPTIONS.lock().unwrap().open(caller.background);
            let extension = caller.name.clone();
            let listener = app.clone();
            activities::change(app, |table, _| {
                table.listen(&extension, subscription);
                Ok(((), Vec::new()))
            })?;
            SUBSCRIPTIONS.lock().unwrap().attach(
                subscription,
                subscription,
                Box::new(move || {
                    let _ = activities::change(&listener, |table, _| {
                        table.unlisten(subscription);
                        Ok(((), Vec::new()))
                    });
                }),
            );
            return value(Subscription { subscription });
        }
    }
    Ok(Value::Null)
}

/// How activities show here: in the palette's "In Progress" section, with
/// one notification per activity for what the user must hear of. No
/// status bar presenter yet, so nothing is expanded and no buttons show
/// outside the palette.
fn presentation() -> crate::wire::Presentation {
    crate::wire::Presentation { support: Support::None, compact_characters: activities::COMPACT_CHARACTERS as u32, expanded: false, buttons: false, version: 1 }
}

// --- What this session can do ---------------------------------------------------

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

fn need_windows() -> Result<(), ExtError> {
    match supported().windows {
        Support::None => Err(ExtError::unsupported(platform::unsupported("Reaching other applications' windows"))),
        _ => Ok(()),
    }
}

// --- Windows ---------------------------------------------------------------------

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
fn frontmost_application(app: &AppHandle) -> Value {
    let current = crate::working_window(app);
    let Some(window) = current.and_then(|id| windows::list(Some(id)).ok()?.into_iter().find(|w| w.id == id)) else {
        return Value::Null;
    };
    let class = window.class.to_lowercase();
    let index = app.state::<crate::IndexState>();
    let index = index.0.read().unwrap();
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

// --- The screen ------------------------------------------------------------------

static CAPTURES: AtomicU32 = AtomicU32::new(0);

/// Where an extension's next capture goes: its cache directory, which the
/// runtime clears of captures when the command that asked has ended.
fn capture_path(extension: &str) -> PathBuf {
    let n = CAPTURES.fetch_add(1, Ordering::Relaxed);
    crate::paths::cache().join(extension).join("captures").join(format!("capture-{}-{n}.png", std::process::id()))
}

/// What of a rectangle is on the screen (all the monitors together).
fn on_screen(rect: Rect, monitors: &[Rect]) -> Option<Rect> {
    let left = monitors.iter().map(|m| m.x).min()?.max(rect.x);
    let top = monitors.iter().map(|m| m.y).min()?.max(rect.y);
    let right = monitors.iter().map(|m| m.x + m.width).max()?.min(rect.x.saturating_add(rect.width));
    let bottom = monitors.iter().map(|m| m.y + m.height).max()?.min(rect.y.saturating_add(rect.height));
    (right > left && bottom > top).then(|| Rect { x: left, y: top, width: right - left, height: bottom - top })
}

/// Writes a capture as a PNG only the user can read: it shows their screen.
fn save_png(shot: &Shot, path: &Path) -> Result<(), String> {
    let mut rgba = shot.bgrx.clone();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    let image = image::RgbaImage::from_raw(shot.width, shot.height, rgba).ok_or("the capture is not the size it says")?;
    std::fs::create_dir_all(path.parent().ok_or("the capture has no directory")?).map_err(|e| e.to_string())?;
    image.save(path).map_err(|e| format!("failed to save {}: {e}", path.display()))?;
    private(path);
    Ok(())
}

#[cfg_attr(not(unix), allow(unused_variables))]
fn private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}

/// Runs `f` with the palette out of the picture, and brings it back as it
/// was: the command that asked is still in it.
fn without_the_palette<T>(app: &AppHandle, f: impl FnOnce() -> T) -> T {
    let window = crate::main_window(app);
    let shown = window.is_visible().unwrap_or(false);
    if shown {
        let _ = window.hide();
        // Let it disappear from the screen first.
        std::thread::sleep(Duration::from_millis(180));
    }
    let result = f();
    if shown {
        let _ = window.show();
        crate::focus(&window);
    }
    result
}

fn capture(app: &AppHandle, caller: &Caller, target: CaptureTarget) -> Result<Capture, ExtError> {
    if supported().capture == Support::None {
        return Err(ExtError::unsupported(platform::unsupported("Capturing the screen")));
    }
    let path = capture_path(&caller.name);
    let shown = path.to_string_lossy().into_owned();
    let rect = match target {
        CaptureTarget::Whole(CaptureWhole::Selection) => {
            return match without_the_palette(app, || screenshot::pick(app, path.clone())) {
                Picked::Saved { width, height } => {
                    private(&path);
                    Ok(Capture { path: shown, width, height })
                }
                Picked::Cancelled => Err(ExtError::new(ErrorCode::Cancelled, "No area was selected.")),
                Picked::Failed(e) => Err(e.into()),
            };
        }
        CaptureTarget::Whole(CaptureWhole::Screen) => None,
        CaptureTarget::Window { window } => {
            let found = windows::list(None)?.into_iter().find(|w| w.id == window).ok_or_else(|| ExtError::invalid(format!("There is no window {window}.")))?;
            Some(found.bounds.ok_or_else(|| ExtError::unsupported("This desktop does not say where its windows are."))?)
        }
        CaptureTarget::Rect { rect } => Some(rect),
    };
    let area = match rect {
        None => screenshot::current_monitor(app)?,
        Some(rect) => {
            let rect = on_screen(rect, &crate::monitors(app)?).ok_or_else(|| ExtError::invalid("None of that is on the screen."))?;
            Area { x: rect.x, y: rect.y, width: rect.width as u32, height: rect.height as u32, scale: 1.0 }
        }
    };
    let shot = without_the_palette(app, || platform::capture::grab(area))?;
    save_png(&shot, &path)?;
    Ok(Capture { path: shown, width: shot.width, height: shot.height })
}

// --- Dictation -------------------------------------------------------------------

/// What the one listening is sent for what was heard.
fn heard_event(heard: &Heard) -> Value {
    match heard {
        Heard::Segment(text) => json!({ "kind": "segment", "text": text }),
        Heard::Result(text) => json!({ "kind": "result", "text": text }),
        Heard::Cancelled => json!({ "kind": "error", "code": "cancelled", "message": "The recording was discarded." }),
        Heard::Failed(message) => json!({ "kind": "error", "code": "failed", "message": message }),
    }
}

fn not_listening(why: NotListening) -> ExtError {
    match why {
        NotListening::Busy => ExtError::from("A recording is already in progress."),
        NotListening::NotInstalled => ExtError::new(ErrorCode::Unavailable, "Dictation is not set up yet. Download it in Heyo Settings → Dictation."),
        NotListening::Failed(e) => e.into(),
    }
}

fn listen(app: &AppHandle, caller: &Caller, language: Option<String>, indicator: bool) -> Result<Subscription, ExtError> {
    if support(Module::Dictation, supported()) == Support::None {
        return Err(ExtError::unsupported(platform::unsupported("Dictation")));
    }
    if let Some(language) = language.as_deref().filter(|language| !dictation::LANGUAGES.contains(language)) {
        return Err(ExtError::invalid(format!("Dictation does not know the language {language:?}.")));
    }
    let subscription = SUBSCRIPTIONS.lock().unwrap().open(caller.background);
    let hearer = app.clone();
    let started = dictation::listen(app, language, indicator, move |heard| {
        emit(&hearer, subscription, heard_event(&heard));
        if matches!(heard, Heard::Result(_) | Heard::Cancelled | Heard::Failed(_)) {
            SUBSCRIPTIONS.lock().unwrap().ended(subscription);
        }
    });
    let recording = started.map_err(|why| {
        SUBSCRIPTIONS.lock().unwrap().ended(subscription);
        not_listening(why)
    })?;
    let canceller = app.clone();
    SUBSCRIPTIONS.lock().unwrap().attach(subscription, recording, Box::new(move || dictation::cancel_listening(&canceller, recording)));
    Ok(Subscription { subscription })
}

// --- Subscriptions ---------------------------------------------------------------

type Cancel = Box<dyn FnOnce() + Send>;

struct Open {
    /// Which runtime process subscribed, and so is sent the events.
    background: bool,
    /// What the source calls the thing subscribed to (a recording's number).
    token: Option<u64>,
    /// Stops the source. Absent until the source has started.
    cancel: Option<Cancel>,
}

/// The subscriptions that are open.
#[derive(Default)]
struct Subscriptions {
    last: u64,
    open: Option<HashMap<u64, Open>>,
}

impl Subscriptions {
    fn all(&mut self) -> &mut HashMap<u64, Open> {
        self.open.get_or_insert_with(HashMap::new)
    }

    fn open(&mut self, background: bool) -> u64 {
        self.last += 1;
        let id = self.last;
        self.all().insert(id, Open { background, token: None, cancel: None });
        id
    }

    /// Says how to stop what a subscription is to, once that has started.
    /// If the subscription is gone already, nothing is left to stop.
    fn attach(&mut self, id: u64, token: u64, cancel: Cancel) {
        if let Some(open) = self.all().get_mut(&id) {
            open.token = Some(token);
            open.cancel = Some(cancel);
        }
    }

    /// The process a subscription's events go to, while it is open.
    fn listener(&mut self, id: u64) -> Option<bool> {
        self.all().get(&id).map(|open| open.background)
    }

    /// What the source calls it, for the process that subscribed.
    fn token(&mut self, id: u64, background: bool) -> Option<u64> {
        self.all().get(&id).filter(|open| open.background == background).and_then(|open| open.token)
    }

    /// The source is done: no more events, and nothing to stop.
    fn ended(&mut self, id: u64) {
        self.all().remove(&id);
    }

    /// The process that subscribed wants no more: returns how to stop the source.
    fn close(&mut self, id: u64, background: bool) -> Option<Cancel> {
        if self.listener(id) != Some(background) {
            return None;
        }
        self.all().remove(&id).and_then(|open| open.cancel)
    }

    /// A runtime process is gone, and all it subscribed to with it.
    fn close_all(&mut self, background: bool) -> Vec<Cancel> {
        let ids: Vec<u64> = self.all().iter().filter(|(_, open)| open.background == background).map(|(id, _)| *id).collect();
        ids.into_iter().filter_map(|id| self.all().remove(&id).and_then(|open| open.cancel)).collect()
    }
}

static SUBSCRIPTIONS: Mutex<Subscriptions> = Mutex::new(Subscriptions { last: 0, open: None });

/// Sends an event to the process that subscribed, if it still wants them.
pub(crate) fn emit(app: &AppHandle, subscription: u64, data: Value) {
    let listener = SUBSCRIPTIONS.lock().unwrap().listener(subscription);
    if let Some(background) = listener {
        crate::runtime::reply_to(app, background, &HostMessage::Event { subscription, data }.json());
    }
}

fn unsubscribe(subscription: u64, background: bool) {
    let cancel = SUBSCRIPTIONS.lock().unwrap().close(subscription, background);
    if let Some(cancel) = cancel {
        cancel();
    }
}

/// A runtime process ended: what its commands subscribed to stops.
pub(crate) fn runtime_exited(background: bool) {
    let cancels = SUBSCRIPTIONS.lock().unwrap().close_all(background);
    for cancel in cancels {
        cancel();
    }
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

// --- Files, the system, the clipboard, the model -------------------------------

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

/// Pastes text into the window in front (the palette has hidden) and puts
/// back the text that was on the clipboard (`pasting`).
fn paste_and_restore(app: &AppHandle, text: String, html: Option<String>) -> Result<(), ExtError> {
    if supported().paste == Support::None {
        return Err(ExtError::unsupported(platform::unsupported("Pasting")));
    }
    Ok(crate::pasting::paste_and_give_back(app, &text, html.as_deref())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::Heyo;
    use crate::platform::desktop::fake::Fake;
    use crate::wire::Route;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
        Rect { x, y, width, height }
    }

    const LEFT: Rect = Rect { x: 0, y: 0, width: 1920, height: 1080 };
    const RIGHT: Rect = Rect { x: 1920, y: 0, width: 2560, height: 1440 };

    fn given(extension: &str, dir: &str) -> ExtCaller {
        ExtCaller { extension: extension.into(), dir: dir.into(), via: None }
    }

    fn installed(name: &str) -> Option<Declared> {
        Some(Declared { name: name.into(), title: "Tiles".into(), heyo: Some(Heyo { api_version: 1, permissions: vec!["windows".into()], platforms: None }) })
    }

    #[test]
    fn a_request_is_the_extensions_whose_manifest_is_where_it_says() {
        let tiles = caller(given("tiles", "/x/tiles"), installed("tiles"), false).unwrap();
        assert_eq!((tiles.name.as_str(), tiles.title.as_str(), tiles.builtin, tiles.background), ("tiles", "Tiles", false, false));
        assert_eq!(tiles.heyo.unwrap().permissions, ["windows"]);
        assert_eq!(tiles.route, Route::Heyo);
        let through = caller(ExtCaller { via: Some(Route::Raycast), ..given("tiles", "/x/tiles") }, installed("tiles"), true).unwrap();
        assert_eq!((through.route, through.background), (Route::Raycast, true));

        // The name of another extension than the one installed there.
        assert_eq!(caller(given("notes", "/x/tiles"), installed("tiles"), false).unwrap_err().code, ErrorCode::Invalid);
        // No extension there at all.
        assert_eq!(caller(given("tiles", "/nowhere"), None, false).unwrap_err().code, ErrorCode::Invalid);
        // A name that would not stay inside the cache directory.
        for name in ["", "../tiles", "a/b", ".hidden", "a\\b"] {
            assert_eq!(caller(given(name, "/x/tiles"), installed(name), false).unwrap_err().code, ErrorCode::Invalid, "{name:?}");
        }
        // Heyo's own commands have no directory.
        let heyo = caller(given("@heyo", ""), None, false).unwrap();
        assert!(heyo.builtin && heyo.title == "Heyo" && heyo.location == "@heyo");
        assert_eq!(tiles.location, "/x/tiles");
        // An extension that calls itself by Heyo's name is not Heyo, whatever
        // its manifest says, and is nobody else either.
        for dir in ["/x/@heyo", "/x/tiles", "."] {
            let pretender = caller(given("@heyo", dir), installed("@heyo"), false).unwrap_err();
            assert_eq!(pretender.code, ErrorCode::Invalid, "{dir}");
            assert_eq!(caller(given("@heyo", dir), None, true).unwrap_err().code, ErrorCode::Invalid, "{dir}");
        }
        // Nor is one that says it comes from nowhere.
        assert_eq!(caller(given("tiles", ""), installed("tiles"), false).unwrap().builtin, false);
    }

    #[test]
    fn a_reply_carries_a_result_or_an_error_with_its_code() {
        assert_eq!(reply(json!(41), Ok(json!([1]))), json!({ "type": "reply", "id": 41, "result": [1] }));
        assert_eq!(
            reply(json!(41), Err(ExtError::new(ErrorCode::Denied, "Window access was not allowed."))),
            json!({ "type": "reply", "id": 41, "error": { "code": "denied", "message": "Window access was not allowed." } })
        );
    }

    #[test]
    fn each_part_of_the_api_says_how_well_the_session_supports_it() {
        let nothing = Abilities::NOTHING;
        let all = Abilities { windows: Support::Full, capture: Support::Limited, clipboard_watch: Support::Full, ..Abilities::NOTHING };
        assert_eq!(support(Module::Windows, nothing), Support::None);
        assert_eq!(support(Module::Windows, all), Support::Full);
        assert_eq!(support(Module::Screen, nothing), Support::None);
        assert_eq!(support(Module::Screen, all), Support::Limited);
        // The history is kept either way; without a watcher it is polled.
        assert_eq!(support(Module::ClipboardHistory, nothing), Support::Limited);
        assert_eq!(support(Module::Memory, all), Support::Full);
        assert_eq!(support(Module::Files, nothing), Support::Full);
        assert_eq!(support(Module::Dictation, nothing), if cfg!(target_os = "linux") { Support::Full } else { Support::None });
        assert_eq!(support(Module::Activity, nothing), Support::Limited);
        let shown = presentation();
        assert_eq!((shown.support, shown.compact_characters, shown.expanded, shown.buttons, shown.version), (Support::None, 10, false, false, 1));
    }

    #[test]
    fn windows_are_told_with_the_one_the_user_works_in_and_their_monitor() {
        let fake = Fake::with_windows(&[(1, "firefox"), (2, "Code"), (3, "org.gnome.Ptyxis")]);
        fake.windows.lock().unwrap()[1].bounds = Some(rect(2000, 100, 800, 600));
        let told = fake.during(|| described(windows::list(None).unwrap(), Some(2), &[LEFT, RIGHT]));
        // Most recently used first, as the desktop lists them.
        assert_eq!(told.iter().map(|w| w.id).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(told.iter().map(|w| w.active).collect::<Vec<_>>(), [false, true, false]);
        assert_eq!(told[1], ExtWindow { id: 2, title: "Code 2".into(), class: "Code".into(), active: true, bounds: Some(rect(2000, 100, 800, 600)), monitor: Some(1) });
        // Where the desktop does not say where a window is, neither does Heyo.
        assert_eq!((told[0].bounds, told[0].monitor), (None, None));
        assert_eq!(serde_json::to_value(&told[0]).unwrap(), json!({ "id": 1, "title": "firefox 1", "class": "firefox", "active": false }));

        assert_eq!(monitor_of(rect(100, 100, 800, 600), &[LEFT, RIGHT]), Some(0));
        // Off every monitor: the first. With no monitor known: none.
        assert_eq!(monitor_of(rect(-9000, 0, 10, 10), &[LEFT, RIGHT]), Some(0));
        assert_eq!(monitor_of(rect(0, 0, 10, 10), &[]), None);
    }

    #[test]
    fn a_layout_is_one_of_the_named_or_a_part_of_the_work_area() {
        // Every layout the specification names is one the host has.
        let named = [
            "left-half", "right-half", "top-half", "bottom-half", "top-left", "top-right", "bottom-left", "bottom-right", "first-third", "center-third", "last-third",
            "first-two-thirds", "last-two-thirds", "almost-maximize", "maximize", "center", "next-display",
        ];
        for name in named {
            assert!(layout_of(&WindowLayout::Named(name.into())).is_ok(), "{name}");
        }
        assert_eq!(named.len(), windows::LAYOUTS.len());
        assert_eq!(layout_of(&WindowLayout::Named("left-half".into())).unwrap(), Layout::Fraction(0, 0, 6, 12));
        assert_eq!(layout_of(&WindowLayout::Named("maximize".into())).unwrap(), Layout::Maximize);
        assert_eq!(layout_of(&WindowLayout::Named("sideways".into())).unwrap_err().code, ErrorCode::Invalid);

        assert_eq!(layout_of(&WindowLayout::Fraction { fraction: [3, 0, 6, 12] }).unwrap(), Layout::Fraction(3, 0, 6, 12));
        for outside in [[-1, 0, 6, 12], [0, 0, 0, 12], [8, 0, 6, 12], [0, 6, 6, 7], [0, 0, 13, 1]] {
            assert_eq!(layout_of(&WindowLayout::Fraction { fraction: outside }).unwrap_err().code, ErrorCode::Invalid, "{outside:?}");
        }
    }

    #[test]
    fn the_desktop_is_asked_to_do_what_an_extension_asked() {
        let fake = Fake::with_windows(&[(1, "firefox"), (2, "Code")]);
        fake.during(|| {
            windows::apply(2, layout_of(&WindowLayout::Named("right-half".into())).unwrap(), &[LEFT]).unwrap();
            windows::apply(2, Layout::Exact(rect(10, 20, 300, 200)), &[LEFT]).unwrap();
            windows::close(1).unwrap();
            assert!(need_windows().is_ok());
        });
        assert_eq!(fake.asked(), ["arrange 2 Fraction(6, 0, 6, 12)", "arrange 2 Exact(Rect { x: 10, y: 20, width: 300, height: 200 })", "close 1"]);
    }

    #[test]
    fn only_what_is_on_the_screen_is_captured() {
        let monitors = [LEFT, RIGHT];
        assert_eq!(on_screen(rect(100, 100, 800, 600), &monitors), Some(rect(100, 100, 800, 600)));
        // Across two monitors: all of it.
        assert_eq!(on_screen(rect(1800, 100, 400, 300), &monitors), Some(rect(1800, 100, 400, 300)));
        // Hanging off the edges: what is left.
        assert_eq!(on_screen(rect(-50, -20, 200, 100), &monitors), Some(rect(0, 0, 150, 80)));
        assert_eq!(on_screen(rect(4400, 1400, 500, 500), &monitors), Some(rect(4400, 1400, 80, 40)));
        // Off the screen, of no size, or with no screen known: nothing.
        assert_eq!(on_screen(rect(9000, 0, 100, 100), &monitors), None);
        assert_eq!(on_screen(rect(10, 10, 0, 50), &monitors), None);
        assert_eq!(on_screen(rect(10, 10, -5, 50), &monitors), None);
        assert_eq!(on_screen(rect(0, 0, 10, 10), &[]), None);
        assert_eq!(on_screen(rect(i32::MAX - 5, 0, 100, 10), &monitors), None);
    }

    #[test]
    fn a_capture_is_a_png_in_the_extensions_cache_only_the_user_can_read() {
        let first = capture_path("tiles");
        assert!(first.starts_with(crate::paths::cache().join("tiles").join("captures")));
        assert_ne!(first, capture_path("tiles"));

        // Two pixels as the screen is read: blue, green, red and a byte unused.
        let shot = Shot { width: 2, height: 1, bgrx: vec![255, 0, 0, 0, 10, 20, 30, 0] };
        let dir = std::env::temp_dir().join(format!("heyo-ext-test-{}", std::process::id()));
        let path = dir.join("captures").join("a.png");
        save_png(&shot, &path).unwrap();
        let read = image::open(&path).unwrap().into_rgba8();
        assert_eq!(read.dimensions(), (2, 1));
        assert_eq!((read.get_pixel(0, 0).0, read.get_pixel(1, 0).0), ([0, 0, 255, 255], [30, 20, 10, 255]));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert!(save_png(&Shot { width: 5, height: 5, bgrx: vec![0; 8] }, &dir.join("b.png")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn what_was_heard_is_sent_as_events() {
        assert_eq!(heard_event(&Heard::Segment("Hello.".into())), json!({ "kind": "segment", "text": "Hello." }));
        assert_eq!(heard_event(&Heard::Result("Hello. World.".into())), json!({ "kind": "result", "text": "Hello. World." }));
        assert_eq!(heard_event(&Heard::Cancelled)["code"], "cancelled");
        assert_eq!(heard_event(&Heard::Failed("no microphone".into())), json!({ "kind": "error", "code": "failed", "message": "no microphone" }));
        // One recording at a time; an engine that is not there can be installed.
        assert_eq!(not_listening(NotListening::Busy).code, ErrorCode::Failed);
        assert_eq!(not_listening(NotListening::NotInstalled).code, ErrorCode::Unavailable);
        assert_eq!(not_listening(NotListening::Failed("no node".into())).message, "no node");
    }

    fn counting(count: &Arc<AtomicUsize>) -> Cancel {
        let count = count.clone();
        Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        })
    }

    #[test]
    fn a_subscription_ends_by_itself_by_its_subscriber_or_with_its_process() {
        let stopped = Arc::new(AtomicUsize::new(0));
        let mut subscriptions = Subscriptions::default();
        let (first, second) = (subscriptions.open(false), subscriptions.open(true));
        assert_ne!(first, second);
        subscriptions.attach(first, 7, counting(&stopped));
        subscriptions.attach(second, 8, counting(&stopped));
        assert_eq!((subscriptions.listener(first), subscriptions.listener(second)), (Some(false), Some(true)));
        assert_eq!(subscriptions.token(first, false), Some(7));

        // The other process can neither end it nor learn what it is to.
        assert!(subscriptions.close(first, true).is_none());
        assert_eq!(subscriptions.token(first, true), None);
        assert_eq!(subscriptions.listener(first), Some(false));
        // Its own can: the source is stopped, once.
        subscriptions.close(first, false).expect("how to stop the source")();
        assert_eq!(stopped.load(Ordering::SeqCst), 1);
        assert!(subscriptions.close(first, false).is_none());
        assert_eq!(subscriptions.listener(first), None);

        // A source that is done leaves nothing to stop.
        subscriptions.ended(second);
        assert!(subscriptions.close(second, true).is_none());
        // Nor is there anything to attach to once it is gone.
        subscriptions.attach(second, 9, counting(&stopped));
        assert_eq!(subscriptions.listener(second), None);

        // A process that exits takes its subscriptions with it, and only its own.
        let (palette, menu_bar, starting) = (subscriptions.open(false), subscriptions.open(true), subscriptions.open(false));
        subscriptions.attach(palette, 1, counting(&stopped));
        subscriptions.attach(menu_bar, 2, counting(&stopped));
        let cancels = subscriptions.close_all(false);
        // The one whose source had not started yet has nothing to stop.
        assert_eq!(cancels.len(), 1);
        cancels.into_iter().for_each(|cancel| cancel());
        assert_eq!(stopped.load(Ordering::SeqCst), 2);
        assert_eq!((subscriptions.listener(palette), subscriptions.listener(starting), subscriptions.listener(menu_bar)), (None, None, Some(true)));
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

    #[test]
    fn what_ends_the_session_is_confirmed_every_time() {
        for action in [Action::Restart, Action::ShutDown, Action::LogOut] {
            assert!(confirmed(action).is_some(), "{action:?}");
        }
        for action in [Action::LockScreen, Action::Sleep, Action::EmptyTrash, Action::ToggleDarkMode, Action::VolumeUp, Action::VolumeDown, Action::ToggleMute] {
            assert!(confirmed(action).is_none(), "{action:?}");
        }
        assert_eq!(confirmed(Action::ShutDown), Some(("Shut the computer down?", "Shut Down")));
    }

    #[test]
    fn the_model_is_said_without_its_key() {
        let claude = provider(&json!({ "provider": "claude", "apiKey": "sk-secret", "localUrl": "http://127.0.0.1:8000/v1", "localModel": "" }));
        assert_eq!(claude, AiProvider { kind: "claude", model: String::new(), local: false });
        let local = provider(&json!({ "provider": "local", "apiKey": "sk-secret", "localUrl": "http://127.0.0.1:8000/v1", "localModel": "qwen3" }));
        assert_eq!(serde_json::to_value(local).unwrap(), json!({ "kind": "local", "model": "qwen3", "local": true }));
    }
}
