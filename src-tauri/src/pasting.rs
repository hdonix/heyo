//! Pasting for the user from the palette, with care for terminals.
//!
//! A terminal runs pasted text line by line as it lands, so several lines
//! from a snippet, an AI answer or an extension are several commands run
//! unseen. Before such a paste the palette asks; "don't ask again" is the
//! `confirmTerminalPaste` setting. (Dictation has no palette to ask in:
//! what it pastes into a terminal goes in as one line, see `platform::input`.)

use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::confirm::{self, Answer, Question};
use crate::platform::clipboard_watch;
use crate::platform::input::{is_terminal, paste_as_is, runs_as_commands, terminal_focused};
use crate::*;

/// Whether to ask before pasting `text` into a window that is (or is not)
/// a terminal, given the setting.
pub(crate) fn needs_asking(text: &str, terminal: bool, ask: bool) -> bool {
    ask && terminal && runs_as_commands(text)
}

/// What the palette asks: how much would be pasted, and how it begins.
fn question(text: &str) -> Question {
    const SHOWN: usize = 4;
    const WIDTH: usize = 48;
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
    Question {
        title: format!("Paste {count} into the terminal?"),
        message: format!("A terminal runs each line as a command as soon as it is pasted.\n\n{}", preview.join("\n")),
        yes: "Paste",
        always: "Paste and Don't Ask Again",
    }
}

fn asks(app: &AppHandle) -> bool {
    app.state::<SettingsState>().0.read().unwrap().confirm_terminal_paste
}

/// Whether the window the text would land in is a terminal. With the
/// palette in front that is the window the user came from.
fn target_is_terminal(app: &AppHandle) -> bool {
    let Some(id) = working_window(app) else { return terminal_focused() };
    windows::list(Some(id)).ok().and_then(|all| all.into_iter().find(|w| w.id == id)).is_some_and(|w| is_terminal(&w.class.to_lowercase()))
}

/// Carries out an answer: `paste` runs for Yes and Always.
fn on_answer(app: AppHandle, answer: Answer, paste: impl FnOnce(&AppHandle)) {
    match answer {
        Answer::No => return,
        Answer::Always => {
            if let Err(e) = update_settings(&app, |s| s.confirm_terminal_paste = false) {
                eprintln!("heyo: could not save the answer: {e}");
            }
        }
        Answer::Yes => {}
    }
    paste(&app);
}

/// Hides the palette and pastes what is on the clipboard into the window
/// that gets the focus back.
fn hide_and_paste(app: &AppHandle) {
    let _ = main_window(app).hide();
    std::thread::spawn(|| paste_as_is(Duration::from_millis(200)));
}

/// Copies text, hides the palette and pastes into the window the user came
/// from; asks first where the text would run as commands.
pub(crate) fn copy_and_paste(app: &AppHandle, text: String) -> Result<(), String> {
    if runs_as_commands(&text) && needs_asking(&text, target_is_terminal(app), asks(app)) {
        let asking = app.clone();
        confirm::ask(app, question(&text), move |answer| {
            on_answer(asking, answer, |app| match clipboard_watch::write_text(&text) {
                Ok(()) => hide_and_paste(app),
                Err(e) => notify("Could not paste", &e),
            });
        });
        return Ok(());
    }
    clipboard_watch::write_text(&text)?;
    hide_and_paste(app);
    Ok(())
}

/// Pastes what an extension put on the clipboard (`host.paste`, after the
/// palette hid). Where it would run as commands the palette comes back to
/// ask first.
pub(crate) fn paste_clipboard(app: &AppHandle) {
    // Wait for the focus to return to the window being pasted into.
    std::thread::sleep(Duration::from_millis(150));
    let text = terminal_focused().then(clipboard_watch::read_text).flatten().unwrap_or_default();
    if !needs_asking(&text, true, asks(app)) {
        return paste_as_is(Duration::ZERO);
    }
    // The terminal is in front now; it is where the palette returns to.
    if let Some(terminal) = windows::active_external() {
        *app.state::<windows::LastWindow>().0.lock().unwrap() = Some(terminal);
    }
    let window = main_window(app);
    let _ = window.show();
    focus(&window);
    let asking = app.clone();
    confirm::ask(app, question(&text), move |answer| on_answer(asking, answer, hide_and_paste));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_several_lines_into_a_terminal_are_asked_about() {
        let cases = [
            ("ls\nrm -rf build", true, true, true),
            ("ls\n", true, true, true),
            ("a\r\nb", true, true, true),
            // One line runs nothing by itself.
            ("rm -rf build", true, true, false),
            // An editor takes lines as lines.
            ("ls\nrm -rf build", false, true, false),
            // "Don't ask again".
            ("ls\nrm -rf build", true, false, false),
            ("", true, true, false),
        ];
        for (text, terminal, ask, expected) in cases {
            assert_eq!(needs_asking(text, terminal, ask), expected, "{text:?} terminal={terminal} ask={ask}");
        }
    }

    #[test]
    fn the_question_shows_how_much_and_how_it_begins() {
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
}
