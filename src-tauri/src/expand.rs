//! Snippets that expand as you type: a snippet's keyword, typed in any
//! application, is replaced by the snippet's text.
//!
//! Two things are held to here above all else.
//!
//! The keyboard is only listened to (`platform::typed`): no key is grabbed,
//! held back or changed, so nothing Heyo does or fails to do can leave the
//! keyboard unusable. While the setting is off there is no listener at all.
//!
//! What is typed is neither stored nor logged. [`Engine`] remembers the last
//! few characters ([`HELD`]) in memory, for as long as they could still
//! become a keyword, and forgets them at anything that ends a word being
//! typed in one place: an arrow key, Enter, Tab, Escape, a shortcut, a click,
//! another window.
//!
//! # When a keyword counts as typed
//!
//! A keyword is compared exactly, capitals included, and must be
//! [`MIN_KEYWORD`] to [`MAX_KEYWORD`] characters without line ends (others
//! still work in the root search, and are simply never expanded).
//!
//! - A keyword with a symbol at either end (`;sig`, `:addr`, `sig;`) expands
//!   the moment its last character is typed, as in espanso: nothing typed
//!   by accident looks like it.
//! - A keyword that begins and ends with a letter or digit (`sig`) is also a
//!   piece of ordinary words, so it expands only once a character that ends
//!   the word follows (a space, a full stop, …), which is typed again after
//!   the text; "signal" stays "signal".
//! - A keyword that begins with a letter or digit does not expand directly
//!   after one: "design" does not end in `sig`. One that begins with a
//!   symbol expands anywhere.
//! - Of several keywords completed by the same character the longest wins
//!   (`:a` and `b:a`). A keyword that expands at once hides every longer one
//!   that begins with it (`;s` and `;sig`): there is no telling that more
//!   was coming.
//!
//! What cannot be known: what the text looks like before the first key heard
//! (after a click, the cursor may be inside a word), and whether the field
//! is one for passwords.
//!
//! # Replacing it
//!
//! The keyword is erased with Backspaces and the text pasted from the
//! clipboard, which is borrowed for it (`replace`, `pasting`). That is
//! not done in the windows of password managers and of the applications
//! the user excluded, in Heyo's own, or while an input method may be
//! composing; into a terminal the text goes on one line. Placeholders are
//! filled in by the palette's webview, as for a snippet chosen there.
//!
//! Nobody stops typing while this happens (a few hundredths of a second).
//! Plain characters typed meanwhile are erased along with the keyword and
//! put back after the text ([`Meanwhile`]); anything else typed meanwhile,
//! and the keyword is left as it is.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use tauri::{AppHandle, Manager};

use crate::library::Snippet;
use crate::platform::typed::{self, Typed};
use crate::platform::{input, windows};
use crate::{pasting, SettingsState};

/// The longest keyword that expands, in characters.
pub const MAX_KEYWORD: usize = 32;
/// The shortest: a single character is typed too often by itself.
pub const MIN_KEYWORD: usize = 2;
/// How many characters are remembered: a keyword, the character before it
/// and the one after.
pub const HELD: usize = MAX_KEYWORD + 2;
/// Two expansions are at least this far apart (nobody types a keyword in
/// less),
const APART: Duration = Duration::from_millis(100);
/// and there are no more than `BURST` of them within `WINDOW`: whatever
/// went wrong, Heyo does not type on and on.
const BURST: usize = 10;
const WINDOW: Duration = Duration::from_secs(10);
/// How long the clipboard is given to have the text before it is pasted,
const SETTLE: Duration = Duration::from_millis(30);
/// and how long a keyword waits for a pause in the typing, or for Shift to
/// be let go, before it is left alone.
const PATIENCE: Duration = Duration::from_millis(900);

/// What was typed in one place of the buffer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Char(char),
    /// A character that is not known. It is part of a word and of no keyword.
    Unknown,
    /// Text that was there before anything was heard, reached by erasing
    /// back into it. As likely as not the cursor is inside a word.
    Unseen,
}

/// Part of a word: what a keyword made of letters must not touch.
fn in_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

struct Keyword {
    chars: Vec<char>,
    /// Index of its snippet among those the engine was made from.
    snippet: usize,
    /// Begins with a letter or digit: does not expand inside a word.
    bounded: bool,
    /// Begins and ends with one: expands once the word is over.
    delimited: bool,
}

/// A keyword as it expands, or `None` for one that never does.
pub fn usable(keyword: &str) -> Option<Vec<char>> {
    let chars: Vec<char> = keyword.trim().chars().collect();
    let fits = (MIN_KEYWORD..=MAX_KEYWORD).contains(&chars.len());
    (fits && !chars.iter().any(|c| c.is_control())).then_some(chars)
}

/// A keyword was typed: what to do about it.
#[derive(Debug, PartialEq, Eq)]
pub struct Expansion {
    /// Index of the snippet among those the engine was made from.
    pub snippet: usize,
    /// How many characters to erase before the cursor.
    pub erase: usize,
    /// The character that ended the word, to type again after the text.
    pub after: Option<char>,
}

/// Follows what is typed and says when a keyword was completed. It only
/// remembers and answers; who feeds it and who acts on the answer is not
/// its business.
pub struct Engine {
    keywords: Vec<Keyword>,
    held: VecDeque<Slot>,
    /// When it last said to expand.
    recent: VecDeque<Instant>,
}

impl Engine {
    pub fn new(snippets: &[Snippet]) -> Engine {
        let mut keywords: Vec<Keyword> = Vec::new();
        for (snippet, chars) in snippets.iter().enumerate().filter_map(|(i, s)| Some((i, usable(s.keyword.as_deref()?)?))) {
            // Two snippets with one keyword: the first has it.
            if snippets[snippet].text.is_empty() || keywords.iter().any(|k| k.chars == chars) {
                continue;
            }
            let (first, last) = (chars[0], chars[chars.len() - 1]);
            keywords.push(Keyword { bounded: first.is_alphanumeric(), delimited: first.is_alphanumeric() && last.is_alphanumeric(), chars, snippet });
        }
        Engine { keywords, held: VecDeque::with_capacity(HELD), recent: VecDeque::new() }
    }

    /// Whether there is anything to listen for.
    pub fn has_keywords(&self) -> bool {
        !self.keywords.is_empty()
    }

    /// Forgets what was typed.
    pub fn clear(&mut self) {
        // Overwritten first, so the characters are gone and not merely unreachable.
        self.held.iter_mut().for_each(|slot| *slot = Slot::Unknown);
        self.held.clear();
    }

    fn push(&mut self, slot: Slot) {
        if self.held.len() == HELD {
            self.held.pop_front();
        }
        self.held.push_back(slot);
    }

    /// A key was pressed at `now`. Says so if it completed a keyword.
    pub fn typed(&mut self, key: Typed, now: Instant) -> Option<Expansion> {
        match key {
            Typed::Break => self.clear(),
            Typed::Unknown => self.push(Slot::Unknown),
            Typed::Backspace => match self.held.back() {
                // Erased past what was heard.
                None => self.push(Slot::Unseen),
                Some(Slot::Unseen) => {}
                Some(_) => {
                    self.held.pop_back();
                }
            },
            Typed::Char(c) => {
                self.push(Slot::Char(c));
                let found = self.completed()?;
                self.clear();
                return self.unhurried(now).then_some(found);
            }
        }
        None
    }

    /// The keyword the last character completed: the longest; of two as
    /// long, the one that needs nothing after it.
    fn completed(&self) -> Option<Expansion> {
        let Some(Slot::Char(last)) = self.held.back().copied() else { return None };
        let ended = !in_word(last) && !last.is_control();
        let mut best: Option<(&Keyword, Expansion)> = None;
        for keyword in &self.keywords {
            let found = if !keyword.delimited {
                self.ends_with(keyword, 0).then(|| Expansion { snippet: keyword.snippet, erase: keyword.chars.len(), after: None })
            } else if ended {
                self.ends_with(keyword, 1).then(|| Expansion { snippet: keyword.snippet, erase: keyword.chars.len() + 1, after: Some(last) })
            } else {
                None
            };
            let Some(found) = found else { continue };
            let better = best.as_ref().is_none_or(|(other, _)| (keyword.chars.len(), !keyword.delimited) > (other.chars.len(), !other.delimited));
            if better {
                best = Some((keyword, found));
            }
        }
        best.map(|(_, expansion)| expansion)
    }

    /// Whether what is held, less its last `after` characters, ends with the
    /// keyword, and not in the middle of a word if the keyword minds.
    fn ends_with(&self, keyword: &Keyword, after: usize) -> bool {
        let Some(start) = self.held.len().checked_sub(keyword.chars.len() + after) else { return false };
        let typed = self.held.iter().skip(start).zip(&keyword.chars).all(|(slot, c)| *slot == Slot::Char(*c));
        let inside = keyword.bounded
            && start.checked_sub(1).and_then(|before| self.held.get(before)).is_some_and(|slot| match slot {
                Slot::Char(c) => in_word(*c),
                Slot::Unknown | Slot::Unseen => true,
            });
        typed && !inside
    }

    /// Whether another expansion may happen at `now`; counts it if so.
    fn unhurried(&mut self, now: Instant) -> bool {
        while self.recent.front().is_some_and(|then| now.saturating_duration_since(*then) >= WINDOW) {
            self.recent.pop_front();
        }
        let hurried = self.recent.back().is_some_and(|then| now.saturating_duration_since(*then) < APART);
        if hurried || self.recent.len() >= BURST {
            return false;
        }
        self.recent.push_back(now);
        true
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.clear();
    }
}

// --- Acting on it ------------------------------------------------------------------

/// What is typed between a keyword's last character and its being erased:
/// replacing takes a moment, and nobody stops typing for it. Plain
/// characters stand after the keyword and are carried over to after the
/// text; anything else means the cursor is no longer where it was.
#[derive(Default)]
struct Meanwhile(Option<String>);

impl Meanwhile {
    /// More than this is somebody typing on regardless.
    const MOST: usize = 12;

    fn begin() -> Meanwhile {
        Meanwhile(Some(String::new()))
    }

    fn typed(&mut self, key: Typed) {
        match (key, &mut self.0) {
            (Typed::Char(c), Some(typed)) if typed.chars().count() < Meanwhile::MOST => typed.push(c),
            _ => self.forget(),
        }
    }

    fn forget(&mut self) {
        // Emptied in place before it is let go, like the engine's buffer.
        if let Some(typed) = self.0.as_mut() {
            typed.clear();
        }
        self.0 = None;
    }

    /// The characters to carry over, or that the keyword is to be left alone.
    fn carried(&self) -> Result<String, String> {
        self.0.clone().ok_or_else(|| "the cursor moved, or too much was typed, meanwhile".to_owned())
    }
}

/// A keyword is being replaced: what is typed meanwhile is not followed,
static BUSY: AtomicBool = AtomicBool::new(false);
/// only kept here until the keyword is erased.
static MEANWHILE: Mutex<Meanwhile> = Mutex::new(Meanwhile(None));

/// `BUSY` for as long as it lives, whatever becomes of the thread.
struct Busy;

impl Drop for Busy {
    fn drop(&mut self) {
        MEANWHILE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).forget();
        BUSY.store(false, Ordering::SeqCst);
    }
}
/// The palette's webview is filling in a snippet's placeholders: which
/// request, and where the answer goes.
static FILLING: Mutex<Option<(u64, Sender<String>)>> = Mutex::new(None);
static REQUESTS: AtomicU64 = AtomicU64::new(0);

/// The setting.
fn wanted(app: &AppHandle) -> bool {
    app.state::<SettingsState>().0.read().unwrap().snippet_expansion
}

/// The snippets as they are on disk, read again when the file changes.
struct Library {
    snippets: Vec<Snippet>,
    engine: Engine,
    changed: Option<SystemTime>,
    looked: Instant,
}

impl Library {
    fn stamp() -> Option<SystemTime> {
        std::fs::metadata(crate::paths::data().join("snippets.json")).and_then(|m| m.modified()).ok()
    }

    fn load() -> Library {
        let snippets = crate::library::snippets();
        Library { engine: Engine::new(&snippets), snippets, changed: Library::stamp(), looked: Instant::now() }
    }

    /// Looks at the file about once a second.
    fn refresh(&mut self) {
        if self.looked.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.looked = Instant::now();
        if Library::stamp() != self.changed {
            *self = Library::load();
        }
    }
}

/// Begins expanding snippets whenever the setting is on, for as long as
/// Heyo runs. Where typing cannot be followed this does nothing.
pub fn start(app: &AppHandle) {
    if !typed::supported() {
        return;
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("snippet-expansion".into()).spawn(move || loop {
        if !wanted(&app) {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        let library = std::cell::RefCell::new(Library::load());
        // Nothing to expand: nothing to listen for.
        if !library.borrow().engine.has_keywords() {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }
        eprintln!("heyo: snippet expansion: listening");
        let listen = || {
            let mut library = library.borrow_mut();
            library.refresh();
            wanted(&app) && library.engine.has_keywords()
        };
        let mut on = |key: Typed| {
            let mut library = library.borrow_mut();
            if BUSY.load(Ordering::SeqCst) {
                MEANWHILE.lock().unwrap().typed(key);
                return library.engine.clear();
            }
            let Some(expansion) = library.engine.typed(key, Instant::now()) else { return };
            let text = library.snippets[expansion.snippet].text.clone();
            *MEANWHILE.lock().unwrap() = Meanwhile::begin();
            BUSY.store(true, Ordering::SeqCst);
            let app = app.clone();
            std::thread::spawn(move || {
                let _busy = Busy;
                // Says why not, never what was typed or what the text is.
                if let Err(why) = replace(&app, text, &expansion) {
                    eprintln!("heyo: snippet expansion: left alone ({why})");
                }
            });
        };
        let result = typed::watch(&listen, &mut on);
        library.borrow_mut().engine.clear();
        match result {
            Ok(()) => eprintln!("heyo: snippet expansion: no longer listening"),
            Err(e) => {
                eprintln!("heyo: snippet expansion: cannot follow typing: {e}");
                std::thread::sleep(Duration::from_secs(30));
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("heyo: snippet expansion could not start: {e}");
    }
}

/// Why a keyword that was typed is left as it is. `None`: go ahead.
fn refusal(class: Option<&str>, own: bool, excluded: impl FnOnce(&str) -> bool) -> Option<&'static str> {
    match class {
        _ if own => Some("one of Heyo's own windows"),
        // The exclusions go by the window's class.
        None => Some("the window in front is not known"),
        Some(class) if class.to_lowercase().contains("heyo") => Some("one of Heyo's own windows"),
        Some(class) if excluded(class) => Some("an excluded application"),
        Some(_) => None,
    }
}

/// Whether snippets are never expanded in a window of that class: password
/// managers, and what the user listed.
fn excluded(app: &AppHandle, class: &str) -> bool {
    if crate::clipboard::ignored_app(Some(class)) {
        return true;
    }
    let listed = app.state::<SettingsState>().0.read().unwrap().snippet_expansion_ignored_apps.clone();
    if listed.is_empty() {
        return false;
    }
    let index = app.state::<crate::IndexState>();
    let index = index.0.read().unwrap();
    crate::clipboard::sources::ignored(class, &listed, &index.apps)
}

/// Replaces the keyword just typed with `text`: erases it and pastes.
/// Anything in doubt before the first Backspace means nothing is touched.
///
/// The Backspaces and the paste go out as one (`Typing::replace`). After
/// that the application fetches the clipboard, and a key pressed before it
/// has done so lands before the text; nothing can be done about that from
/// outside the application.
fn replace(app: &AppHandle, text: String, expansion: &Expansion) -> Result<(), String> {
    let meanwhile = || MEANWHILE.lock().unwrap().carried();
    let own = app.webview_windows().values().any(|window| window.is_focused().unwrap_or(false));
    if let Some(why) = refusal(windows::active_class().as_deref(), own, |class| excluded(app, class)) {
        return Err(why.into());
    }
    if typed::composing() {
        return Err("an input method may be composing".into());
    }
    let mut text = if text.contains('{') { filled(app, text)? } else { text };
    text.extend(expansion.after);
    if text.is_empty() || expansion.erase > HELD {
        return Err("nothing to insert".into());
    }
    // There is nowhere to ask: into a terminal the text goes on one line.
    let terminal = input::terminal_focused();
    let text = pasting::unasked(text, terminal);

    // Given back when this returns, however it went.
    let clipboard = pasting::borrow();
    let since = Instant::now();
    // What the clipboard was last given.
    let mut written: Option<String> = None;
    loop {
        if since.elapsed() > PATIENCE {
            return Err("typing went on, or a modifier key stayed down".to_owned());
        }
        // The clipboard gets the text and what was typed since, and a
        // moment to have it: the paste must not find the old contents.
        let whole = format!("{text}{}", meanwhile()?);
        if written.as_deref() != Some(&whole) {
            clipboard.put(&whole, None).map_err(|e| format!("the clipboard could not be written: {e}"))?;
            written = Some(whole);
            std::thread::sleep(SETTLE);
            continue;
        }
        // The last character may have needed Shift or AltGr, which would
        // make the Backspaces something else. It is let go in a moment,
        // usually.
        if typed::modifiers_held() {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        // Unless one more key came just now, this is it: from here on
        // nothing is carried over, and nothing turns back.
        let carried = {
            let mut meanwhile = MEANWHILE.lock().unwrap();
            let carried = meanwhile.carried()?;
            if !whole.ends_with(&carried) || whole.len() != text.len() + carried.len() {
                continue;
            }
            meanwhile.forget();
            carried.chars().count()
        };
        return typed::replace(expansion.erase + carried, terminal);
    }
}

/// What `EXPAND_SNIPPET` carries: a snippet's text for the palette to fill
/// in, and the number its answer (`snippet_expanded`) is known by.
#[derive(Clone, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub(crate) struct ExpandSnippet {
    #[ts(type = "number")]
    request: u64,
    text: String,
}

/// The snippet's text with its placeholders filled in. The palette's
/// webview does that for snippets chosen there (`shared/placeholders.ts`),
/// so it is asked: one piece of code decides what `{date}` means.
fn filled(app: &AppHandle, template: String) -> Result<String, String> {
    let (answer, answered) = channel();
    let request = REQUESTS.fetch_add(1, Ordering::SeqCst) + 1;
    *FILLING.lock().unwrap() = Some((request, answer));
    let asked = crate::palette::tell(app, crate::wire::event::EXPAND_SNIPPET, ExpandSnippet { request, text: template });
    let text = asked.and_then(|()| answered.recv_timeout(Duration::from_millis(1500)).map_err(|_| "the placeholders were not filled in in time".to_owned()));
    *FILLING.lock().unwrap() = None;
    text
}

/// The palette's answer to `EXPAND_SNIPPET`.
#[tauri::command]
pub(crate) fn snippet_expanded(request: u64, text: String) {
    let mut filling = FILLING.lock().unwrap();
    if filling.as_ref().is_some_and(|(asked, _)| *asked == request) {
        if let Some((_, answer)) = filling.take() {
            let _ = answer.send(text);
        }
    }
}

/// Turns expansion on or off and says which: this is run from the root
/// search or by a hotkey, with nothing on screen to show it.
pub fn toggle(app: &AppHandle) {
    if !typed::supported() {
        return crate::notify("Snippets cannot expand as you type here", &crate::platform::unsupported("Following what is typed"));
    }
    let on = !wanted(app);
    match crate::update_settings(app, |settings| settings.snippet_expansion = on) {
        Ok(()) if on => crate::notify("Snippet expansion is on", "Heyo follows what you type to replace the keywords of your snippets."),
        Ok(()) => crate::notify("Snippet expansion is off", "Heyo no longer follows what you type."),
        Err(e) => crate::notify("Could not change snippet expansion", &e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(keyword: &str) -> Snippet {
        Snippet { id: keyword.into(), name: keyword.into(), text: format!("<{keyword}>"), keyword: Some(keyword.into()) }
    }

    fn listening(keywords: &[&str]) -> Engine {
        Engine::new(&keywords.iter().map(|k| snippet(k)).collect::<Vec<_>>())
    }

    /// Types `text`, a key a second, and collects what expands. `⌫` is
    /// Backspace, `⏎` anything that breaks a word, `�` an unknown character.
    fn typing(engine: &mut Engine, text: &str) -> Vec<Expansion> {
        thread_local!(static CLOCK: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) });
        text.chars()
            .filter_map(|c| {
                let key = match c {
                    '⌫' => Typed::Backspace,
                    '⏎' => Typed::Break,
                    '�' => Typed::Unknown,
                    c => Typed::Char(c),
                };
                let now = CLOCK.get().unwrap_or_else(Instant::now) + Duration::from_secs(1);
                CLOCK.set(Some(now));
                engine.typed(key, now)
            })
            .collect()
    }

    fn immediate(snippet: usize, erase: usize) -> Expansion {
        Expansion { snippet, erase, after: None }
    }

    fn after(snippet: usize, erase: usize, delimiter: char) -> Expansion {
        Expansion { snippet, erase, after: Some(delimiter) }
    }

    #[test]
    fn a_keyword_with_a_symbol_expands_on_its_last_character() {
        let mut engine = listening(&[";sig", "addr;"]);
        assert_eq!(typing(&mut engine, "Best, ;sig"), [immediate(0, 4)]);
        assert_eq!(typing(&mut engine, "addr;"), [immediate(1, 5)]);
        // Anywhere, also against a word: nothing ordinary looks like it.
        assert_eq!(typing(&mut engine, "word;sig"), [immediate(0, 4)]);
        // Not before it is whole, and not something else.
        assert_eq!(typing(&mut engine, ";si ;sih ;Sig sig"), []);
    }

    #[test]
    fn a_keyword_of_letters_waits_for_the_word_to_end() {
        let mut engine = listening(&["sig"]);
        assert_eq!(typing(&mut engine, "sig"), []);
        assert_eq!(typing(&mut engine, " "), [after(0, 4, ' ')]);
        assert_eq!(typing(&mut engine, "my sig."), [after(0, 4, '.')]);
        assert_eq!(typing(&mut engine, "(sig)"), [after(0, 4, ')')]);
        // The word went on: it was another word.
        assert_eq!(typing(&mut engine, "⏎signal sigs sig_x sig2 "), []);
    }

    #[test]
    fn a_keyword_of_letters_does_not_expand_inside_a_word() {
        let mut engine = listening(&["sig", "addr;"]);
        assert_eq!(typing(&mut engine, "design desig "), []);
        assert_eq!(typing(&mut engine, "my_sig "), []);
        assert_eq!(typing(&mut engine, "⏎xaddr;"), []);
        assert_eq!(typing(&mut engine, "⏎x addr;"), [immediate(1, 5)]);
        // A character that is not known counts as part of a word.
        assert_eq!(typing(&mut engine, "�sig "), []);
        assert_eq!(typing(&mut engine, "� sig "), [after(0, 4, ' ')]);
    }

    #[test]
    fn an_unknown_character_is_part_of_no_keyword() {
        let mut engine = listening(&[";sig", "a b"]);
        assert_eq!(typing(&mut engine, ";s�g ;si�"), []);
        // A keyword may have a space in it.
        assert_eq!(typing(&mut engine, "⏎a b."), [after(1, 4, '.')]);
    }

    #[test]
    fn the_longest_keyword_completed_wins() {
        let mut engine = listening(&[":a;", "b:a;", "x:a;"]);
        assert_eq!(typing(&mut engine, "b:a;"), [immediate(1, 4)]);
        assert_eq!(typing(&mut engine, "c:a;"), [immediate(0, 3)]);
        // The longer one begins with a letter, so not inside a word; the
        // shorter one begins with a symbol and still expands.
        assert_eq!(typing(&mut engine, "ab:a;"), [immediate(0, 3)]);
        // One that waits for the word's end against one that does not.
        let mut engine = listening(&["sig", "g.", "xsig."]);
        assert_eq!(typing(&mut engine, "sig."), [after(0, 4, '.')]);
        assert_eq!(typing(&mut engine, "xsig."), [immediate(2, 5)]);
        assert_eq!(typing(&mut engine, "bag. a g."), [immediate(1, 2)]);
    }

    #[test]
    fn a_keyword_that_expands_at_once_hides_longer_ones_that_begin_with_it() {
        let mut engine = listening(&[";s", ";sig"]);
        assert_eq!(typing(&mut engine, ";sig"), [immediate(0, 2)]);
        // Of letters, both wait for the end and the right one is found.
        let mut engine = listening(&["ad", "addr"]);
        assert_eq!(typing(&mut engine, "addr "), [after(1, 5, ' ')]);
        assert_eq!(typing(&mut engine, "ad "), [after(0, 3, ' ')]);
    }

    #[test]
    fn capitals_matter() {
        let mut engine = listening(&[";Sig", "TY"]);
        assert_eq!(typing(&mut engine, ";sig ;SIG ty Ty "), []);
        assert_eq!(typing(&mut engine, ";Sig"), [immediate(0, 4)]);
        assert_eq!(typing(&mut engine, "TY "), [after(1, 3, ' ')]);
    }

    #[test]
    fn backspace_takes_back_a_character() {
        let mut engine = listening(&[";sig", "sig"]);
        assert_eq!(typing(&mut engine, ";sog⌫⌫ig"), [immediate(0, 4)]);
        assert_eq!(typing(&mut engine, "sigh⌫ "), [after(1, 4, ' ')]);
        // The keyword was there for a moment and was taken back.
        assert_eq!(typing(&mut engine, ";si⌫⌫⌫g"), []);
    }

    #[test]
    fn erasing_past_what_was_heard_leaves_the_cursor_in_a_word() {
        let mut engine = listening(&["sig", ";sig"]);
        // "desx|": the x is erased, then "sig " makes "dessig ".
        assert_eq!(typing(&mut engine, "⌫sig "), []);
        assert_eq!(typing(&mut engine, "⏎ab⌫⌫⌫⌫sig "), []);
        // Erased back to where listening began, and no further: that place
        // is a beginning as far as is known.
        assert_eq!(typing(&mut engine, "⏎ab⌫⌫sig "), [after(0, 4, ' ')]);
        // A keyword with a symbol does not mind where it is.
        assert_eq!(typing(&mut engine, "⏎⌫;sig"), [immediate(1, 4)]);
        // And after a space all is known again.
        assert_eq!(typing(&mut engine, "⌫ sig "), [after(0, 4, ' ')]);
    }

    #[test]
    fn whatever_breaks_a_word_forgets_what_was_typed() {
        let mut engine = listening(&[";sig", "sig"]);
        assert_eq!(typing(&mut engine, ";si⏎g"), []);
        assert_eq!(typing(&mut engine, "sig⏎ "), []);
        assert!(engine.held.len() <= 1);
        engine.typed(Typed::Break, Instant::now());
        assert!(engine.held.is_empty());
        // What follows a break stands at the beginning, as far as is known.
        assert_eq!(typing(&mut engine, "de⏎sig "), [after(1, 4, ' ')]);
    }

    #[test]
    fn nothing_is_held_after_an_expansion_and_little_before() {
        let mut engine = listening(&[";sig"]);
        typing(&mut engine, &"long text typed without a break ".repeat(20));
        assert_eq!(engine.held.len(), HELD);
        assert_eq!(typing(&mut engine, ";sig"), [immediate(0, 4)]);
        assert!(engine.held.is_empty());
    }

    #[test]
    fn keywords_need_not_be_ascii() {
        let mut engine = listening(&["päd", "；签名", "ñ!", "住所"]);
        assert_eq!(typing(&mut engine, "päd "), [after(0, 4, ' ')]);
        assert_eq!(typing(&mut engine, "späd "), []);
        assert_eq!(typing(&mut engine, "；签名"), [immediate(1, 3)]);
        assert_eq!(typing(&mut engine, "ñ!"), [immediate(2, 2)]);
        // Ideographs are letters: the keyword waits for the word's end.
        assert_eq!(typing(&mut engine, "住所。"), [after(3, 3, '。')]);
    }

    #[test]
    fn a_keyword_is_two_to_thirty_two_characters_on_one_line() {
        assert_eq!(usable(" ;sig "), Some(";sig".chars().collect()));
        assert_eq!(usable("ab").map(|k| k.len()), Some(2));
        assert_eq!(usable(&"é".repeat(MAX_KEYWORD)).map(|k| k.len()), Some(MAX_KEYWORD));
        for unusable in ["", " ", "a", ";", "a\nb", "a\tb", &"x".repeat(MAX_KEYWORD + 1)] {
            assert_eq!(usable(unusable), None, "{unusable:?}");
        }
        // The longest keyword and its neighbours fit in what is held.
        let longest = "k".repeat(MAX_KEYWORD);
        let mut engine = listening(&[&longest]);
        assert_eq!(typing(&mut engine, &format!("x {longest} ")), [after(0, MAX_KEYWORD + 1, ' ')]);
        assert_eq!(typing(&mut engine, &format!("k{longest} ")), []);
    }

    #[test]
    fn snippets_without_a_usable_keyword_or_text_are_not_listened_for() {
        let none = Snippet { keyword: None, ..snippet("x") };
        let empty = Snippet { text: String::new(), ..snippet(";empty") };
        let mut engine = Engine::new(&[none.clone(), snippet("a"), empty.clone()]);
        assert!(!engine.has_keywords());
        assert_eq!(typing(&mut engine, "a ;empty x "), []);
        // The index is the snippet's place in the list it came in.
        let mut engine = Engine::new(&[none, empty, snippet(";a"), snippet(";a"), snippet(";b")]);
        assert!(engine.has_keywords());
        assert_eq!(typing(&mut engine, ";a"), [immediate(2, 2)]);
        assert_eq!(typing(&mut engine, ";b"), [immediate(4, 2)]);
    }

    #[test]
    fn expansions_do_not_follow_each_other_at_once() {
        let mut engine = listening(&[";a"]);
        let t = Instant::now();
        let keyword = |engine: &mut Engine, at: Instant| {
            engine.typed(Typed::Char(';'), at);
            engine.typed(Typed::Char('a'), at)
        };
        assert!(keyword(&mut engine, t).is_some());
        // Faster than anyone types: something is feeding on itself.
        assert!(keyword(&mut engine, t + APART / 2).is_none());
        assert!(engine.held.is_empty());
        assert!(keyword(&mut engine, t + APART).is_some());
    }

    #[test]
    fn expansions_stop_after_a_burst_and_come_back() {
        let mut engine = listening(&[";a"]);
        let t = Instant::now();
        let mut expanded = |at: Instant| {
            engine.typed(Typed::Char(';'), at);
            engine.typed(Typed::Char('a'), at).is_some()
        };
        let step = Duration::from_millis(500);
        let count = (0..20).filter(|i| expanded(t + step * *i)).count();
        assert_eq!(count, BURST);
        // Once the burst is `WINDOW` in the past, expansions resume.
        assert!(!expanded(t + step * 19 + Duration::from_millis(300)));
        assert!(expanded(t + WINDOW + step * 20));
    }

    #[test]
    fn what_is_typed_while_a_keyword_is_replaced_is_carried_over_if_it_is_plain() {
        let mut meanwhile = Meanwhile::begin();
        assert_eq!(meanwhile.carried(), Ok(String::new()));
        "he ".chars().for_each(|c| meanwhile.typed(Typed::Char(c)));
        assert_eq!(meanwhile.carried().as_deref(), Ok("he "));
        // Anything that moves the cursor or erases, and it is off.
        for key in [Typed::Break, Typed::Backspace, Typed::Unknown] {
            let mut meanwhile = Meanwhile::begin();
            meanwhile.typed(Typed::Char('h'));
            meanwhile.typed(key);
            assert!(meanwhile.carried().is_err());
            // And stays off.
            meanwhile.typed(Typed::Char('i'));
            assert!(meanwhile.carried().is_err());
        }
        let mut meanwhile = Meanwhile::begin();
        (0..Meanwhile::MOST).for_each(|_| meanwhile.typed(Typed::Char('x')));
        assert_eq!(meanwhile.carried().map(|typed| typed.len()), Ok(Meanwhile::MOST));
        meanwhile.typed(Typed::Char('x'));
        assert!(meanwhile.carried().is_err());
        // Nothing is kept when no keyword is being replaced.
        assert!(Meanwhile::default().carried().is_err());
    }

    #[test]
    fn debug_output_never_shows_what_was_typed() {
        assert_eq!(format!("{:?}", Typed::Char('s')), "Char(…)");
    }

    #[test]
    fn nothing_expands_in_heyos_own_windows_or_where_the_window_is_not_known() {
        let never = |_: &str| false;
        assert_eq!(refusal(Some("firefox"), false, never), None);
        assert!(refusal(Some("firefox"), true, never).is_some());
        assert!(refusal(Some("Heyo"), false, never).is_some());
        assert!(refusal(None, false, never).is_some());
        assert!(refusal(Some("KeePassXC"), false, |class| class == "KeePassXC").is_some());
    }

    #[test]
    fn password_managers_are_excluded_without_being_listed() {
        for class in ["KeePassXC", "org.keepassxc.KeePassXC", "Bitwarden", "1Password", "Enpass"] {
            assert!(crate::clipboard::ignored_app(Some(class)), "{class}");
        }
        assert!(!crate::clipboard::ignored_app(Some("firefox")));
    }
}
