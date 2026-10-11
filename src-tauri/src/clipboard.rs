//! Clipboard history: records what is copied while Heyo runs.
//!
//! On X11 the XFixes extension reports every change of the CLIPBOARD owner,
//! so nothing is polled; elsewhere the clipboard text is polled. Copies that
//! password managers mark as secret are skipped, as are copies made in the
//! applications the user names (`sources`) and while recording is paused.
//! History lives in `$XDG_DATA_HOME/heyo/clipboard/` (`history.db` and
//! `images/`).

pub mod embed;
pub mod eval;
pub mod fusion;
pub mod meaning;
pub mod query;
pub mod sources;
pub mod store;
pub mod vectors;

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::SettingsState;

/// Longer text is truncated before it is stored.
const MAX_TEXT: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum Kind {
    Text,
    Link,
    Color,
    File,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    pub kind: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// PNG file of an image entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Window class of the application that was focused when it was copied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// Milliseconds since the Unix epoch.
    pub copied_at: u64,
    pub first_copied_at: u64,
    pub times_copied: u32,
    pub pinned: bool,
    hash: u64,
}

pub struct ClipboardHistory {
    store: Mutex<store::Store>,
    /// Finding entries by what they mean, when that is turned on.
    meaning: meaning::Meaning,
}

/// How many entries a list or a search shows.
pub const SHOWN: usize = 300;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn dir() -> PathBuf {
    crate::paths::data().join("clipboard")
}

/// Until when (milliseconds since the epoch) clipboard changes are not recorded.
static SKIP_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// How often that was asked for, for where changes are only noticed later.
static SKIPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Keeps the clipboard change Heyo is about to make out of the history.
/// For writes that are plumbing rather than something the user copied
/// (dictation pasting a sentence, then putting the old contents back — which
/// may be a password that was rightly skipped when it was first copied), and
/// for what an extension marks as concealed.
pub fn skip_next() {
    SKIP_UNTIL.store(now() + 800, std::sync::atomic::Ordering::SeqCst);
    SKIPS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

fn skipping() -> bool {
    now() < SKIP_UNTIL.load(std::sync::atomic::Ordering::SeqCst)
}

/// Applications whose copies are never recorded: password managers that do
/// not all mark their secrets. Matched against the window class, lowercase.
const IGNORED_APPS: &[&str] = &["keepass", "1password", "bitwarden", "enpass", "lastpass", "proton pass", "protonpass", "seahorse", "secrets", "pass-otp"];

pub(crate) fn ignored_app(app: Option<&str>) -> bool {
    app.is_some_and(|app| {
        let app = app.to_lowercase();
        IGNORED_APPS.iter().any(|ignored| app.contains(ignored))
    })
}

/// Makes a file or directory readable by the user only: the history holds
/// whatever was copied.
#[cfg_attr(not(unix), allow(unused_variables))]
fn private(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
    }
}

fn hash_of(data: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

fn classify(text: &str) -> Kind {
    let trimmed = text.trim();
    let is_url = |s: &str| {
        (s.starts_with("http://") || s.starts_with("https://")) && !s.contains(char::is_whitespace)
    };
    let is_hex_color = |s: &str| {
        s.strip_prefix('#').is_some_and(|hex| {
            matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
        })
    };
    if is_url(trimmed) {
        Kind::Link
    } else if is_hex_color(trimmed) || (trimmed.starts_with("rgb") && trimmed.ends_with(')') && trimmed.len() < 40) {
        Kind::Color
    } else if trimmed.lines().all(|l| {
        let path = l.trim().strip_prefix("file://").unwrap_or(l.trim());
        path.starts_with('/') && Path::new(path).exists()
    }) && !trimmed.is_empty()
    {
        Kind::File
    } else {
        Kind::Text
    }
}

/// Says what went wrong with the history, and carries on without the result.
fn logged<T: Default>(what: &str, result: rusqlite::Result<T>) -> T {
    result.unwrap_or_else(|e| {
        eprintln!("heyo: clipboard history: could not {what}: {e}");
        T::default()
    })
}

impl ClipboardHistory {
    pub fn load() -> Self {
        let store = store::Store::open(&dir()).unwrap_or_else(|e| {
            eprintln!("heyo: could not open the clipboard history ({e}); it will not outlast this session");
            store::Store::in_memory()
        });
        ClipboardHistory { store: Mutex::new(store), meaning: meaning::Meaning::default() }
    }

    /// A history that is kept nowhere.
    #[cfg(test)]
    pub(crate) fn in_memory() -> Self {
        ClipboardHistory { store: Mutex::new(store::Store::in_memory()), meaning: meaning::Meaning::default() }
    }

    /// Entries are gone, their vectors with them; none are new.
    fn deleted(&self, count: usize) {
        if count > 0 {
            self.meaning.forget();
        }
    }

    /// Adds an entry, or notes that an identical one was copied again.
    fn record(&self, hash: u64, app: Option<String>, make: impl FnOnce(u64) -> Option<Entry>, retention_days: u32) {
        let store = self.store.lock().unwrap();
        logged("record a copy", store.record(hash, app, now(), make));
        self.deleted(logged("prune", store.prune(cutoff(retention_days))));
        drop(store);
        // There may be an entry to read for its meaning.
        self.meaning.wake();
    }

    pub fn record_text(&self, text: String, app: Option<String>, retention_days: u32) {
        if text.trim().is_empty() {
            return;
        }
        let mut text = text;
        if text.len() > MAX_TEXT {
            let mut end = MAX_TEXT;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
        }
        let hash = hash_of(text.as_bytes());
        let make = |id| {
            let time = now();
            Some(Entry {
                id,
                kind: classify(&text),
                text: Some(text),
                image: None,
                width: None,
                height: None,
                app: None,
                copied_at: time,
                first_copied_at: time,
                times_copied: 1,
                pinned: false,
                hash,
            })
        };
        self.record(hash, app, make, retention_days);
    }

    pub fn record_image(&self, rgba: Vec<u8>, width: u32, height: u32, app: Option<String>, retention_days: u32) {
        let hash = hash_of(&rgba);
        let make = |id| {
            let path = dir().join("images").join(format!("{id}.png"));
            let image = image::RgbaImage::from_raw(width, height, rgba)?;
            fs::create_dir_all(path.parent()?).ok()?;
            if let Err(e) = image.save(&path) {
                eprintln!("heyo: failed to save clipboard image: {e}");
                return None;
            }
            private(&path, 0o600);
            let time = now();
            Some(Entry {
                id,
                kind: Kind::Image,
                text: None,
                image: Some(path.to_string_lossy().into_owned()),
                width: Some(width),
                height: Some(height),
                app: None,
                copied_at: time,
                first_copied_at: time,
                times_copied: 1,
                pinned: false,
                hash,
            })
        };
        self.record(hash, app, make, retention_days);
    }

    /// The entries a query finds, newest first; without a query, the most
    /// recent ones and the pinned.
    pub fn search(&self, query: &str, kind: Option<Kind>) -> store::Found {
        let store = self.store.lock().unwrap();
        let nothing = |entries| store::Found { entries, query: query::Query::default(), relaxed: false, by_meaning: Vec::new() };
        if query.trim().is_empty() {
            return nothing(logged("list the entries", store.recent(kind, SHOWN)));
        }
        store.search(query, kind, SHOWN).unwrap_or_else(|e| {
            eprintln!("heyo: clipboard history: could not search for {query:?}: {e}");
            nothing(Vec::new())
        })
    }

    pub fn get(&self, id: u64) -> Option<Entry> {
        self.store.lock().unwrap().get(id)
    }

    /// The window classes there are copies from.
    pub fn apps(&self) -> Vec<String> {
        logged("list the applications", self.store.lock().unwrap().apps())
    }

    pub fn set_pinned(&self, id: u64, pinned: bool) {
        logged("pin an entry", self.store.lock().unwrap().set_pinned(id, pinned));
    }

    pub fn delete(&self, id: u64) {
        let deleted = logged("delete an entry", self.store.lock().unwrap().delete(id));
        self.deleted(deleted);
    }

    /// Deletes all entries except pinned ones.
    pub fn clear(&self) {
        let deleted = logged("clear the history", self.store.lock().unwrap().clear());
        self.deleted(deleted);
    }

    pub fn prune(&self, retention_days: u32) {
        let deleted = logged("prune", self.store.lock().unwrap().prune(cutoff(retention_days)));
        self.deleted(deleted);
    }

    /// Has the work of finding by meaning look at the settings again now:
    /// they say whether there is any to do.
    pub fn wake_meaning(&self) {
        self.meaning.wake();
    }

    /// Notes that searching for `query` led to using the entry at `rank`.
    pub fn chose(&self, query: &str, id: u64, rank: u32) {
        if !query.trim().is_empty() {
            logged("note a choice", self.store.lock().unwrap().chose(now(), query, id, rank));
        }
    }
}

/// Entries last copied before this are past their keeping.
fn cutoff(retention_days: u32) -> u64 {
    now().saturating_sub(u64::from(retention_days) * 86_400_000)
}

/// Puts an entry back on the clipboard.
pub fn copy_entry(app: &AppHandle, entry: &Entry) -> Result<(), String> {
    if let Some(path) = &entry.image {
        let image = image::open(path).map_err(|e| e.to_string())?.into_rgba8();
        let (width, height) = image.dimensions();
        let image = tauri::image::Image::new_owned(image.into_raw(), width, height);
        app.clipboard().write_image(&image).map_err(|e| e.to_string())
    } else {
        crate::platform::clipboard_watch::write_text(entry.text.as_deref().unwrap_or_default())
    }
}

/// For how many days a copy made in `source` is kept, if it is to be
/// recorded at all.
fn wanted(app: &AppHandle, source: Option<&str>) -> Option<u32> {
    let (ignored, retention) = {
        let settings = app.state::<SettingsState>();
        let settings = settings.0.read().unwrap();
        if !settings.clipboard_history || settings.clipboard_paused {
            return None;
        }
        (settings.clipboard_ignored_apps.clone(), settings.clipboard_retention_days)
    };
    if let (Some(source), false) = (source, ignored.is_empty()) {
        let index = app.state::<crate::IndexState>();
        let index = index.0.read().unwrap();
        if sources::ignored(source, &ignored, &index.apps) {
            return None;
        }
    }
    Some(retention)
}

/// How long an answer waits for what a query means before making do with
/// its words: the model takes a second or two to start, a few milliseconds
/// after that.
const MEANING_PATIENCE: Duration = Duration::from_secs(4);

/// What the words and the meaning of a query find together, waiting for the
/// query's vector to be made. Nothing where there is nothing to add to what
/// the words find (`ClipboardHistory::search`): finding by meaning is off,
/// say.
pub(crate) fn meant(app: &AppHandle, query: &str, kind: Option<Kind>) -> Option<store::Found> {
    let asked = std::time::Instant::now();
    loop {
        match meaning::search(app, query, kind, SHOWN) {
            meaning::Answer::Found(found) => return Some(found),
            meaning::Answer::Nothing => return None,
            meaning::Answer::Coming if asked.elapsed() > MEANING_PATIENCE => return None,
            meaning::Answer::Coming => std::thread::sleep(Duration::from_millis(40)),
        }
    }
}

/// How the keeping of the history stands, for Heyo's own view of it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    /// Whether copies are recorded at all, and whether that is paused.
    pub recording: bool,
    pub paused: bool,
    /// How many entries can be found by meaning and how many are yet to be
    /// read, while finding by meaning is on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meaning: Option<meaning::Progress>,
}

pub fn status(app: &AppHandle) -> Status {
    let (recording, paused) = {
        let settings = app.state::<SettingsState>();
        let settings = settings.0.read().unwrap();
        (settings.clipboard_history, settings.clipboard_paused)
    };
    Status { recording, paused, meaning: meaning::progress(app) }
}

/// Where finding by meaning stands, for the settings window.
#[tauri::command(async)]
pub(crate) fn clipboard_meaning_status(app: AppHandle) -> meaning::Status {
    meaning::status(&app)
}

/// Downloads what finding by meaning needs.
#[tauri::command]
pub(crate) fn clipboard_meaning_install(app: AppHandle) -> Result<(), String> {
    meaning::install(&app)
}

/// Stops or resumes recording.
pub fn set_paused(app: &AppHandle, paused: bool) -> Result<(), String> {
    crate::update_settings(app, |settings| settings.clipboard_paused = paused)
}

/// Pauses recording, or resumes it, and says which: this is run from the
/// root search or by a hotkey, with nothing on screen to show it.
pub fn toggle_paused(app: &AppHandle) {
    let paused = !app.state::<SettingsState>().0.read().unwrap().clipboard_paused;
    match set_paused(app, paused) {
        Ok(()) if paused => crate::notify("Clipboard history is paused", "Nothing you copy is recorded until you resume it."),
        Ok(()) => crate::notify("Clipboard history resumed", "What you copy is recorded again."),
        Err(e) => crate::notify("Could not change clipboard history", &e),
    }
}

/// The applications that can be named as ones not to record from, for the
/// settings window.
#[tauri::command]
pub(crate) fn clipboard_sources(app: AppHandle) -> Vec<sources::Source> {
    let seen = app.state::<ClipboardHistory>().apps();
    let index = app.state::<crate::IndexState>();
    let index = index.0.read().unwrap();
    sources::choices(&index.apps, &seen)
}

/// Reads the clipboard and records it, preferring an image when the owner
/// offers one but no text.
fn capture(app: &AppHandle, prefer_image: bool, source: Option<String>) {
    let Some(retention) = wanted(app, source.as_deref()) else { return };
    let history = app.state::<ClipboardHistory>();
    if prefer_image {
        // Through the session where an unfocused application cannot read.
        if let Some(image) = crate::platform::clipboard_watch::image() {
            let (width, height) = image.dimensions();
            history.record_image(image.into_raw(), width, height, source, retention);
            return;
        }
        if let Ok(image) = app.clipboard().read_image() {
            history.record_image(image.rgba().to_vec(), image.width(), image.height(), source, retention);
            return;
        }
    }
    if let Some(text) = crate::platform::clipboard_watch::read_text() {
        history.record_text(text, source, retention);
    }
}

/// Starts recording clipboard changes in the background.
pub fn watch(app: AppHandle) {
    meaning::start(&app);
    std::thread::spawn(move || {
        let retention = app.state::<SettingsState>().0.read().unwrap().clipboard_retention_days;
        app.state::<ClipboardHistory>().prune(retention);
        let watched = crate::platform::clipboard_watch::watch(|change| {
            let wanted = (change.has_text || change.has_image) && !change.secret;
            if wanted && !skipping() && !ignored_app(change.source.as_deref()) {
                capture(&app, change.has_image && !change.has_text, change.source);
            }
        });
        if let Err(e) = watched {
            eprintln!("heyo: {e} Checking it once a second instead (text only, nothing marks secrets).");
            poll(&app);
        }
    });
}

/// What looking at the clipboard now and then makes of it. A change is
/// noticed up to a whole look after it was made, when `skipping` may have
/// forgotten that Heyo made it: so a change is Heyo's own if a skip was
/// asked for at any time since the last look.
#[derive(Default)]
struct Looks {
    last: Option<String>,
    skips: u64,
}

impl Looks {
    /// Whether `text`, found on the clipboard now, is something the user
    /// copied since the last look. `skips`: how often a skip was asked for
    /// so far; `skipping`: whether one is in force.
    fn copied(&mut self, text: Option<String>, skips: u64, skipping: bool) -> bool {
        let own = skipping || skips != self.skips;
        self.skips = skips;
        if text.is_none() || text == self.last {
            return false;
        }
        self.last = text;
        !own
    }
}

fn poll(app: &AppHandle) {
    let mut looks = Looks { last: app.clipboard().read_text().ok(), skips: SKIPS.load(std::sync::atomic::Ordering::SeqCst) };
    loop {
        std::thread::sleep(Duration::from_millis(1000));
        if looks.copied(app.clipboard().read_text().ok(), SKIPS.load(std::sync::atomic::Ordering::SeqCst), skipping()) {
            capture(app, false, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_noticed_late_is_still_known_to_be_heyos_own() {
        let text = |text: &str| Some(text.to_owned());
        let mut looks = Looks { last: text("old"), skips: 0 };
        // Nothing new, nothing copied; something new is a copy.
        assert!(!looks.copied(text("old"), 0, false));
        assert!(!looks.copied(None, 0, false));
        assert!(looks.copied(text("copied"), 0, false));
        assert!(!looks.copied(text("copied"), 0, false));
        // Dictation puts a sentence there; this look comes while the skip is in force.
        assert!(!looks.copied(text("A sentence."), 1, true));
        // The next sentence is noticed a second after it was put there,
        // when the skip is no longer in force.
        assert!(!looks.copied(text("Another."), 2, false));
        // So is the user's text coming back, which may be a secret.
        assert!(!looks.copied(text("copied"), 3, false));
        // After that, what changes is the user's doing again.
        assert!(looks.copied(text("copied next"), 3, false));
        // A skip asked for with nothing changing does not hide a later copy.
        assert!(!looks.copied(text("copied next"), 4, false));
        assert!(looks.copied(text("and another"), 4, false));
    }
}
