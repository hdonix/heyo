//! Slint's thread: the one all of Heyo's Slint windows are on, from the
//! first time one is wanted until Heyo ends. winit's loop cannot be made a
//! second time, so there is one, with one renderer, and a window that is
//! closed is only hidden.
//!
//! What a window asks of the host is asked on another thread (`asker`): a
//! command may wait on other programs, and runs the host's own code under
//! the host's own locks. Slint's thread therefore never holds anything of
//! the host's, which is what lets it be given up on if it panics (`guard`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{mpsc, Arc};

use super::guard;
use super::host::Host;

/// The renderer the windows are drawn with unless `HEYO_SETTINGS_RENDERER`
/// names another of Slint's: Skia, on the CPU. Except on Wayland, where a
/// window drawn on the CPU cannot be seen through (softbuffer hands the
/// compositor XRGB) and the pill would be a black box: there it is Skia as
/// Slint chooses, which is OpenGL where the session has it and the CPU
/// where it has not.
const RENDERER: &str = "skia-software";
const RENDERER_ON_WAYLAND: &str = "skia";

/// Something asked of the host, away from Slint's thread.
pub(super) type Job = Box<dyn FnOnce(&dyn Host) + Send>;

/// What the windows on this thread share.
pub(super) struct Shared {
    pub host: Arc<dyn Host>,
    pub renderer: String,
}

/// Which window is being made: winit is told things of a window before it
/// exists, through one hook for all of them, which is not told which.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Settings,
    Pill,
    Screenshot,
}

thread_local! {
    static SHARED: RefCell<Option<Rc<Shared>>> = const { RefCell::new(None) };
    static MAKING: Cell<Kind> = const { Cell::new(Kind::Settings) };
    /// Whether the windows are seen through, once a window has been drawn
    /// and it is known what draws them.
    static CLEAR: Cell<Option<bool>> = const { Cell::new(None) };
}

/// What the windows share, on Slint's thread once it has begun.
pub(super) fn shared() -> Option<Rc<Shared>> {
    SHARED.with(|shared| shared.borrow().clone())
}

/// Makes a window of that kind: winit is told what such a window is.
pub(super) fn making<T>(kind: Kind, make: impl FnOnce() -> T) -> T {
    MAKING.with(|making| making.set(kind));
    let made = make();
    MAKING.with(|making| making.set(Kind::Settings));
    made
}

/// A thread for asking the host, one thing after another in the order
/// asked. It ends when what is returned is dropped.
pub(super) fn asker(name: &str, host: Arc<dyn Host>) -> Result<mpsc::Sender<Job>, String> {
    let (asking, jobs) = mpsc::channel::<Job>();
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            for job in jobs {
                job(host.as_ref());
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(asking)
}

/// Whether what is behind a window shows where the window draws nothing,
/// which a margin for a shadow needs, and the pill its corners. Not on
/// Wayland when it is drawn on the CPU. Which it is there is known for
/// certain only once a window has been drawn (`watch`); until then it is
/// what was asked for.
pub(super) fn seen_through(renderer: &str) -> bool {
    !wayland() || CLEAR.with(Cell::get).unwrap_or(!renderer.ends_with("software"))
}

/// Finds out, when the window is first drawn, whether it is drawn through
/// OpenGL, and has `then` done with whether it is seen through if that is
/// not what was assumed. For a window made on Wayland.
pub(super) fn watch(window: &slint::Window, renderer: &str, then: impl Fn(bool) + 'static) {
    if !wayland() {
        return;
    }
    let assumed = seen_through(renderer);
    let learnt = Rc::new(move |clear: bool| {
        CLEAR.with(|known| known.set(Some(clear)));
        if clear != assumed {
            then(clear);
        }
    });
    let told = window.set_rendering_notifier({
        let learnt = learnt.clone();
        move |state, api| {
            if matches!(state, slint::RenderingState::RenderingSetup) {
                learnt(matches!(api, slint::GraphicsAPI::NativeOpenGL { .. }));
            }
        }
    });
    // A renderer that cannot say is one on the CPU.
    if told.is_err() {
        learnt(false);
    }
}

/// Whether the windows are Wayland ones, told as winit tells it.
pub(super) fn wayland() -> bool {
    cfg!(target_os = "linux") && ["WAYLAND_DISPLAY", "WAYLAND_SOCKET"].iter().any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

/// Which of Slint's renderers draws the windows.
fn renderer() -> String {
    std::env::var("HEYO_SETTINGS_RENDERER").ok().filter(|name| !name.is_empty()).unwrap_or_else(|| if wayland() { RENDERER_ON_WAYLAND } else { RENDERER }.to_string())
}

/// Runs Slint on this thread until Heyo ends, and says through `begun`
/// whether it began: Ok once its loop turns. It only returns if it did
/// not, or if the loop was ended.
pub(super) fn run(host: Arc<dyn Host>, begun: mpsc::Sender<Result<(), String>>) {
    let begin = || -> Result<(), String> {
        guard::test_no_start()?;
        let renderer = renderer();
        let selector = slint::BackendSelector::new().backend_name("winit".into()).renderer_name(renderer.clone());
        let selector = selector.with_winit_window_attributes_hook(|attributes| {
            #[cfg(target_os = "linux")]
            let attributes = {
                use slint::winit_030::winit::platform::wayland::WindowAttributesExtWayland;
                use slint::winit_030::winit::platform::x11::WindowAttributesExtX11;
                // As Heyo's other windows are named to the desktop: the class
                // on X11, the application's id on Wayland.
                WindowAttributesExtWayland::with_name(WindowAttributesExtX11::with_name(attributes, "Heyo", "heyo"), "heyo", "heyo")
            };
            match MAKING.with(Cell::get) {
                Kind::Settings => attributes,
                Kind::Pill => super::pill::native::attributes(attributes),
                Kind::Screenshot => super::screenshot::native::attributes(attributes),
            }
        });
        let selector = if super::window::frames::wanted().is_some() { selector.with_winit_custom_application_handler(super::window::frames::Timing) } else { selector };
        selector.select().map_err(|e| format!("Slint has no backend: {e}"))?;
        // A Wayland window is made anew each time it is shown, and the
        // second time Slint leaves out what the hook above gave the first:
        // the window had no application to belong to, and the Shell did
        // not know it for Heyo's. What Slint is told itself it gives every
        // time. (Not on X11, where it would take the instance's name away
        // and the window is named either way.)
        if wayland() {
            slint::set_xdg_app_id("heyo").map_err(|e| format!("Slint took no application id: {e}"))?;
        }
        let fonts = super::fonts::register();
        eprintln!("heyo: Slint draws with {renderer}, in {}", if fonts.is_empty() { "the system's fonts".to_string() } else { fonts.join(" and ") });
        SHARED.with(|shared| *shared.borrow_mut() = Some(Rc::new(Shared { host, renderer })));
        Ok(())
    };
    if let Err(reason) = begin() {
        let _ = begun.send(Err(reason));
        return;
    }
    // Said from inside the loop: it has begun when it turns.
    let turning = begun.clone();
    if let Err(e) = slint::invoke_from_event_loop(move || {
        let _ = turning.send(Ok(()));
    }) {
        let _ = begun.send(Err(e.to_string()));
        return;
    }
    guard::test_panic();
    if let Err(e) = slint::run_event_loop_until_quit() {
        eprintln!("heyo: slint's event loop ended: {e}");
    }
    guard::gone();
}

/// Has `work` done on Slint's thread, from any other. False when its loop
/// is no longer there to hear it.
pub(super) fn on(work: impl FnOnce() + Send + 'static) -> bool {
    slint::invoke_from_event_loop(work).is_ok()
}
