//! A window as a layer surface: what Wayland has for a window that floats
//! over the others, on every workspace, where it says, without the
//! keyboard (the protocol wlr-layer-shell, which Sway, Hyprland and the
//! other wlroots compositors, KWin and COSMIC offer, and GNOME does not).
//!
//! winit makes no such surface, and Slint draws only into winit's windows,
//! so this one is Heyo's own: a connection of its own to the compositor,
//! a surface on it, Slint's own renderer drawing the same markup into a
//! buffer that is handed to the compositor, and the pointer's events
//! handed to Slint. To Slint it is a window like another (a
//! `WindowAdapter`); the rest of Heyo's Slint code does not know it from
//! winit's. Slint's platform is winit's backend, wrapped ([`Platform`]):
//! it hands out winit's windows, except to a window whose standing is
//! carried out here (`standing::Told::layered`).
//!
//! What is here: one kind of window, the dictation pill's. A surface on
//! the overlay layer of an output, anchored to its bottom centre with the
//! margin the place module's rule gives, never with the keyboard, taking
//! the pointer where it is told to (`reach`), drawn at the output's scale.
//! Not here: the keyboard, a text being composed, more than one output.

use std::cell::{Cell, RefCell};
use std::os::fd::{AsFd, AsRawFd};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use slint::winit_030::winit::raw_window_handle;

use slint::platform::software_renderer::{PremultipliedRgbaColor, RepaintBufferType, SoftwareRenderer};
use slint::platform::{PointerEventButton, WindowAdapter, WindowEvent};
use slint::{LogicalPosition, LogicalSize, PhysicalSize, PlatformError};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use slint::winit_030::winit::window::CursorIcon;
use smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{Shape, WpCursorShapeDeviceV1};
use smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, EventQueue, Proxy, QueueHandle, WEnum};

use super::standing::{Button, Keyed, Pointed};
use crate::platform::desktop::OwnPlace;
use crate::platform::place::{Monitor, Rect};

/// Whether the compositor offers the layer shell. Asked once, on a
/// connection that is then let go.
pub(super) fn offered() -> bool {
    static OFFERED: OnceLock<bool> = OnceLock::new();
    *OFFERED.get_or_init(|| {
        let Ok(connection) = Connection::connect_to_env() else { return false };
        let Ok((globals, _queue)) = registry_queue_init::<Probe>(&connection) else { return false };
        globals.contents().with_list(|all| all.iter().any(|global| global.interface == "zwlr_layer_shell_v1"))
    })
}

/// Hears nothing: the list of what the compositor offers is all that is
/// wanted of it.
struct Probe;

impl wayland_client::Dispatch<wayland_client::protocol::wl_registry::WlRegistry, wayland_client::globals::GlobalListContents> for Probe {
    fn event(_: &mut Self, _: &wayland_client::protocol::wl_registry::WlRegistry, _: wayland_client::protocol::wl_registry::Event, _: &wayland_client::globals::GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

// --- Slint's platform, wrapped --------------------------------------------------------

/// winit's backend, which is Slint's platform for every window but those
/// made here.
pub(super) struct Platform {
    winit: i_slint_backend_winit::Backend,
    /// Whether the window being made is to be a layer surface.
    layered: Box<dyn Fn() -> Option<Drawn>>,
}

/// What draws a layer surface.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Drawn {
    /// Slint's own renderer on the CPU, into a buffer that can be seen
    /// through: a small window with clear corners (the pill, a pinned
    /// capture).
    Clear,
    /// Skia on the CPU, as winit's windows are drawn on X11, handed to the
    /// compositor without the bits that say how much is seen through: a
    /// window that hides all behind it and is as large as a screen (the
    /// overlay), which the other is too slow for.
    Solid,
}

impl Platform {
    pub(super) fn over(winit: i_slint_backend_winit::Backend, layered: impl Fn() -> Option<Drawn> + 'static) -> Self {
        Self { winit, layered: Box::new(layered) }
    }
}

impl slint::platform::Platform for Platform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        match (self.layered)() {
            Some(drawn) => Ok(Layered::new(drawn)),
            None => self.winit.create_window_adapter(),
        }
    }

    fn run_event_loop(&self) -> Result<(), PlatformError> {
        self.winit.run_event_loop()
    }

    fn process_events(&self, timeout: Option<core::time::Duration>, token: i_slint_core::InternalToken) -> Result<core::ops::ControlFlow<()>, PlatformError> {
        self.winit.process_events(timeout, token)
    }

    fn bind_context(&self, context: i_slint_core::SlintContextWeak, token: i_slint_core::InternalToken) {
        self.winit.bind_context(context, token)
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn slint::platform::EventLoopProxy>> {
        self.winit.new_event_loop_proxy()
    }

    fn duration_since_start(&self) -> core::time::Duration {
        self.winit.duration_since_start()
    }

    fn click_interval(&self) -> core::time::Duration {
        self.winit.click_interval()
    }

    fn cursor_flash_cycle(&self) -> core::time::Duration {
        self.winit.cursor_flash_cycle()
    }

    fn set_clipboard_text(&self, text: &str, clipboard: slint::platform::Clipboard) {
        self.winit.set_clipboard_text(text, clipboard)
    }

    fn clipboard_text(&self, clipboard: slint::platform::Clipboard) -> Option<String> {
        self.winit.clipboard_text(clipboard)
    }

    fn debug_log(&self, arguments: core::fmt::Arguments) {
        self.winit.debug_log(arguments)
    }

    fn open_url(&self, url: &str) -> Result<(), PlatformError> {
        self.winit.open_url(url)
    }
}

// --- One window ----------------------------------------------------------------------

/// A window of Slint's that is a layer surface.
pub(super) struct Layered {
    window: slint::Window,
    renderer: Renderer,
    /// Where it goes, as its window said (`place`).
    place: Cell<Option<OwnPlace>>,
    /// Where in it the pointer is its own, in its device pixels (`reach`).
    reach: Cell<Option<Rect>>,
    surface: RefCell<Option<LayerSurface>>,
    /// The compositor has said how large it is: it may be drawn.
    configured: Cell<bool>,
    /// Its size in logical pixels, and how many device pixels one is.
    size: Cell<(u32, u32)>,
    scale: Cell<i32>,
    /// Where its top left corner is on its output, in logical pixels, for
    /// one that is put at a place and can be moved.
    at: Cell<(i32, i32)>,
    /// Who hears what the pointer does in it, before Slint: and says
    /// whether Slint is to have it too.
    hears: RefCell<Option<Rc<dyn Fn(Pointed) -> bool>>>,
    /// The same for the keys, for one that has the keyboard.
    keyed: RefCell<Option<Rc<dyn Fn(Keyed) -> bool>>>,
    /// What the pointer looks like over it.
    cursor: Cell<Shape>,
    /// Slint said that something in it takes text now, with its cursor
    /// there, or that nothing does any more: to be said to the
    /// compositor at the next turn.
    typing: Cell<Option<Option<composing::Cursor>>>,
    /// Something in it changed since it was drawn.
    dirty: Cell<bool>,
    /// A frame was handed over and the compositor has not shown it yet.
    waiting: Cell<bool>,
}

thread_local! {
    /// The layer surfaces there are, on Slint's thread.
    static WINDOWS: RefCell<Vec<Weak<Layered>>> = const { RefCell::new(Vec::new()) };
    static WAYLAND: RefCell<Option<Wayland>> = const { RefCell::new(None) };
    /// A turn has been asked for and not taken yet.
    static ASKED: Cell<bool> = const { Cell::new(false) };
    /// The key that is down and pressed again and again, the window it is
    /// pressed in, and what presses it.
    static REPEAT: RefCell<Option<(u32, Weak<Layered>, slint::Timer)>> = const { RefCell::new(None) };
}

/// The surface that has every key, and the connection it is on, as they
/// are known outside Slint's thread (`take_down`).
static HELD: Mutex<Option<(Connection, wl_surface::WlSurface)>> = Mutex::new(None);
/// It was taken off the screen from outside: nothing is drawn into it.
static DOWN: AtomicBool = AtomicBool::new(false);
/// The socket of the connection, for `lost`. -1 while there is none.
static SOCKET: AtomicI32 = AtomicI32::new(-1);

/// Takes the surface that has every key off the screen, from any thread,
/// when Slint's does not answer: the compositor gives the keyboard back
/// to what had it when the surface is no longer shown. A surface is not
/// its thread's, so this says it on the connection Slint's thread has;
/// that thread, if it ever goes on, draws nothing into it and takes it
/// away as it would have. False when no surface has the keys.
pub(super) fn take_down() -> bool {
    let Ok(held) = HELD.lock() else { return false };
    let Some((connection, surface)) = held.as_ref() else { return false };
    DOWN.store(true, Ordering::SeqCst);
    // A surface without a picture is not shown.
    surface.attach(None, 0, 0);
    surface.commit();
    if connection.flush().is_err() {
        eprintln!("heyo: the surface that holds the keyboard could not be taken off the screen from outside");
    }
    true
}

/// Slint's thread is lost for good (it panicked): the compositor is to
/// drop all that was on its connection, as an X server is made to
/// (`guard`). Not closed: the number must not come to mean another file
/// while the connection has it written down.
pub(super) fn lost() {
    let socket = SOCKET.load(Ordering::SeqCst);
    if socket >= 0 {
        // SAFETY: a socket of this process's, which stays open.
        unsafe { libc::shutdown(socket, libc::SHUT_RDWR) };
    }
}

impl Layered {
    fn new(drawn: Drawn) -> Rc<Self> {
        let made = Rc::new_cyclic(|own: &Weak<Self>| Self {
            window: slint::Window::new(own.clone() as Weak<dyn WindowAdapter>),
            renderer: match drawn {
                Drawn::Clear => Renderer::Clear(SoftwareRenderer::new_with_repaint_buffer_type(RepaintBufferType::NewBuffer)),
                Drawn::Solid => Renderer::Solid(i_slint_renderer_skia::SkiaRenderer::default_software(&i_slint_renderer_skia::SkiaSharedContext::default()), Cell::new(None)),
            },
            place: Cell::new(None),
            reach: Cell::new(None),
            surface: RefCell::new(None),
            configured: Cell::new(false),
            size: Cell::new((1, 1)),
            scale: Cell::new(1),
            at: Cell::new((0, 0)),
            hears: RefCell::new(None),
            keyed: RefCell::new(None),
            cursor: Cell::new(Shape::Default),
            typing: Cell::new(None),
            dirty: Cell::new(false),
            waiting: Cell::new(false),
        });
        WINDOWS.with_borrow_mut(|windows| {
            windows.retain(|window| window.strong_count() > 0);
            windows.push(Rc::downgrade(&made));
        });
        made
    }

    fn is(&self, surface: &wl_surface::WlSurface) -> bool {
        self.surface.borrow().as_ref().is_some_and(|own| own.wl_surface().id() == surface.id())
    }

    /// Tells Slint how large the window is and at what scale it is drawn.
    fn measured(&self) {
        let (size, scale) = (self.size.get(), self.scale.get());
        let _ = self.window.dispatch_event_with_result(WindowEvent::ScaleFactorChanged { scale_factor: scale as f32 });
        let _ = self.window.dispatch_event_with_result(WindowEvent::Resized { size: LogicalSize::new(size.0 as f32, size.1 as f32) });
        self.dirty.set(true);
    }

    /// Makes the surface, where the window said it goes. It is drawn once
    /// the compositor has answered.
    fn put_on(&self) -> Result<(), String> {
        if self.surface.borrow().is_some() {
            return Ok(());
        }
        with(|wayland| {
            // The first output: Wayland knows no primary one, and a place
            // of the screen is taken to be on it (one output, for now).
            let output = wayland.state.outputs.outputs().next();
            let info = output.as_ref().and_then(|output| wayland.state.outputs.info(output));
            let scale = info.as_ref().map_or(1, |info| info.scale_factor.max(1));
            let queue = wayland.queue.handle();
            let surface = wayland.state.compositor.create_surface(&queue);
            let (layer, size) = match self.place.get() {
                Some(OwnPlace::BottomCentre { size }) => {
                    let size = (size.0.round() as u32, size.1.round() as u32);
                    // The same rule as everywhere (`place`): what it gives
                    // as a place is said here as a margin from the bottom.
                    let lifted = info.as_ref().and_then(|info| info.logical_size).map_or(0, |(width, height)| {
                        let whole = Monitor::new((0, 0, width.max(0) as u32, height.max(0) as u32), 1.0, true, None);
                        let (_, y) = whole.bottom_centre(size);
                        height - y - size.1 as i32
                    });
                    // Over everything, a window that fills the screen too.
                    let layer = wayland.state.layers.create_layer_surface(&queue, surface, Layer::Overlay, Some("heyo"), output.as_ref());
                    layer.set_anchor(Anchor::BOTTOM);
                    layer.set_margin(0, 0, lifted.max(0), 0);
                    (layer, size)
                }
                Some(OwnPlace::At { at, size }) => {
                    // A place and a size in the screen's pixels, which are
                    // the output's logical ones times its scale.
                    let size = ((size.0 as i32 + scale - 1) / scale, (size.1 as i32 + scale - 1) / scale);
                    self.at.set((at.0 / scale, at.1 / scale));
                    // Above the windows, as a window kept above them is.
                    let layer = wayland.state.layers.create_layer_surface(&queue, surface, Layer::Top, Some("heyo"), output.as_ref());
                    layer.set_anchor(Anchor::TOP | Anchor::LEFT);
                    layer.set_margin(self.at.get().1, 0, 0, self.at.get().0);
                    (layer, (size.0.max(1) as u32, size.1.max(1) as u32))
                }
                Some(OwnPlace::Cover { area }) => {
                    // All of the output that was read, over everything on
                    // it, with every key while it is there.
                    let scale_of = |output: &wl_output::WlOutput| wayland.state.outputs.info(output).map_or(1, |info| info.scale_factor.max(1));
                    let starts_there = |output: &wl_output::WlOutput| wayland.state.outputs.info(output).and_then(|info| info.logical_position).is_some_and(|(x, y)| (x * scale_of(output), y * scale_of(output)) == (area.x, area.y));
                    let output = wayland.state.outputs.outputs().find(starts_there).or(output);
                    let scale = output.as_ref().map_or(1, scale_of);
                    let layer = wayland.state.layers.create_layer_surface(&queue, surface, Layer::Overlay, Some("heyo"), output.as_ref());
                    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
                    layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
                    layer.set_exclusive_zone(-1);
                    layer.wl_surface().set_buffer_scale(scale);
                    layer.commit();
                    self.size.set(((area.width as i32 / scale).max(1) as u32, (area.height as i32 / scale).max(1) as u32));
                    self.scale.set(scale);
                    DOWN.store(false, Ordering::SeqCst);
                    if let Ok(mut held) = HELD.lock() {
                        *held = Some((wayland.connection.clone(), layer.wl_surface().clone()));
                    }
                    *self.surface.borrow_mut() = Some(layer);
                    let _ = wayland.connection.flush();
                    return Ok(());
                }
                _ => return Err("it was not said where it goes".to_string()),
            };
            layer.set_size(size.0, size.1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            // It is over the others without taking room from them.
            layer.set_exclusive_zone(-1);
            layer.wl_surface().set_buffer_scale(scale);
            layer.commit();
            self.size.set(size);
            self.scale.set(scale);
            *self.surface.borrow_mut() = Some(layer);
            let _ = wayland.connection.flush();
            Ok(())
        })
        .unwrap_or_else(|| Err("no connection to the compositor".to_string()))?;
        self.measured();
        Ok(())
    }

    fn take_off(&self) {
        self.configured.set(false);
        self.waiting.set(false);
        self.typing.set(Some(None));
        if let Renderer::Solid(skia, given) = &self.renderer {
            // What it drew into goes with the surface.
            let _ = skia.suspend();
            given.set(None);
        }
        if let Ok(mut held) = HELD.lock() {
            if held.as_ref().is_some_and(|(_, surface)| self.is(surface)) {
                *held = None;
            }
        }
        if self.surface.borrow_mut().take().is_some() {
            with(|wayland| {
                let _ = wayland.connection.flush();
            });
        }
    }

    /// Says to the compositor where the pointer is the window's.
    fn reaches(&self, wayland: &Wayland) {
        let surface = self.surface.borrow();
        let Some(layer) = surface.as_ref() else { return };
        let scale = self.scale.get().max(1);
        match (self.reach.get(), Region::new(&wayland.state.compositor)) {
            (Some((x, y, width, height)), Ok(region)) => {
                region.add(x / scale, y / scale, (width as i32 + scale - 1) / scale, (height as i32 + scale - 1) / scale);
                layer.wl_surface().set_input_region(Some(region.wl_region()));
            }
            // All of it.
            _ => layer.wl_surface().set_input_region(None),
        }
    }

    /// Has the pointer look as the window wants it, if it is over it.
    fn shape(&self) {
        with(|wayland| {
            if let (Some(device), Some((surface, serial))) = (wayland.state.shape.as_ref(), wayland.state.inside.as_ref()) {
                if self.is(surface) {
                    device.set_shape(*serial, self.cursor.get());
                }
            }
        });
    }

    /// A key went down or up in it, or is pressed again because it stays
    /// down: whoever hears of the keys first, then Slint if it is to.
    fn key(&self, press: &keys::Press, down: bool, again: bool) {
        if self.surface.borrow().is_none() {
            return;
        }
        let hears = self.keyed.borrow().clone();
        if hears.is_some_and(|hears| !hears(Keyed::Key { key: press.key, down, again })) {
            return;
        }
        let Some(text) = press.text.clone() else { return };
        let event = match (down, again) {
            (true, false) => WindowEvent::KeyPressed { text },
            (true, true) => WindowEvent::KeyPressRepeated { text },
            (false, _) => WindowEvent::KeyReleased { text },
        };
        let _ = self.window.dispatch_event_with_result(event);
    }

    /// Draws the window and hands the picture to the compositor.
    fn draw(&self, wayland: &mut Wayland) {
        let surface = self.surface.borrow();
        let Some(layer) = surface.as_ref() else { return };
        let (size, scale) = (self.size.get(), self.scale.get().max(1) as u32);
        let (width, height) = ((size.0 * scale) as usize, (size.1 * scale) as usize);
        let software = match &self.renderer {
            Renderer::Clear(software) => software,
            Renderer::Solid(skia, given) => {
                if DOWN.load(Ordering::SeqCst) {
                    return;
                }
                // Skia is given the surface once it has its size, and
                // hands each picture to the compositor itself.
                let surface = layer.wl_surface();
                if given.get() != Some((width, height)) {
                    let handles = std::sync::Arc::new(Handles { surface: surface.id().as_ptr().cast(), display: wayland.connection.backend().display_ptr().cast() });
                    if let Err(e) = skia.set_window_handle(handles.clone(), handles, PhysicalSize::new(width as u32, height as u32), None, false) {
                        return eprintln!("heyo: Skia could not draw into the layer surface: {e}");
                    }
                    given.set(Some((width, height)));
                }
                self.reaches(wayland);
                surface.frame(&wayland.queue.handle(), surface.clone());
                match skia.render() {
                    Ok(_) => {
                        self.waiting.set(true);
                        super::screenshot::trace("layer surface drawn");
                    }
                    Err(e) => eprintln!("heyo: the layer surface could not be drawn: {e}"),
                }
                self.dirty.set(false);
                return;
            }
        };
        let mut drawn = vec![PremultipliedRgbaColor::default(); width * height];
        software.render(&mut drawn, width);
        let Ok((buffer, canvas)) = wayland.state.pool.create_buffer(width as i32, height as i32, width as i32 * 4, wl_shm::Format::Argb8888) else { return };
        for (pixel, out) in drawn.iter().zip(canvas.chunks_exact_mut(4)) {
            out.copy_from_slice(&[pixel.blue, pixel.green, pixel.red, pixel.alpha]);
        }
        let queue = wayland.queue.handle();
        let surface = layer.wl_surface();
        self.reaches(wayland);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.frame(&queue, surface.clone());
        if buffer.attach_to(surface).is_ok() {
            self.waiting.set(true);
        }
        layer.commit();
        self.dirty.set(false);
        super::screenshot::trace("layer surface drawn");
    }
}

impl WindowAdapter for Layered {
    fn window(&self) -> &slint::Window {
        &self.window
    }

    fn size(&self) -> PhysicalSize {
        let (size, scale) = (self.size.get(), self.scale.get().max(1) as u32);
        PhysicalSize::new(size.0 * scale, size.1 * scale)
    }

    fn renderer(&self) -> &dyn slint::platform::Renderer {
        match &self.renderer {
            Renderer::Clear(software) => software,
            Renderer::Solid(skia, _) => skia,
        }
    }

    fn set_visible(&self, visible: bool) -> Result<(), PlatformError> {
        if visible {
            self.put_on().map_err(|reason| PlatformError::Other(format!("no layer surface: {reason}")))?;
        } else {
            self.take_off();
        }
        soon();
        Ok(())
    }

    fn request_redraw(&self) {
        self.dirty.set(true);
        soon();
    }

    fn internal(&self, _: i_slint_core::InternalToken) -> Option<&dyn i_slint_core::window::WindowAdapterInternal> {
        Some(self)
    }
}

/// What Slint asks of a window besides what `WindowAdapter` has: here,
/// that the input method be told when a line is being typed, and where.
impl i_slint_core::window::WindowAdapterInternal for Layered {
    fn input_method_request(&self, request: i_slint_core::window::InputMethodRequest) {
        use i_slint_core::window::InputMethodRequest;
        self.typing.set(Some(match request {
            InputMethodRequest::Enable(typed) | InputMethodRequest::Update(typed) => Some((typed.cursor_rect_origin.x.round() as i32, typed.cursor_rect_origin.y.round() as i32, typed.cursor_rect_size.width.round().max(1.0) as i32, typed.cursor_rect_size.height.round().max(1.0) as i32)),
            _ => None,
        }));
        soon();
    }
}

mod composing;
mod keys;

/// What draws a window, and for Skia the size of the surface it was given.
enum Renderer {
    Clear(SoftwareRenderer),
    Solid(i_slint_renderer_skia::SkiaRenderer, Cell<Option<(usize, usize)>>),
}

/// A surface of ours as Skia's way of drawing on the CPU wants to be
/// handed one: by the compositor's own names for it and for the
/// connection.
struct Handles {
    surface: *mut std::ffi::c_void,
    display: *mut std::ffi::c_void,
}

// SAFETY: two names, which are only ever used on Slint's thread; the type
// they are handed over as asks for this.
unsafe impl Send for Handles {}
unsafe impl Sync for Handles {}

impl raw_window_handle::HasWindowHandle for Handles {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let surface = std::ptr::NonNull::new(self.surface).ok_or(raw_window_handle::HandleError::Unavailable)?;
        // SAFETY: the surface outlives what draws into it (`take_off` lets go of that first).
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::Wayland(raw_window_handle::WaylandWindowHandle::new(surface))) })
    }
}

impl raw_window_handle::HasDisplayHandle for Handles {
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        let display = std::ptr::NonNull::new(self.display).ok_or(raw_window_handle::HandleError::Unavailable)?;
        // SAFETY: the connection lives as long as Slint's thread.
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(raw_window_handle::RawDisplayHandle::Wayland(raw_window_handle::WaylandDisplayHandle::new(display))) })
    }
}

// --- The connection ------------------------------------------------------------------

/// What the compositor said, to be done with once it has been heard out:
/// what Slint does about it may come back here.
enum Said {
    Sized(wl_surface::WlSurface, (u32, u32)),
    Scaled(wl_surface::WlSurface, i32),
    Shown(wl_surface::WlSurface),
    Closed(wl_surface::WlSurface),
    Pointer(wl_surface::WlSurface, PointerEventKind, (f64, f64)),
    /// The surface has the keyboard, or has it no longer.
    Keyboard(wl_surface::WlSurface, bool),
    Key(wl_surface::WlSurface, keys::Press, bool),
    /// Whether Control and Alt are held.
    Held(wl_surface::WlSurface, bool, bool),
    /// The input method: the text that was chosen, and what is being
    /// composed now, with the part of it that is marked (in bytes).
    Composed(wl_surface::WlSurface, Option<String>, Option<(String, i32, i32)>),
}

struct State {
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    compositor: CompositorState,
    layers: LayerShell,
    shm: Shm,
    pool: SlotPool,
    pointer: Option<wl_pointer::WlPointer>,
    /// What says to the compositor how the pointer is to look, where it
    /// takes that by name; and the surface the pointer is over, with the
    /// number it came in under, which has to be said back.
    shapes: Option<CursorShapeManager>,
    shape: Option<WpCursorShapeDeviceV1>,
    inside: Option<(wl_surface::WlSurface, u32)>,
    composing: composing::Composing,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    keys: keys::Keyboard,
    /// The surface that has the keyboard.
    keyed: Option<wl_surface::WlSurface>,
    said: Vec<Said>,
}

struct Wayland {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
}

/// Connects, the first time a layer surface is wanted, and has the loop
/// woken whenever the compositor says something.
fn connect() -> Result<Wayland, String> {
    let connection = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, queue) = registry_queue_init::<State>(&connection).map_err(|e| e.to_string())?;
    let handle = queue.handle();
    let compositor = CompositorState::bind(&globals, &handle).map_err(|e| e.to_string())?;
    let layers = LayerShell::bind(&globals, &handle).map_err(|e| e.to_string())?;
    let shm = Shm::bind(&globals, &handle).map_err(|e| e.to_string())?;
    let pool = SlotPool::new(360 * 44 * 4 * 4, &shm).map_err(|e| e.to_string())?;
    let state = State { registry: RegistryState::new(&globals), outputs: OutputState::new(&globals, &handle), seats: SeatState::new(&globals, &handle), compositor, layers, shm, pool, pointer: None, shapes: CursorShapeManager::bind(&globals, &handle).ok(), shape: None, inside: None, composing: composing::Composing::default(), keyboard: None, keys: keys::Keyboard::new(), keyed: None, said: Vec::new() };
    let mut wayland = Wayland { connection, queue, state };
    // The outputs and the seat are known after this.
    for _ in 0..2 {
        wayland.queue.roundtrip(&mut wayland.state).map_err(|e| e.to_string())?;
    }
    wayland.state.composing = composing::Composing::new(&globals, &handle, wayland.state.seats.seats().next().as_ref());
    SOCKET.store(wayland.connection.as_fd().as_raw_fd(), Ordering::SeqCst);
    watch(wayland.connection.as_fd().as_raw_fd());
    Ok(wayland)
}

/// A thread that waits for the compositor to say something and has it
/// heard on Slint's thread, one hearing at a time.
fn watch(socket: i32) {
    let _ = std::thread::Builder::new().name("heyo-slint-layer".into()).spawn(move || loop {
        let mut waited = libc::pollfd { fd: socket, events: libc::POLLIN, revents: 0 };
        // SAFETY: one descriptor, which the connection keeps open for as long as Slint's thread lives.
        let ready = unsafe { libc::poll(&mut waited, 1, -1) };
        if ready < 0 || waited.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            break;
        }
        let (heard, done) = std::sync::mpsc::channel();
        if slint::invoke_from_event_loop(move || {
            turn();
            let _ = heard.send(());
        })
        .is_err()
            || done.recv().is_err()
        {
            break;
        }
    });
}

/// Does `work` with the connection, made first if this is the first time.
/// None when there is none to be had, or it is in the middle of something.
fn with<R>(work: impl FnOnce(&mut Wayland) -> R) -> Option<R> {
    WAYLAND.with(|slot| {
        let mut slot = slot.try_borrow_mut().ok()?;
        if slot.is_none() {
            match connect() {
                Ok(wayland) => *slot = Some(wayland),
                Err(reason) => eprintln!("heyo: no layer surfaces: {reason}"),
            }
        }
        slot.as_mut().map(work)
    })
}

/// Has a turn taken at the next turn of Slint's loop.
fn soon() {
    if !ASKED.replace(true) {
        slint::Timer::single_shot(std::time::Duration::ZERO, turn);
    }
}

/// Hears what the compositor said, tells Slint, and draws what changed.
fn turn() {
    ASKED.set(false);
    let said = with(|wayland| {
        let _ = wayland.connection.flush();
        if let Some(reading) = wayland.connection.prepare_read() {
            let _ = reading.read();
        }
        let _ = wayland.queue.dispatch_pending(&mut wayland.state);
        std::mem::take(&mut wayland.state.said)
    })
    .unwrap_or_default();
    let windows: Vec<Rc<Layered>> = WINDOWS.with_borrow(|windows| windows.iter().filter_map(Weak::upgrade).collect());
    let of = |surface: &wl_surface::WlSurface| windows.iter().find(|window| window.is(surface));
    // A key is not pressed again in a window that has left the screen
    // (of which the compositor says nothing).
    REPEAT.with_borrow_mut(|repeat| {
        if repeat.as_ref().is_some_and(|(_, window, _)| window.upgrade().is_none_or(|window| window.surface.borrow().is_none())) {
            *repeat = None;
        }
    });
    for said in said {
        match said {
            Said::Sized(surface, size) => {
                if let Some(window) = of(&surface) {
                    if size.0 > 0 && size.1 > 0 {
                        window.size.set(size);
                    }
                    window.configured.set(true);
                    window.measured();
                }
            }
            Said::Scaled(surface, scale) => {
                if let Some(window) = of(&surface).filter(|window| window.scale.get() != scale) {
                    window.scale.set(scale.max(1));
                    surface.set_buffer_scale(scale.max(1));
                    window.measured();
                }
            }
            Said::Shown(surface) => {
                if let Some(window) = of(&surface) {
                    window.waiting.set(false);
                    super::screenshot::trace("layer surface shown");
                    // What moves is drawn again as often as the compositor shows it.
                    if window.window.has_active_animations() {
                        window.dirty.set(true);
                    }
                }
            }
            Said::Closed(surface) => {
                if let Some(window) = of(&surface) {
                    window.take_off();
                }
            }
            Said::Keyboard(surface, has) => {
                if !has {
                    REPEAT.set(None);
                }
                if let Some(window) = of(&surface) {
                    // What types in it takes keys only in a window that has them.
                    let _ = window.window.dispatch_event_with_result(WindowEvent::WindowActiveChanged(has));
                }
            }
            Said::Held(surface, ctrl, alt) => {
                let hears = of(&surface).and_then(|window| window.keyed.borrow().clone());
                if let Some(hears) = hears {
                    hears(Keyed::Held { ctrl, alt });
                }
            }
            Said::Key(surface, press, down) => {
                // Another key down, or this one up, ends the pressing again.
                REPEAT.with_borrow_mut(|repeat| {
                    if down || repeat.as_ref().is_some_and(|(code, ..)| *code == press.code) {
                        *repeat = None;
                    }
                });
                let Some(window) = of(&surface) else { continue };
                window.key(&press, down, false);
                if let (true, Some((rate, delay))) = (down, press.repeats) {
                    let (began, wait, timer) = (Instant::now(), Duration::from_millis(delay.into()), slint::Timer::default());
                    let (pressed, again) = (Rc::downgrade(window), press.clone());
                    timer.start(slint::TimerMode::Repeated, Duration::from_millis(u64::from(1000 / rate.max(1)).max(1)), move || {
                        if began.elapsed() >= wait {
                            if let Some(window) = pressed.upgrade() {
                                window.key(&again, true, true);
                            }
                        }
                    });
                    REPEAT.set(Some((press.code, Rc::downgrade(window), timer)));
                }
            }
            Said::Composed(surface, chosen, composed) => {
                use i_slint_core::input::{InternalKeyEvent, KeyEvent, KeyEventType};
                use i_slint_core::platform::InternalEvent;
                let Some(window) = of(&surface) else { continue };
                // As Slint's own backend hands them over: what was chosen
                // first, then what is being composed after it.
                if let Some(chosen) = chosen {
                    let mut key_event = KeyEvent::default();
                    key_event.text = chosen.as_str().into();
                    let _ = window.window.dispatch_event_with_result(WindowEvent::internal(InternalEvent::Key(InternalKeyEvent { event_type: KeyEventType::CommitComposition, key_event, ..Default::default() })));
                }
                let (text, marked) = composed.map_or((String::new(), None), |(text, from, to)| (text, (from >= 0 && to >= 0).then_some(from..to)));
                let _ = window.window.dispatch_event_with_result(WindowEvent::internal(InternalEvent::Key(InternalKeyEvent { event_type: KeyEventType::UpdateComposition, preedit_text: text.as_str().into(), preedit_selection: marked, ..Default::default() })));
            }
            Said::Pointer(surface, kind, at) => {
                let Some(window) = of(&surface) else { continue };
                match kind {
                    PointerEventKind::Enter { serial } => {
                        with(|wayland| wayland.state.inside = Some((surface.clone(), serial)));
                        window.shape();
                    }
                    PointerEventKind::Leave { .. } => {
                        with(|wayland| wayland.state.inside = None);
                    }
                    _ => {}
                }
                // A window that hears of the pointer itself, in its device pixels.
                let hears = window.hears.borrow().clone();
                if let Some(hears) = hears {
                    let scale = f64::from(window.scale.get().max(1));
                    let pointed = match kind {
                        PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => Pointed::Moved((at.0 * scale, at.1 * scale)),
                        PointerEventKind::Press { button: 0x110, .. } => Pointed::Pressed(Button::Left),
                        PointerEventKind::Release { button: 0x110, .. } => Pointed::Released(Button::Left),
                        // The right one (BTN_RIGHT).
                        PointerEventKind::Press { button: 0x111, .. } => Pointed::Pressed(Button::Right),
                        PointerEventKind::Release { button: 0x111, .. } => Pointed::Released(Button::Right),
                        PointerEventKind::Leave { .. } => Pointed::Left,
                        _ => continue,
                    };
                    if !hears(pointed) {
                        continue;
                    }
                }
                let position = LogicalPosition::new(at.0 as f32, at.1 as f32);
                let event = match kind {
                    PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => WindowEvent::PointerMoved { position },
                    PointerEventKind::Leave { .. } => WindowEvent::PointerExited,
                    // The left button (BTN_LEFT).
                    PointerEventKind::Press { button: 0x110, .. } => WindowEvent::PointerPressed { position, button: PointerEventButton::Left },
                    PointerEventKind::Release { button: 0x110, .. } => WindowEvent::PointerReleased { position, button: PointerEventButton::Left },
                    _ => continue,
                };
                let _ = window.window.dispatch_event_with_result(event);
            }
        }
    }
    for window in &windows {
        if let Some(typing) = window.typing.take() {
            with(|wayland| wayland.state.composing.want(typing));
        }
        if window.dirty.get() && window.configured.get() && !window.waiting.get() {
            with(|wayland| window.draw(wayland));
        }
    }
    with(|wayland| {
        let _ = wayland.connection.flush();
    });
}

// --- What a window says ---------------------------------------------------------------

fn of(window: &slint::Window) -> Option<Rc<Layered>> {
    WINDOWS.with_borrow(|windows| windows.iter().filter_map(Weak::upgrade).find(|layered| std::ptr::eq(&layered.window, window)))
}

/// Whether the window is a layer surface.
pub(super) fn is(window: &slint::Window) -> bool {
    of(window).is_some()
}

/// Has `hear` told what the pointer does in the window, before Slint; it
/// says whether Slint is to have it too.
pub(super) fn hear(window: &slint::Window, hear: impl Fn(Pointed) -> bool + 'static) {
    if let Some(layered) = of(window) {
        *layered.hears.borrow_mut() = Some(Rc::new(hear));
    }
}

/// How the pointer is to look over the window, where the compositor
/// takes that by name (the protocol cursor-shape); elsewhere it stays as
/// it was when it came over the window.
pub(super) fn cursor(window: &slint::Window, icon: CursorIcon) {
    let Some(layered) = of(window) else { return };
    layered.cursor.set(match icon {
        CursorIcon::Crosshair => Shape::Crosshair,
        CursorIcon::Move => Shape::Move,
        CursorIcon::NsResize => Shape::NsResize,
        CursorIcon::EwResize => Shape::EwResize,
        CursorIcon::NwseResize => Shape::NwseResize,
        CursorIcon::NeswResize => Shape::NeswResize,
        CursorIcon::Grab => Shape::Grab,
        CursorIcon::Grabbing => Shape::Grabbing,
        CursorIcon::Pointer => Shape::Pointer,
        CursorIcon::Text => Shape::Text,
        _ => Shape::Default,
    });
    layered.shape();
    with(|wayland| {
        let _ = wayland.connection.flush();
    });
}

/// The same for the keys of a window that has the keyboard.
pub(super) fn hear_keys(window: &slint::Window, hear: impl Fn(Keyed) -> bool + 'static) {
    if let Some(layered) = of(window) {
        *layered.keyed.borrow_mut() = Some(Rc::new(hear));
    }
}

/// Moves a window that was put at a place, by so many of its device
/// pixels: nobody moves a layer surface but itself, by its margins. Says
/// by how many it went, which is whole logical pixels.
pub(super) fn move_by(window: &slint::Window, by: (f64, f64)) -> (f64, f64) {
    let Some(layered) = of(window) else { return (0.0, 0.0) };
    let scale = f64::from(layered.scale.get().max(1));
    let (x, y) = layered.at.get();
    let to = (x + (by.0 / scale).round() as i32, y + (by.1 / scale).round() as i32);
    let went = (f64::from(to.0 - x) * scale, f64::from(to.1 - y) * scale);
    if to == (x, y) {
        return went;
    }
    layered.at.set(to);
    if let Some(layer) = layered.surface.borrow().as_ref() {
        layer.set_margin(to.1, 0, 0, to.0);
        layer.commit();
    }
    with(|wayland| {
        let _ = wayland.connection.flush();
    });
    went
}

/// Where the window goes, to be said before it is shown.
pub(super) fn place(window: &slint::Window, at: Option<OwnPlace>) {
    if let Some(layered) = of(window) {
        layered.place.set(at);
    }
}

/// Where in the window the pointer is its own, in its device pixels; or
/// everywhere.
pub(super) fn reach(window: &slint::Window, within: Option<Rect>) {
    if let Some(layered) = of(window) {
        layered.reach.set(within);
        layered.dirty.set(true);
        soon();
    }
}

// --- What the compositor says ---------------------------------------------------------

impl CompositorHandler for State {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, scale: i32) {
        self.said.push(Said::Scaled(surface.clone(), scale));
    }

    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, _: u32) {
        self.said.push(Said::Shown(surface.clone()));
    }

    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}

    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.said.push(Said::Closed(layer.wl_surface().clone()));
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface, configure: LayerSurfaceConfigure, _: u32) {
        self.said.push(Said::Sized(layer.wl_surface().clone(), configure.new_size));
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(&mut self, _: &Connection, queue: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seats.get_pointer(queue, &seat).ok();
            self.shape = self.shapes.as_ref().zip(self.pointer.as_ref()).map(|(shapes, pointer)| shapes.get_shape_device(pointer, queue));
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = Some(seat.get_keyboard(queue, ()));
        }
    }

    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
            if let Some(shape) = self.shape.take() {
                shape.destroy();
            }
            self.inside = None;
        }
        if capability == Capability::Keyboard {
            if let Some(keyboard) = self.keyboard.take() {
                keyboard.release();
            }
            self.keyed = None;
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for State {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for event in events {
            self.said.push(Said::Pointer(event.surface.clone(), event.kind.clone(), event.position));
        }
    }
}

/// The keyboard, heard here and not through the toolkit, whose own
/// hearing of it would bring a second copy of xkbcommon into the build
/// (`keys`).
impl wayland_client::Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(state: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use wl_keyboard::Event;
        match event {
            Event::Keymap { format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1), fd, size } => state.keys.layout(fd, size),
            Event::RepeatInfo { rate, delay } => (state.keys.rate, state.keys.delay) = (rate.max(0) as u32, delay.max(0) as u32),
            Event::Enter { surface, .. } => {
                // The keys that were down already are not presses in it.
                state.keyed = Some(surface.clone());
                state.said.push(Said::Keyboard(surface, true));
            }
            Event::Leave { surface, .. } => {
                state.keyed = None;
                state.said.push(Said::Keyboard(surface, false));
            }
            Event::Key { key, state: pressed, .. } => {
                let down = pressed == WEnum::Value(wl_keyboard::KeyState::Pressed);
                if let (Some(surface), Some(press)) = (state.keyed.clone(), state.keys.key(key, down)) {
                    state.said.push(Said::Key(surface, press, down));
                }
            }
            Event::Modifiers { mods_depressed, mods_latched, mods_locked, group, .. } => {
                let (ctrl, alt) = state.keys.held(mods_depressed, mods_latched, mods_locked, group);
                if let Some(surface) = state.keyed.clone() {
                    state.said.push(Said::Held(surface, ctrl, alt));
                }
            }
            _ => {}
        }
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(State);
delegate_output!(State);
delegate_layer!(State);
delegate_seat!(State);
delegate_pointer!(State);
delegate_shm!(State);
delegate_registry!(State);
