//! Pasting text into the application that has the focus.
//!
//! Heyo types nothing itself: it puts the text on the clipboard and presses
//! the application's paste shortcut. Every paste goes through here, whether
//! it comes from dictation, a snippet, an extension or the palette, so that
//! two things hold for all of them. So does text that takes the place of
//! what stands before the cursor (a snippet's keyword, dictated words being
//! corrected): Backspaces and a paste, pressed by `platform::typed`.
//!
//! # The clipboard is only borrowed
//!
//! Text pasted for the user without their having copied it (a dictated
//! sentence, a snippet expanded as it was typed, what an extension pastes
//! "and restores") goes onto a clipboard on loan ([`Loan`]). The text that
//! was there comes back once the application has read what it was given
//! ([`READ`] after the loan ends), and the clipboard history hears of
//! neither change. Of several loans close together only the last gives the
//! clipboard back, and what it gives back is what the user had before the
//! first. Should Heyo die in between, the borrowed text is left there and
//! nothing worse.
//!
//! Text the user asked to have copied and pasted (the palette's Paste
//! actions) is simply copied: it stays, and the history has it.
//!
//! # Terminals
//!
//! A terminal runs pasted text line by line as it lands, so several lines
//! from a snippet, an AI answer or an extension are several commands run
//! unseen. Where the palette is there to ask in, the user is asked first
//! ([`question_before`]; "don't ask again" is the `confirmTerminalPaste`
//! setting). Where it is not (dictation, a snippet expanding as it is
//! typed, Paste as Plain Text), the text goes in on one line
//! ([`Lines::Joined`]).

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::confirm::{self, Answer, Question};
use crate::platform::clipboard_watch::{read_text, write_html, write_text};
use crate::platform::input::{is_terminal, press_paste, terminal_focused};
use crate::platform::typed::{self, Lost, Writing};
use crate::*;

/// How long an application gets to read what was pasted before the
/// clipboard has its old text back.
const READ: Duration = Duration::from_millis(500);
/// How long the clipboard is given to have new text before a paste asks
/// it for it.
pub(crate) const SETTLE: Duration = Duration::from_millis(60);
/// How long the focus takes to be back in the window being pasted into
/// after the palette hid,
const FOCUS: Duration = Duration::from_millis(150);
/// and when the palette is only now told to hide.
const HIDING: Duration = Duration::from_millis(200);

/// What pasting needs besides the desktop: time to pass, and the clipboard
/// history not to listen.
pub(crate) trait World: Send + Sync {
    fn wait(&self, time: Duration);
    /// Runs `task` once `time` has passed; the caller does not wait for it.
    fn later(&self, time: Duration, task: Box<dyn FnOnce() + Send>);
    /// The clipboard change about to be made is Heyo's plumbing, not
    /// something the user copied.
    fn unrecorded(&self);
}

struct Real;

impl World for Real {
    fn wait(&self, time: Duration) {
        std::thread::sleep(time);
    }

    fn later(&self, time: Duration, task: Box<dyn FnOnce() + Send>) {
        std::thread::spawn(move || {
            std::thread::sleep(time);
            task();
        });
    }

    fn unrecorded(&self) {
        crate::clipboard::skip_next();
    }
}

/// What becomes of text of several lines on its way into a terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lines {
    /// It goes in on one line: nothing in it presses Enter.
    Joined,
    /// It goes in as it is: the user was asked, or wants no asking.
    Kept,
}

/// Whether a terminal would run (part of) this text as it is pasted: any
/// line end does it, also a single one at the very end.
fn runs_as_commands(text: &str) -> bool {
    text.contains(['\n', '\r'])
}

/// The text with its lines joined by spaces.
fn on_one_line(text: &str) -> String {
    text.split(['\n', '\r']).map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" ")
}

/// The text as it may go into a window without the user being asked: into
/// a terminal, on one line.
pub(crate) fn unasked(text: String, terminal: bool) -> String {
    if terminal && runs_as_commands(&text) {
        on_one_line(&text)
    } else {
        text
    }
}

/// Whether what is pasted lands in a terminal: in `window`, where it is
/// known which window gets the text, else in the one that has the focus.
/// Unknown counts as not.
pub(crate) fn into_terminal(window: Option<windows::Window>) -> bool {
    let Some(id) = window else { return terminal_focused() };
    windows::list(Some(id)).ok().and_then(|all| all.into_iter().find(|w| w.id == id)).is_some_and(|w| is_terminal(&w.class.to_lowercase()))
}

/// What to ask before `text` is pasted, if anything: it must be on its way
/// into a terminal that would run it, and the user must want to be asked
/// (`ask`, the setting). The question says how much would be pasted and how
/// it begins.
pub(crate) fn question_before(text: &str, ask: bool, terminal: impl FnOnce() -> bool) -> Option<Question> {
    const SHOWN: usize = 4;
    const WIDTH: usize = 48;
    if !(ask && runs_as_commands(text) && terminal()) {
        return None;
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut preview: Vec<String> = lines
        .iter()
        .take(SHOWN)
        .map(|line| match line.char_indices().nth(WIDTH) {
            Some((cut, _)) => format!("{}…", &line[..cut]),
            None => (*line).to_owned(),
        })
        .collect();
    if lines.len() > SHOWN {
        preview.push(format!("… and {} more", lines.len() - SHOWN));
    }
    let count = match lines.len() {
        0 | 1 => "a line and its line end".to_owned(),
        n => format!("{n} lines"),
    };
    Some(Question {
        title: format!("Paste {count} into the terminal?"),
        message: format!("A terminal runs each line as a command as soon as it is pasted.\n\n{}", preview.join("\n")),
        yes: "Paste",
        always: "Paste and Don't Ask Again",
    })
}

/// The one clipboard, and what of it is the user's while Heyo's text is on it.
pub(crate) struct Pasting {
    world: &'static dyn World,
    lent: Mutex<Lent>,
}

struct Lent {
    /// The user's text, from the first time Heyo's took its place until it
    /// is back. Empty where there was none: then nothing comes back.
    owed: Option<String>,
    /// Counts what was put on the borrowed clipboard: giving back is left
    /// to whoever put something there last.
    puts: u64,
}

impl Pasting {
    const fn new(world: &'static dyn World) -> Pasting {
        Pasting { world, lent: Mutex::new(Lent { owed: None, puts: 0 }) }
    }

    /// Held while the clipboard is read and written, so that a loan
    /// beginning and one ending cannot take each other's text for the user's.
    fn lent(&self) -> MutexGuard<'_, Lent> {
        self.lent.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Borrows the clipboard. Nothing is touched until something is put on it.
    pub(crate) fn borrow(&'static self) -> Loan {
        Loan { of: self }
    }

    /// Notes that the clipboard is about to be given text of Heyo's: what
    /// the user had is remembered, and the history told not to listen.
    fn lending(&self) -> MutexGuard<'_, Lent> {
        let mut lent = self.lent();
        // While Heyo's text is there, what the user had is known already.
        if lent.owed.is_none() {
            lent.owed = Some(read_text().unwrap_or_default());
        }
        lent.puts += 1;
        self.world.unrecorded();
        lent
    }

    fn put(&self, text: &str, html: Option<&str>) -> Result<(), String> {
        let _lent = self.lending();
        match html {
            Some(html) => write_html(html, text),
            None => write_text(text),
        }
    }

    /// Where the lending stands, if the clipboard is on loan at all.
    fn owing(&self) -> Option<u64> {
        let lent = self.lent();
        lent.owed.is_some().then_some(lent.puts)
    }

    /// Puts the user's text back, unless more was put on the clipboard
    /// since `puts`: then that loan sees to it.
    fn give_back(&self, puts: u64) -> Result<(), String> {
        let mut lent = self.lent();
        if lent.puts != puts {
            return Ok(());
        }
        let Some(owed) = lent.owed.take().filter(|owed| !owed.is_empty()) else { return Ok(()) };
        // What comes back may be a secret the history skipped the first time.
        self.world.unrecorded();
        write_text(&owed)
    }

    /// Presses the paste shortcut in the window that has the focus, once
    /// `after` has passed. With `Lines::Joined`, what is on the clipboard is
    /// first put on one line if that window is a terminal.
    pub(crate) fn press(&self, after: Duration, lines: Lines) -> Result<(), String> {
        self.world.wait(after);
        let terminal = terminal_focused();
        if terminal && lines == Lines::Joined {
            if let Some(text) = read_text().filter(|text| runs_as_commands(text)) {
                self.world.unrecorded();
                write_text(&on_one_line(&text))?;
                self.world.wait(SETTLE);
            }
        }
        press_paste(terminal)
    }
}

/// The clipboard on loan. What is put on it stands in for the user's text,
/// which comes back when the loan ends: when this is dropped, whatever
/// became of the paste.
pub(crate) struct Loan {
    of: &'static Pasting,
}

impl Loan {
    /// Puts text on the clipboard (with `html`, as rich text too), unheard
    /// by the history. The first time, what was there is remembered.
    pub(crate) fn put(&self, text: &str, html: Option<&str>) -> Result<(), String> {
        self.of.put(text, html)
    }

    /// Remembers what the user has on the clipboard now, where that must
    /// not take time later: reading it can, and `replace` is for the moment
    /// between two keys.
    pub(crate) fn ready(&self) {
        let mut lent = self.of.lent();
        if lent.owed.is_none() {
            lent.owed = Some(read_text().unwrap_or_default());
        }
    }

    /// Puts text on the borrowed clipboard and pastes it into the window
    /// that has the focus.
    pub(crate) fn paste(&self, text: &str, lines: Lines) -> Result<(), String> {
        self.put(text, None)?;
        self.of.press(SETTLE, lines)
    }

    /// Replaces the `erase` characters before the cursor of the window in
    /// front with `text`, which goes by way of the borrowed clipboard
    /// (`typed::replace`).
    pub(crate) fn replace(&self, erase: usize, text: &str, terminal: bool) -> Result<(), String> {
        drop(self.of.lending());
        typed::replace(erase, text, terminal)
    }

    /// The same for one piece of what `hand` is writing.
    pub(crate) fn rewrite(&self, hand: &mut dyn Writing, erase: usize, text: &str) -> Result<(), Lost> {
        // Erasing alone needs no clipboard.
        if !text.is_empty() {
            drop(self.of.lending());
        }
        hand.replace(erase, text)
    }

    /// Ends the loan and returns when the clipboard is the user's again,
    /// for where someone is told that it is.
    pub(crate) fn returned(self) -> Result<(), String> {
        let of = self.of;
        std::mem::forget(self);
        let Some(puts) = of.owing() else { return Ok(()) };
        // With no text to come back there is nothing to wait for.
        if of.lent().owed.as_deref() != Some("") {
            of.world.wait(READ);
        }
        of.give_back(puts)
    }
}

impl Drop for Loan {
    fn drop(&mut self) {
        let of = self.of;
        let Some(puts) = of.owing() else { return };
        of.world.later(
            READ,
            Box::new(move || {
                if let Err(e) = of.give_back(puts) {
                    eprintln!("heyo: could not put the clipboard back: {e}");
                }
            }),
        );
    }
}

static PASTING: Pasting = Pasting::new(&Real);

/// See [`Pasting::borrow`].
pub(crate) fn borrow() -> Loan {
    PASTING.borrow()
}

/// See [`Pasting::press`].
pub(crate) fn press(after: Duration, lines: Lines) -> Result<(), String> {
    PASTING.press(after, lines)
}

// --- With the palette ----------------------------------------------------------------

/// For a paste nobody waits to hear about.
fn logged(pasted: Result<(), String>) {
    if let Err(e) = pasted {
        eprintln!("heyo: could not paste: {e}");
    }
}

fn asks(app: &AppHandle) -> bool {
    app.state::<SettingsState>().0.read().unwrap().confirm_terminal_paste
}

/// Whether an answer says to paste; "don't ask again" is noted.
fn agreed(app: &AppHandle, answer: Answer) -> bool {
    if answer == Answer::Always {
        if let Err(e) = update_settings(app, |s| s.confirm_terminal_paste = false) {
            eprintln!("heyo: could not save the answer: {e}");
        }
    }
    answer != Answer::No
}

/// Hides the palette and pastes what is on the clipboard into the window
/// that gets the focus back. If the clipboard was `borrowed` for it, the
/// loan ends with the paste.
fn hide_and_paste(app: &AppHandle, borrowed: Option<Loan>) {
    palette::hide(app);
    std::thread::spawn(move || {
        logged(press(HIDING, Lines::Kept));
        drop(borrowed);
    });
}

/// Brings the palette back over the terminal that is in front to ask, and
/// pastes what is on the clipboard if the answer is yes. If the clipboard
/// was `borrowed` for it, the loan ends with the answer: after the paste,
/// or at once when there is to be none.
fn ask_over_terminal(app: &AppHandle, question: Question, borrowed: Option<Loan>) {
    // It is where the palette returns to.
    palette::remember_window(app);
    palette::bring_back(app);
    let asking = app.clone();
    confirm::ask(app, question, move |answer| {
        if agreed(&asking, answer) {
            hide_and_paste(&asking, borrowed);
        }
    });
}

/// Copies text, hides the palette and pastes into the window the user came
/// from; asks first where the text would run as commands.
pub(crate) fn copy_and_paste(app: &AppHandle, text: String) -> Result<(), String> {
    if let Some(question) = question_before(&text, asks(app), || into_terminal(palette::working_window(app))) {
        let asking = app.clone();
        confirm::ask(app, question, move |answer| {
            if agreed(&asking, answer) {
                match write_text(&text) {
                    Ok(()) => hide_and_paste(&asking, None),
                    Err(e) => notify("Could not paste", &e),
                }
            }
        });
        return Ok(());
    }
    write_text(&text)?;
    hide_and_paste(app, None);
    Ok(())
}

/// Pastes what an extension put on the clipboard (`host.paste`, after the
/// palette hid). Where it would run as commands the palette comes back to
/// ask first.
pub(crate) fn paste_clipboard(app: &AppHandle) {
    PASTING.world.wait(FOCUS);
    let text = terminal_focused().then(read_text).flatten().unwrap_or_default();
    match question_before(&text, asks(app), || true) {
        Some(question) => ask_over_terminal(app, question, None),
        None => logged(press(Duration::ZERO, Lines::Kept)),
    }
}

/// Pastes text for an extension into the window in front (the palette has
/// hidden) and returns when the clipboard has its old text back. Where the
/// paste has to be asked about first it returns with the question up: the
/// paste, and the clipboard's return after it, wait for the answer.
pub(crate) fn paste_and_give_back(app: &AppHandle, text: &str, html: Option<&str>) -> Result<(), String> {
    let loan = borrow();
    loan.put(text, html)?;
    PASTING.world.wait(FOCUS);
    if let Some(question) = question_before(text, asks(app), terminal_focused) {
        ask_over_terminal(app, question, Some(loan));
        return Ok(());
    }
    logged(press(Duration::ZERO, Lines::Kept));
    loan.returned()
}

/// Pastes the clipboard's text without what else was copied with it (the
/// palette is hiding). There is nowhere to ask: into a terminal it goes on
/// one line.
pub(crate) fn paste_plain() {
    let Some(text) = read_text().filter(|text| !text.is_empty()) else { return notify("Nothing to paste", "The clipboard has no text.") };
    // Writing only text drops every richer format the source offered.
    match write_text(&text) {
        Ok(()) => {
            std::thread::spawn(|| logged(press(FOCUS, Lines::Joined)));
        }
        Err(e) => notify("Could not paste", &e),
    }
}

/// For tests: time that passes when told to, and a clipboard of one's own.
#[cfg(test)]
pub(crate) mod scripted {
    use super::*;
    use crate::platform::desktop::fake::Fake;

    type Task = Box<dyn FnOnce() + Send>;

    /// Time that passes when told to. What is asked of it goes into the
    /// fake desktop's own record, so one list has all that happened in order.
    pub(crate) struct Scripted {
        desktop: &'static Fake,
        now: Mutex<Duration>,
        put_off: Mutex<Vec<(Duration, Task)>>,
    }

    impl Scripted {
        fn note(&self, what: String) {
            self.desktop.asked.lock().unwrap().push(what);
        }

        /// Lets time pass; what was put off until then happens, in order.
        pub(crate) fn pass(&self, time: Duration) {
            let now = {
                let mut now = self.now.lock().unwrap();
                *now += time;
                *now
            };
            loop {
                let due = {
                    let mut put_off = self.put_off.lock().unwrap();
                    let first = put_off.iter().enumerate().filter(|(_, (at, _))| *at <= now).min_by_key(|(_, (at, _))| *at).map(|(i, _)| i);
                    first.map(|i| put_off.remove(i).1)
                };
                match due {
                    Some(task) => task(),
                    None => break,
                }
            }
        }
    }

    impl World for Scripted {
        fn wait(&self, time: Duration) {
            if !time.is_zero() {
                self.note(format!("wait {}", time.as_millis()));
            }
            self.pass(time);
        }

        fn later(&self, time: Duration, task: Task) {
            let at = *self.now.lock().unwrap() + time;
            self.put_off.lock().unwrap().push((at, task));
        }

        fn unrecorded(&self) {
            self.note("unrecorded".into());
        }
    }

    /// A desktop with a window of `class` in front and `copied` on its clipboard.
    pub(crate) fn desk(class: &str, copied: Option<&str>) -> (&'static Fake, &'static Scripted, &'static Pasting) {
        let desktop = Fake::with_windows(&[(1, class)]);
        *desktop.clipboard.lock().unwrap() = copied.map(str::to_owned);
        let world: &'static Scripted = Box::leak(Box::new(Scripted { desktop, now: Mutex::default(), put_off: Mutex::default() }));
        (desktop, world, Box::leak(Box::new(Pasting::new(world))))
    }

    /// What happened since this was last asked.
    pub(crate) fn happened(desktop: &Fake) -> Vec<String> {
        std::mem::take(&mut *desktop.asked.lock().unwrap())
    }

    pub(crate) fn clipboard(desktop: &Fake) -> Option<String> {
        desktop.clipboard.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::scripted::{clipboard, desk, happened};
    use super::*;
    use crate::platform::desktop::fake::Fake;

    const MS: Duration = Duration::from_millis(1);
    const NOTHING: [&str; 0] = [];

    #[test]
    fn a_paste_on_loan_gives_the_clipboard_back_and_the_history_hears_neither_change() {
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            let loan = pasting.borrow();
            // Borrowing touches nothing.
            assert_eq!(happened(desktop), NOTHING);
            loan.put("Hello.", None).unwrap();
            pasting.press(SETTLE, Lines::Joined).unwrap();
            assert_eq!(happened(desktop), ["unrecorded", "copy Hello.", "wait 60", "paste"]);
            drop(loan);
            // The application is still reading what it was given.
            world.pass(READ - MS);
            assert_eq!(clipboard(desktop).as_deref(), Some("Hello."));
            world.pass(MS);
            assert_eq!(happened(desktop), ["unrecorded", "copy the user's"]);
            // And that was that.
            world.pass(READ * 4);
            assert_eq!(happened(desktop), NOTHING);
        });
    }

    #[test]
    fn rich_text_is_lent_with_its_plain_text() {
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            let loan = pasting.borrow();
            loan.put("bold", Some("<b>bold</b>")).unwrap();
            assert_eq!(happened(desktop), ["unrecorded", "copy bold as <b>bold</b>"]);
            drop(loan);
            world.pass(READ);
            assert_eq!(clipboard(desktop).as_deref(), Some("the user's"));
        });
    }

    #[test]
    fn a_paste_that_fails_still_gives_the_clipboard_back() {
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        *desktop.paste_fails.lock().unwrap() = Some("no keystroke tool".into());
        desktop.during(|| {
            let pasted = (|| {
                let loan = pasting.borrow();
                loan.put("Hello.", None)?;
                pasting.press(SETTLE, Lines::Joined)
            })();
            assert_eq!(pasted, Err("no keystroke tool".into()));
            world.pass(READ);
            assert_eq!(clipboard(desktop).as_deref(), Some("the user's"));
        });
    }

    #[test]
    fn of_two_pastes_close_together_the_second_gives_back_what_the_user_had() {
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            let first = pasting.borrow();
            first.put("one", None).unwrap();
            pasting.press(SETTLE, Lines::Joined).unwrap();
            drop(first);
            world.pass(MS * 200);
            // "one" is on the clipboard now, and is not what the user had.
            let second = pasting.borrow();
            second.put("two", None).unwrap();
            pasting.press(SETTLE, Lines::Joined).unwrap();
            drop(second);
            happened(desktop);
            // The first loan's time is up while the second paste is being read.
            world.pass(MS * 300);
            assert_eq!(happened(desktop), NOTHING);
            assert_eq!(clipboard(desktop).as_deref(), Some("two"));
            world.pass(MS * 200);
            assert_eq!(happened(desktop), ["unrecorded", "copy the user's"]);
        });
    }

    #[test]
    fn a_loan_lasts_over_as_many_pastes_as_it_is_kept_for() {
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            // Dictation: a sentence now and then, one loan for the recording.
            let recording = pasting.borrow();
            recording.put("First sentence.", None).unwrap();
            world.pass(Duration::from_secs(5));
            assert_eq!(clipboard(desktop).as_deref(), Some("First sentence."));
            // A snippet expands meanwhile and is done.
            let snippet = pasting.borrow();
            snippet.put("Kind regards", None).unwrap();
            drop(snippet);
            world.pass(READ);
            assert_eq!(clipboard(desktop).as_deref(), Some("the user's"));
            recording.put("Second sentence.", None).unwrap();
            drop(recording);
            world.pass(READ);
            assert_eq!(clipboard(desktop).as_deref(), Some("the user's"));
        });
    }

    #[test]
    fn returning_a_loan_waits_until_the_clipboard_is_the_users_again() {
        let (desktop, _, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            let loan = pasting.borrow();
            loan.put("Hello.", None).unwrap();
            happened(desktop);
            loan.returned().unwrap();
            assert_eq!(happened(desktop), ["wait 500", "unrecorded", "copy the user's"]);
        });
    }

    #[test]
    fn what_is_not_on_loan_stays() {
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            // The palette's Paste: a copy like any other, then the keystroke.
            write_text("an answer").unwrap();
            pasting.press(HIDING, Lines::Kept).unwrap();
            world.pass(READ * 4);
            assert_eq!(happened(desktop), ["copy an answer", "wait 200", "paste"]);
            // A loan that put nothing there has nothing to give back,
            drop(pasting.borrow());
            pasting.borrow().returned().unwrap();
            world.pass(READ * 4);
            assert_eq!(happened(desktop), NOTHING);
            // nor has one that found no text there.
            *desktop.clipboard.lock().unwrap() = None;
            let loan = pasting.borrow();
            loan.put("over a picture", None).unwrap();
            loan.returned().unwrap();
            assert_eq!(happened(desktop), ["unrecorded", "copy over a picture"]);
        });
    }

    #[test]
    fn into_a_terminal_text_goes_on_one_line_where_nobody_can_be_asked() {
        let (desktop, world, pasting) = desk("org.gnome.Ptyxis", Some("the user's"));
        desktop.during(|| {
            let loan = pasting.borrow();
            loan.put("First paragraph.\n\n  Second one.\r\n", None).unwrap();
            pasting.press(SETTLE, Lines::Joined).unwrap();
            assert_eq!(
                happened(desktop),
                ["unrecorded", "copy First paragraph.\n\n  Second one.\r\n", "wait 60", "unrecorded", "copy First paragraph. Second one.", "wait 60", "paste terminal"]
            );
            drop(loan);
            world.pass(READ);
            assert_eq!(happened(desktop), ["unrecorded", "copy the user's"]);
            // One line runs nothing by itself, and is left as it is.
            write_text("  ls -la").unwrap();
            pasting.press(Duration::ZERO, Lines::Joined).unwrap();
            assert_eq!(happened(desktop), ["copy   ls -la", "paste terminal"]);
            // The user said yes to these lines: they go in as lines.
            write_text("cd /tmp\nls\n").unwrap();
            pasting.press(Duration::ZERO, Lines::Kept).unwrap();
            assert_eq!(happened(desktop), ["copy cd /tmp\nls\n", "paste terminal"]);
        });
        // An editor takes lines as lines.
        let (desktop, _, pasting) = desk("Code", None);
        desktop.during(|| {
            write_text("a\nb").unwrap();
            pasting.press(Duration::ZERO, Lines::Joined).unwrap();
            assert_eq!(happened(desktop), ["copy a\nb", "paste"]);
        });
        // Text that is put together before it is written (a snippet as it expands).
        assert_eq!(unasked("a\r\nb\rc\n".into(), true), "a b c");
        assert_eq!(unasked("\n\n".into(), true), "");
        assert_eq!(unasked("a\nb".into(), false), "a\nb");
    }

    #[test]
    fn several_lines_on_their_way_into_a_terminal_are_asked_about() {
        let asked = |text: &str, ask: bool, terminal: bool| question_before(text, ask, || terminal).is_some();
        for runs in ["ls\nrm -rf build", "ls\n", "ls\r", "a\r\nb"] {
            assert!(asked(runs, true, true), "{runs:?}");
            // An editor takes lines as lines.
            assert!(!asked(runs, true, false), "{runs:?}");
            // "Don't ask again".
            assert!(!asked(runs, false, true), "{runs:?}");
        }
        // One line runs nothing by itself.
        for harmless in ["rm -rf build", "echo one; echo two", ""] {
            assert!(!asked(harmless, true, true), "{harmless:?}");
        }
        // Which window it is going into is only looked up when it matters.
        assert!(question_before("one line", true, || panic!("not asked")).is_none());

        // With the palette in front, that is the window the user came from.
        let desktop = Fake::with_windows(&[(1, "firefox"), (2, "kitty")]);
        desktop.during(|| {
            assert!(!into_terminal(None));
            assert!(!into_terminal(Some(1)));
            assert!(into_terminal(Some(2)));
            assert!(!into_terminal(Some(3)));
            assert!(question_before("ls\nrm -rf build", true, || into_terminal(Some(2))).is_some());
        });
    }

    #[test]
    fn the_question_shows_how_much_and_how_it_begins() {
        let question = |text: &str| question_before(text, true, || true).unwrap();
        let asked = question("cd /tmp\nrm -rf *\n");
        assert_eq!(asked.title, "Paste 2 lines into the terminal?");
        assert!(asked.message.ends_with("cd /tmp\nrm -rf *"));

        assert_eq!(question("make install\n").title, "Paste a line and its line end into the terminal?");

        let long = (1..=9).map(|n| format!("echo {n}")).collect::<Vec<_>>().join("\n");
        let asked = question(&long);
        assert_eq!(asked.title, "Paste 9 lines into the terminal?");
        assert!(asked.message.ends_with("echo 4\n… and 5 more"));

        // A long line is cut between characters, not inside one.
        let wide = format!("{}\nb", "宽".repeat(200));
        let shown = question(&wide).message;
        let line = shown.lines().nth(2).unwrap();
        assert_eq!(line.chars().count(), 49);
        assert!(line.ends_with('…'));
    }

    #[test]
    fn text_that_replaces_what_stands_there_is_on_loan_too() {
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            desktop.user_types("Best, ;sig");
            let loan = pasting.borrow();
            // Reading the clipboard is done ahead of the moment that counts.
            loan.ready();
            assert_eq!(happened(desktop), NOTHING);
            loan.replace(4, "Ada", false).unwrap();
            assert_eq!(happened(desktop), ["unrecorded", "replace 4 with Ada"]);
            assert_eq!(desktop.field(), "Best, Ada");
            drop(loan);
            world.pass(READ);
            assert_eq!(happened(desktop), ["unrecorded", "copy the user's"]);
        });
    }

    #[test]
    fn pieces_written_one_after_the_other_borrow_the_clipboard_once() {
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            let loan = pasting.borrow();
            let mut hand = typed::write(&[], Default::default()).unwrap();
            loan.rewrite(hand.as_mut(), 0, "Helo").unwrap();
            loan.rewrite(hand.as_mut(), 1, "lo").unwrap();
            // Erasing alone puts nothing on the clipboard.
            loan.rewrite(hand.as_mut(), 2, "").unwrap();
            assert_eq!(happened(desktop), ["write", "unrecorded", "rewrite 0 with Helo", "unrecorded", "rewrite 1 with lo", "rewrite 2 with "]);
            assert_eq!(desktop.field(), "Hel");
            // A sentence pasted the ordinary way, on the same loan.
            loan.paste("lo.", Lines::Joined).unwrap();
            assert_eq!(happened(desktop), ["unrecorded", "copy lo.", "wait 60", "paste"]);
            assert_eq!(desktop.field(), "Hello.");
            drop(loan);
            world.pass(READ);
            assert_eq!(clipboard(desktop).as_deref(), Some("the user's"));
        });
    }
}
