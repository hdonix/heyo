//! The settings window itself: made on Slint's thread, filled from the
//! model, and what is done in it carried to the host.
//!
//! Everything here runs on Slint's thread except what is asked of the host:
//! a command may wait on other programs (`gsettings`, Node.js), and it runs
//! the host's own code under the host's own locks, so it is called on a
//! thread of its own (`asking`), and its answer is brought back to Slint's.
//! Slint's thread therefore never holds anything of the host's, which is
//! what lets it be given up on if it panics (`guard`).
//!
//! Nothing is decided here: a control that was used is handed to the model
//! (`Model::acted`), which says what to ask of the host; the answer is
//! handed back to it (`Model::answered`); and the window is shown what the
//! model then says (`push`).

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use serde_json::Value;
use slint::winit_030::winit::dpi::PhysicalPosition;
use slint::winit_030::winit::event::{ElementState, WindowEvent};
use slint::winit_030::winit::keyboard::{KeyCode, PhysicalKey};
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, ModelNotify, ModelRc, ModelTracker, SharedString, VecModel};

use super::host::Host;
use super::keys::{self, Held};
use super::model::{Ask, Line, Model, Then};
use super::{area, guard, icons, ui};

/// The renderer the window is drawn with unless `HEYO_SETTINGS_RENDERER`
/// names another of Slint's: Skia, on the CPU.
const RENDERER: &str = "skia-software";

type Job = Box<dyn FnOnce(&dyn Host) + Send>;

/// For measuring: with `HEYO_SETTINGS_FRAMES` naming a file, how long each
/// frame took is written there when the window is closed, a line a frame:
/// when it began (the system's clock, in ms) and how long it took (ms), from
/// winit asking for the window to be drawn to winit's loop having nothing
/// more to do. Slint draws, and hands the frame to the display, in between.
mod frames {
    use std::cell::{Cell, RefCell};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    use slint::winit_030::winit::event_loop::ActiveEventLoop;
    use slint::winit_030::{CustomApplicationHandler, EventResult};

    thread_local! {
        static BEGUN: Cell<Option<(Instant, SystemTime)>> = const { Cell::new(None) };
        static TAKEN: RefCell<Vec<(u128, f64)>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) fn wanted() -> Option<String> {
        std::env::var("HEYO_SETTINGS_FRAMES").ok().filter(|file| !file.is_empty())
    }

    /// winit asked for the window to be drawn.
    pub(super) fn begun() {
        BEGUN.set(Some((Instant::now(), SystemTime::now())));
    }

    pub(super) struct Timing;

    impl CustomApplicationHandler for Timing {
        fn about_to_wait(&mut self, _: &ActiveEventLoop) -> EventResult {
            if let Some((begun, at)) = BEGUN.take() {
                let at = at.duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis());
                TAKEN.with_borrow_mut(|taken| taken.push((at, begun.elapsed().as_secs_f64() * 1e3)));
            }
            EventResult::Propagate
        }
    }

    /// Writes down the frames so far, and starts again.
    pub(super) fn write() {
        let Some(file) = wanted() else { return };
        let lines: String = TAKEN.with_borrow_mut(|taken| taken.drain(..).map(|(at, took)| format!("{at} {took:.3}\n")).collect());
        if let Err(e) = std::fs::write(&file, lines) {
            eprintln!("heyo: {file}: {e}");
        }
    }
}

/// What is shown for an icon the model names: a file's picture, or a
/// symbol (which is drawn in the colour of text). Each is read once.
#[derive(Default)]
struct Pictures(RefCell<HashMap<String, Option<(slint::Image, bool)>>>);

impl Pictures {
    fn of(&self, icon: &str) -> Option<(slint::Image, bool)> {
        self.0
            .borrow_mut()
            .entry(icon.to_string())
            .or_insert_with(|| {
                let path = std::path::Path::new(icon);
                if path.is_absolute() {
                    return slint::Image::load_from_path(path).ok().map(|picture| (picture, false));
                }
                slint::Image::load_from_svg_data(icons::svg(icon)?.as_bytes()).ok().map(|symbol| (symbol, true))
            })
            .clone()
    }
}

fn strings(items: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(items.iter().map(SharedString::from).collect::<Vec<_>>()))
}

/// The lines of the page, as Slint asks for them: a line is made, and its
/// icon read, when it comes into sight.
struct Lines {
    lines: RefCell<Vec<Line>>,
    pictures: Rc<Pictures>,
    notify: ModelNotify,
}

impl Lines {
    /// Shows these lines, telling Slint only what is different: the lines
    /// that begin and end both the old page and the new stay (changed, where
    /// they changed), and what is between them is taken out and put in. So
    /// a control that is being used stays the control it was (a switch
    /// moves, a field keeps what is typed into it), a list that is opened
    /// further down stays where it was scrolled to, and another page is
    /// not left scrolled to where the last one was: Slint's list keeps its
    /// place by the number of its first row, and forgets it only when that
    /// row is taken out.
    fn show(&self, lines: Vec<Line>) {
        let mut shown = self.lines.borrow_mut();
        let alike = |was: &Line, is: &Line| was.kind == is.kind && was.id == is.id && was.label == is.label;
        let head = shown.iter().zip(&lines).take_while(|(was, is)| alike(was, is)).count();
        let most = shown.len().min(lines.len()) - head;
        let tail = shown.iter().rev().zip(lines.iter().rev()).take(most).take_while(|(was, is)| alike(was, is)).count();
        let (out, come) = (shown.len() - head - tail, lines.len() - head - tail);
        let changed: Vec<usize> = (0..head).filter(|at| shown[*at] != lines[*at]).chain((0..tail).filter(|back| shown[shown.len() - 1 - back] != lines[lines.len() - 1 - back]).map(|back| lines.len() - 1 - back)).collect();
        // Slint asks for rows while it is told of them: first what is no
        // longer there goes, then the lines are the new ones.
        if out > 0 {
            shown.drain(head..head + out);
            drop(shown);
            self.notify.row_removed(head, out);
            shown = self.lines.borrow_mut();
        }
        *shown = lines;
        drop(shown);
        if come > 0 {
            self.notify.row_added(head, come);
        }
        for at in changed {
            self.notify.row_changed(at);
        }
    }
}

impl slint::Model for Lines {
    type Data = ui::Line;

    fn row_count(&self) -> usize {
        self.lines.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<ui::Line> {
        let lines = self.lines.borrow();
        let line = lines.get(row)?;
        // An extension that brings no icon has the one all extensions share.
        let icon = line.icon.as_deref().and_then(|icon| self.pictures.of(icon)).or_else(|| (line.kind == "head").then(|| self.pictures.of("blocks-16")).flatten());
        Some(ui::Line {
            kind: line.kind.into(),
            id: line.id.as_str().into(),
            first: line.first,
            last: line.last,
            space: line.space,
            indent: line.indent,
            label: line.label.as_str().into(),
            description: line.description.as_str().into(),
            danger: line.danger,
            has_icon: icon.is_some(),
            tinted: icon.as_ref().is_some_and(|(_, tinted)| *tinted),
            icon: icon.map(|(picture, _)| picture).unwrap_or_default(),
            control: line.control.into(),
            on: line.on,
            mixed: line.mixed,
            value: line.value.as_str().into(),
            placeholder: line.placeholder.as_str().into(),
            choices: ModelRc::new(VecModel::from(line.choices.iter().map(|(value, title)| ui::Choice { value: value.into(), title: title.into() }).collect::<Vec<_>>())),
            hotkey: line.hotkey.as_str().into(),
            keys: strings(&line.keys),
            more: line.more.as_str().into(),
            alias: line.alias.as_str().into(),
            checkable: line.checkable,
            open: line.open,
            busy: line.busy,
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The window while Slint runs.
struct Live {
    window: ui::SettingsWindow,
    asking: mpsc::Sender<Job>,
    model: Model,
    lines: Rc<Lines>,
    pictures: Rc<Pictures>,
    /// The hotkey field that is recording, and the modifiers held.
    recording: Option<String>,
    held: Held,
    /// Asks again, while something is under way that the page counts down.
    /// Kept, so that it goes on asking.
    _poll: slint::Timer,
}

thread_local! {
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
}

/// Does `work` with the window, on Slint's thread.
fn with<R>(work: impl FnOnce(&mut Live) -> R) -> Option<R> {
    LIVE.with(|live| match live.try_borrow_mut() {
        Ok(mut live) => live.as_mut().map(work),
        Err(_) => {
            eprintln!("heyo: the settings window was asked for something while it was doing another");
            None
        }
    })
}

impl Live {
    /// Asks the host, one thing after another on the thread kept for that,
    /// and hands each answer to the model back here; what the model then
    /// wants asked is asked too.
    fn ask(&self, asks: Vec<Ask>) {
        for Ask { command, args, then } in asks {
            let _ = self.asking.send(Box::new(move |host| {
                let answer = host.call(command, args);
                let _ = slint::invoke_from_event_loop(move || {
                    with(|live| live.answered(then, answer));
                });
            }));
        }
    }

    fn answered(&mut self, then: Then, answer: Result<Value, String>) {
        let copied = then == Then::Copied && answer.is_ok();
        let more = self.model.answered(then, answer);
        self.ask(more);
        self.push();
        if copied {
            // "Copied" is said for a moment.
            slint::Timer::single_shot(Duration::from_millis(2500), || {
                with(|live| {
                    live.model.copied = false;
                    live.push();
                });
            });
        }
    }

    /// Shows what the model says.
    fn push(&mut self) {
        let host = self.window.global::<ui::Host>();
        let (entries, pages) = self.model.nav();
        let selected = entries.iter().position(|entry| entry.id == self.model.page).map_or(-1, |at| at as i32);
        let nav: Vec<ui::NavItem> = entries
            .iter()
            .map(|entry| {
                let icon = entry.icon.as_deref().and_then(|icon| self.pictures.of(icon)).or_else(|| self.pictures.of("blocks-16"));
                ui::NavItem { id: entry.id.as_str().into(), title: entry.title.as_str().into(), extension: entry.extension, tinted: icon.as_ref().is_some_and(|(_, tinted)| *tinted), icon: icon.map(|(picture, _)| picture).unwrap_or_default() }
            })
            .collect();
        host.set_nav(ModelRc::new(VecModel::from(nav)));
        host.set_nav_pages(pages as i32);
        host.set_nav_selected(selected);
        host.set_page(self.model.page.as_str().into());
        host.set_title(self.model.title().into());
        host.set_error(self.model.error.as_str().into());
        host.set_hotkeys(self.model.hotkeys_supported());
        host.set_find_placeholder(self.model.find_placeholder().into());
        host.set_find_text(self.model.find.as_str().into());
        let (title, ticked, picks) = self.model.filter().unwrap_or_default();
        host.set_filter_title(title.into());
        host.set_filter_count(ticked as i32);
        host.set_filter_picks(ModelRc::new(VecModel::from(picks.into_iter().map(|(id, title, heading, checked)| ui::Pick { id: id.into(), title: title.into(), heading, checked }).collect::<Vec<_>>())));
        self.lines.show(self.model.lines());
        self.window.set_appearance(self.model.settings.get("appearance").and_then(Value::as_str).unwrap_or("system").into());
    }

    fn push_recording(&mut self) {
        let host = self.window.global::<ui::Host>();
        host.set_recording(self.recording.as_deref().unwrap_or_default().into());
        host.set_held(strings(&self.held.caps()));
    }

    fn stop_recording(&mut self) {
        if self.recording.take().is_some() {
            self.push_recording();
        }
    }

    /// Has the window drawn at the next turn of the loop, not at Slint's
    /// next tick. Slint paces drawing with a timer that first fires a
    /// frame's time after something changed (on X11 and Windows): a key or
    /// a click would be seen a frame late. This is asked for when one came.
    fn draw_soon(&self) {
        let window = self.window.as_weak();
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(window) = window.upgrade() {
                window.window().with_winit_window(|window| window.request_redraw());
            }
        });
    }

    /// What winit tells the window, before Slint has it. The keyboard above
    /// all: while a hotkey field records, a key press is the hotkey, by
    /// where the key is.
    fn told(&mut self, event: &WindowEvent) -> EventResult {
        match event {
            WindowEvent::RedrawRequested => {
                frames::begun();
                EventResult::Propagate
            }
            WindowEvent::Moved(_) => {
                self.window.set_moved(self.window.get_moved().wrapping_add(1));
                EventResult::Propagate
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.held = Held { control: state.control_key(), shift: state.shift_key(), alt: state.alt_key(), superkey: state.super_key() };
                if self.recording.is_some() {
                    self.push_recording();
                }
                EventResult::Propagate
            }
            WindowEvent::KeyboardInput { event, .. } if self.recording.is_some() => {
                if let (ElementState::Pressed, false, PhysicalKey::Code(code)) = (event.state, event.repeat, event.physical_key) {
                    if code == KeyCode::Escape {
                        self.stop_recording();
                    } else if let Some(accelerator) = keys::accelerator(self.held, &format!("{code:?}")) {
                        let id = self.recording.take().unwrap_or_default();
                        self.push_recording();
                        let asks = self.model.bind(&id, &accelerator);
                        self.ask(asks);
                    }
                }
                self.draw_soon();
                EventResult::PreventDefault
            }
            WindowEvent::KeyboardInput { .. } | WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } | WindowEvent::Ime(_) => {
                self.draw_soon();
                EventResult::Propagate
            }
            WindowEvent::Focused(false) => {
                self.held = Held::default();
                self.stop_recording();
                EventResult::Propagate
            }
            _ => EventResult::Propagate,
        }
    }

    fn hide(&mut self) {
        self.stop_recording();
        let _ = self.window.hide();
        frames::write();
    }

    /// Puts the window on screen, in the middle of the room the screen has
    /// for it and no larger, in front, and reads again what may have
    /// changed while it was away. `then` hears whether winit made the
    /// window, which it does in its own time.
    fn reveal(&mut self, asked: Instant, then: impl FnOnce(Result<(), String>) + 'static) {
        if let Err(e) = self.window.show() {
            return then(Err(e.to_string()));
        }
        let window = self.window.as_weak();
        let waiting = slint::spawn_local(async move {
            let Some(window) = window.upgrade() else { return then(Err("the settings window is gone".into())) };
            // Held no longer than this: a window still referred to stays
            // on screen when Slint hides it.
            match window.window().winit_window().await {
                Ok(made) => {
                    guard::remember(&made);
                    let monitor = made.current_monitor().or_else(|| made.primary_monitor()).or_else(|| made.available_monitors().next());
                    let whole = monitor.map(|monitor| (monitor.position().x, monitor.position().y, monitor.size().width, monitor.size().height));
                    if let Some((x, y, width, height)) = area::work(whole) {
                        let scale = made.scale_factor() as f32;
                        window.set_room_width(width as f32 / scale);
                        window.set_room_height(height as f32 / scale);
                        // As large as it will be, which it may not be yet.
                        let margin = 2.0 * window.get_margin();
                        let (wide, high) = ((860.0_f32.min(width as f32 / scale) + margin) * scale, (640.0_f32.min(height as f32 / scale) + margin) * scale);
                        made.set_outer_position(PhysicalPosition::new(x + ((width as f32 - wide) / 2.0) as i32, y + ((height as f32 - high) / 2.0) as i32));
                    }
                    made.focus_window();
                    eprintln!("heyo: settings up {} ms after they were asked for", asked.elapsed().as_millis());
                    then(Ok(()));
                }
                Err(e) => then(Err(e.to_string())),
            }
        });
        if let Err(e) = waiting {
            eprintln!("heyo: slint could not wait for its window: {e}");
        }
        self.ask(self.model.opened());
        // Where Slint cannot tell whether the desktop is dark (it asks the
        // desktop's portal), the host's own answer stands in, as of now.
        let _ = self.asking.send(Box::new(|host| {
            let dark = host.system_dark();
            let _ = slint::invoke_from_event_loop(move || {
                with(|live| live.window.set_system_dark(dark));
            });
        }));
    }
}

/// What the window does is said to `Live`.
fn wire(window: &ui::SettingsWindow) {
    let host = window.global::<ui::Host>();
    host.on_show(|id| {
        with(|live| {
            live.stop_recording();
            let asks = live.model.show(&id);
            live.ask(asks);
            live.push();
        });
    });
    host.on_search(|text| {
        with(|live| {
            live.model.nav_search = text.into();
            live.push();
        });
    });
    host.on_act(|id, value| {
        with(|live| {
            let asks = live.model.acted(&id, &value);
            live.ask(asks);
            live.push();
        });
    });
    host.on_find(|text| {
        with(|live| {
            live.model.find = text.into();
            live.push();
        });
    });
    host.on_pick(|id| {
        with(|live| {
            live.model.pick(&id);
            live.push();
        });
    });
    host.on_record(|id| {
        with(|live| {
            live.recording = Some(id.into());
            live.push_recording();
        });
    });
    host.on_stop_recording(|| {
        with(Live::stop_recording);
    });
    host.on_clear_hotkey(|id| {
        with(|live| {
            let asks = live.model.bind(&id, "");
            live.ask(asks);
        });
    });
    host.on_close(|| {
        with(Live::hide);
    });
    host.on_drag(|| {
        with(|live| {
            live.window.window().with_winit_window(|window| {
                let _ = window.drag_window();
            });
        });
    });
    window.window().on_close_requested(|| {
        with(Live::hide);
        slint::CloseRequestResponse::HideWindow
    });
    window.window().on_winit_window_event(|_, event| with(|live| live.told(event)).unwrap_or(EventResult::Propagate));
}

/// Which of Slint's renderers draws the window.
fn renderer() -> String {
    std::env::var("HEYO_SETTINGS_RENDERER").ok().filter(|name| !name.is_empty()).unwrap_or_else(|| RENDERER.to_string())
}

/// Runs Slint on this thread until Heyo ends: makes the window, shows it,
/// and says through `started` whether that worked. It only returns if it
/// did not.
pub(super) fn run(host: Arc<dyn Host>, asked: Instant, started: mpsc::Sender<Result<(), String>>) {
    let begin = || -> Result<(), String> {
        guard::test_no_start()?;
        let renderer = renderer();
        let selector = slint::BackendSelector::new().backend_name("winit".into()).renderer_name(renderer.clone());
        #[cfg(target_os = "linux")]
        let selector = selector.with_winit_window_attributes_hook(|attributes| {
            use slint::winit_030::winit::platform::wayland::WindowAttributesExtWayland;
            use slint::winit_030::winit::platform::x11::WindowAttributesExtX11;
            // As Heyo's other windows are named to the desktop: the class
            // on X11, the application's id on Wayland.
            WindowAttributesExtWayland::with_name(WindowAttributesExtX11::with_name(attributes, "Heyo", "heyo"), "heyo", "heyo")
        });
        let selector = if frames::wanted().is_some() { selector.with_winit_custom_application_handler(frames::Timing) } else { selector };
        selector.select().map_err(|e| format!("Slint has no backend: {e}"))?;
        let fonts = super::fonts::register();
        eprintln!("heyo: settings drawn by Slint with {renderer}, in {}", if fonts.is_empty() { "the system's fonts".to_string() } else { fonts.join(" and ") });

        // The host is asked on a thread of its own, in the order asked.
        let (asking, jobs) = mpsc::channel::<Job>();
        let answering = host.clone();
        std::thread::Builder::new()
            .name("heyo-slint-host".into())
            .spawn(move || {
                for job in jobs {
                    job(answering.as_ref());
                }
            })
            .map_err(|e| e.to_string())?;

        let window = ui::SettingsWindow::new().map_err(|e| format!("Slint could not make the window: {e}"))?;
        window.global::<ui::Theme>().set_plain(renderer == "software");
        let pictures = Rc::new(Pictures::default());
        let lines = Rc::new(Lines { lines: RefCell::default(), pictures: pictures.clone(), notify: ModelNotify::default() });
        window.global::<ui::Host>().set_lines(ModelRc::from(lines.clone() as Rc<dyn slint::Model<Data = ui::Line>>));
        wire(&window);

        // What the first page shows is read before the window is: it comes
        // up as it is, not empty and then filled.
        let (first, read) = mpsc::channel();
        let _ = asking.send(Box::new(move |host| {
            let _ = first.send((host.call("get_settings", Value::Null), host.call("get_hotkeys", Value::Null), host.call("get_capabilities", Value::Null), host.system_dark()));
        }));
        let (settings, hotkeys, capabilities, dark) = read.recv().map_err(|_| "the host did not answer".to_string())?;
        let mut model = Model::new();
        model.answered(Then::Settings, Ok(settings?));
        model.answered(Then::Hotkeys, Ok(hotkeys?));
        model.answered(Then::Capabilities, Ok(capabilities?));
        window.set_system_dark(dark);

        let poll = slint::Timer::default();
        poll.start(slint::TimerMode::Repeated, Duration::from_secs(2), || {
            with(|live| live.ask(live.model.poll()));
        });
        let mut live = Live { window, asking, model, lines, pictures, recording: None, held: Held::default(), _poll: poll };
        live.push();
        LIVE.with(|slot| *slot.borrow_mut() = Some(live));

        host.listen(Box::new(|event, told| {
            let event = event.to_string();
            let _ = slint::invoke_from_event_loop(move || {
                with(|live| {
                    let asks = live.model.heard(&event, &told);
                    live.ask(asks);
                    live.push();
                });
            });
        }));
        Ok(())
    };
    if let Err(reason) = begin() {
        let _ = started.send(Err(reason));
        return;
    }
    // The window is shown from inside the loop, where winit has its
    // windows; whether it came up is what `open` waits to hear.
    let showing = started.clone();
    let queued = slint::invoke_from_event_loop(move || {
        with(|live| {
            live.reveal(asked, move |shown| {
                if shown.is_err() {
                    let _ = slint::quit_event_loop();
                }
                let _ = showing.send(shown);
            })
        });
    });
    if let Err(e) = queued {
        let _ = started.send(Err(e.to_string()));
        return;
    }
    guard::test_panic();
    if let Err(e) = slint::run_event_loop_until_quit() {
        eprintln!("heyo: slint's event loop ended: {e}");
    }
    guard::gone();
}

/// Brings the window back. Called on any thread; false when Slint's loop
/// is no longer there to hear it.
pub(super) fn reopen(asked: Instant) -> bool {
    slint::invoke_from_event_loop(move || {
        with(|live| {
            live.reveal(asked, |shown| {
                if let Err(reason) = shown {
                    eprintln!("heyo: the settings could not be shown: {reason}");
                    guard::gone();
                }
            })
        });
    })
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// winit names a key's place as the web does, which is what lets the
    /// recorder's table be the one the palette's recorder has.
    #[test]
    fn winit_names_keys_as_the_web_does() {
        let named = |code: KeyCode| format!("{code:?}");
        assert_eq!(named(KeyCode::KeyA), "KeyA");
        assert_eq!(named(KeyCode::Digit7), "Digit7");
        assert_eq!(named(KeyCode::F12), "F12");
        let theirs = [
            KeyCode::Space,
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Insert,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::ArrowUp,
            KeyCode::ArrowDown,
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::Backquote,
            KeyCode::Minus,
            KeyCode::Equal,
            KeyCode::BracketLeft,
            KeyCode::BracketRight,
            KeyCode::Backslash,
            KeyCode::Semicolon,
            KeyCode::Quote,
            KeyCode::Comma,
            KeyCode::Period,
            KeyCode::Slash,
        ];
        assert_eq!(theirs.map(named).to_vec(), keys::CODE_NAMES.iter().map(|(code, _)| code.to_string()).collect::<Vec<_>>());
    }
}
