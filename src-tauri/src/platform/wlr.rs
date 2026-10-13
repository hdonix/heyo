//! Reading the screen where a Wayland compositor lets a program ask for it
//! outright: the protocol wlr-screencopy, which Sway, Hyprland and the
//! other wlroots compositors offer. (GNOME does not: there Heyo's Shell
//! extension reads the screen. A desktop that offers neither has the
//! portal's screenshot, which asks the user each time: not built.)
//!
//! One output is read, into a buffer the compositor fills, in the
//! output's own pixels. A connection is made for it and let go after.
//!
//! Which output: a Wayland program is not told where the pointer is
//! unless it is over a surface of its own. These compositors put a layer
//! surface that names no output on the one they hold to be in use, which
//! is the one the pointer was last moved on or a key last typed on; so a
//! surface of one pixel with nothing in it is put up, asked where it
//! went, and taken away (`pointed`).

use std::os::fd::{AsFd, FromRawFd, OwnedFd};

use smithay_client_toolkit::reexports::protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};
use smithay_client_toolkit::reexports::protocols_wlr::screencopy::v1::client::{zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1};
use wayland_client::globals::{registry_queue_init, GlobalList, GlobalListContents};
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_output, wl_region, wl_registry, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};

use super::capture::{Area, Shot};

/// What the compositor said of the frame so far.
#[derive(Default)]
struct Heard {
    /// The buffer it wants: format, width, height, bytes a row.
    wanted: Option<(wl_shm::Format, u32, u32, u32)>,
    /// It is drawn bottom row first.
    upside_down: bool,
    /// The buffer is filled, or it could not be.
    done: Option<bool>,
    /// Where each output starts on the desktop, in the desktop's pixels:
    /// as the output says it, and then as the desktop says it of the
    /// output, which is the one to go by where both are said (the
    /// wlroots compositors have every output say 0, 0).
    starts: Vec<(wl_output::WlOutput, (i32, i32))>,
    /// How many of its own pixels one of the desktop's is, for those that say.
    scales: Vec<(wl_output::WlOutput, i32)>,
    /// The surface put up to find the output in use: the compositor has
    /// answered it (with this number), it is on this output, it was refused.
    answered: Option<u32>,
    entered: Option<wl_output::WlOutput>,
    refused: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Heard {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_output::WlOutput, ()> for Heard {
    fn event(heard: &mut Self, output: &wl_output::WlOutput, event: wl_output::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_output::Event::Geometry { x, y, .. } => heard.starts.push((output.clone(), (x, y))),
            wl_output::Event::Scale { factor } => heard.scales.push((output.clone(), factor)),
            _ => {}
        }
    }
}

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, wl_output::WlOutput> for Heard {
    fn event(heard: &mut Self, _: &zxdg_output_v1::ZxdgOutputV1, event: zxdg_output_v1::Event, output: &wl_output::WlOutput, _: &Connection, _: &QueueHandle<Self>) {
        if let zxdg_output_v1::Event::LogicalPosition { x, y } = event {
            heard.starts.push((output.clone(), (x, y)));
        }
    }
}

impl Heard {
    /// Where an output starts, by the last word on it.
    fn start(&self, output: &wl_output::WlOutput) -> Option<(i32, i32)> {
        self.starts.iter().rev().find(|(known, _)| known == output).map(|(_, start)| *start)
    }
}

/// The compositor's outputs, each asked where it starts: said by the
/// time the compositor has next been heard out.
fn outputs(globals: &GlobalList, handle: &QueueHandle<Heard>) -> Vec<wl_output::WlOutput> {
    let outputs: Vec<wl_output::WlOutput> = globals.contents().with_list(|all| all.iter().filter(|global| global.interface == "wl_output").map(|global| globals.registry().bind(global.name, global.version.min(2), handle, ())).collect());
    if let Ok(desktop) = globals.bind::<zxdg_output_manager_v1::ZxdgOutputManagerV1, _, _>(handle, 1..=3, ()) {
        for output in &outputs {
            desktop.get_xdg_output(output, handle, output.clone());
        }
    }
    outputs
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for Heard {
    fn event(heard: &mut Self, _: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, event: zwlr_layer_surface_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => heard.answered = Some(serial),
            zwlr_layer_surface_v1::Event::Closed => heard.refused = true,
            _ => {}
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for Heard {
    fn event(heard: &mut Self, _: &wl_surface::WlSurface, event: wl_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_surface::Event::Enter { output } = event {
            heard.entered = Some(output);
        }
    }
}

impl Dispatch<zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1, ()> for Heard {
    fn event(heard: &mut Self, _: &zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1, event: zwlr_screencopy_frame_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use zwlr_screencopy_frame_v1::{Event, Flags};
        match event {
            Event::Buffer { format: WEnum::Value(format), width, height, stride } => heard.wanted = Some((format, width, height, stride)),
            Event::Flags { flags: WEnum::Value(flags) } => heard.upside_down = flags.contains(Flags::YInvert),
            Event::Ready { .. } => heard.done = Some(true),
            Event::Failed => heard.done = Some(false),
            _ => {}
        }
    }
}

macro_rules! unheard {
    ($($what:ty),*) => {$(
        impl Dispatch<$what, ()> for Heard {
            fn event(_: &mut Self, _: &$what, _: <$what as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        }
    )*};
}
unheard!(wl_shm::WlShm, wl_shm_pool::WlShmPool, wl_buffer::WlBuffer, zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1, wl_compositor::WlCompositor, wl_region::WlRegion, zwlr_layer_shell_v1::ZwlrLayerShellV1, zxdg_output_manager_v1::ZxdgOutputManagerV1);

/// A file in memory of that many bytes, all zero, for the compositor and
/// this program to share.
fn shared(size: usize) -> Result<OwnedFd, String> {
    // SAFETY: a new descriptor, owned from here.
    unsafe {
        let descriptor = libc::memfd_create(c"heyo-capture".as_ptr(), libc::MFD_CLOEXEC);
        if descriptor < 0 {
            return Err("no memory for the screen's picture".into());
        }
        let file = OwnedFd::from_raw_fd(descriptor);
        if libc::ftruncate(descriptor, size as libc::off_t) != 0 {
            return Err("no memory for the screen's picture".into());
        }
        Ok(file)
    }
}

/// Where the output the compositor holds to be in use starts, in the
/// pixels the toolkit counts monitors in (the output's own): the one the
/// pointer is on, as nearly as a Wayland program can be told. None where
/// there is one output only, or no layer shell to ask with.
pub fn pointed() -> Option<(i32, i32)> {
    let connection = Connection::connect_to_env().ok()?;
    let (globals, mut queue) = registry_queue_init::<Heard>(&connection).ok()?;
    let handle = queue.handle();
    let shell: zwlr_layer_shell_v1::ZwlrLayerShellV1 = globals.bind(&handle, 1..=1, ()).ok()?;
    let compositor: wl_compositor::WlCompositor = globals.bind(&handle, 1..=4, ()).ok()?;
    let shm: wl_shm::WlShm = globals.bind(&handle, 1..=1, ()).ok()?;
    let outputs = outputs(&globals, &handle);
    if outputs.len() < 2 {
        return None;
    }
    let mut heard = Heard::default();
    // One pixel in a corner, with nothing in it and nothing of the
    // pointer's, on whichever output the compositor likes. Over
    // everything: a compositor does not say which output a surface is on
    // while a window hides it.
    let surface = compositor.create_surface(&handle, ());
    let layer = shell.get_layer_surface(&surface, None, zwlr_layer_shell_v1::Layer::Overlay, "heyo".into(), &handle, ());
    layer.set_size(1, 1);
    layer.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left);
    let nowhere = compositor.create_region(&handle, ());
    surface.set_input_region(Some(&nowhere));
    nowhere.destroy();
    surface.commit();
    for _ in 0..3 {
        if heard.answered.is_some() || heard.refused || queue.roundtrip(&mut heard).is_err() {
            break;
        }
    }
    let mut shown = None;
    if let (Some(serial), Ok(file)) = (heard.answered, shared(4)) {
        layer.ack_configure(serial);
        let pool = shm.create_pool(file.as_fd(), 4, &handle, ());
        let buffer = pool.create_buffer(0, 1, 1, 4, wl_shm::Format::Argb8888, &handle, ());
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        for _ in 0..3 {
            if heard.entered.is_some() || heard.refused || queue.roundtrip(&mut heard).is_err() {
                break;
            }
        }
        shown = Some((pool, buffer));
    }
    layer.destroy();
    surface.destroy();
    if let Some((pool, buffer)) = shown {
        buffer.destroy();
        pool.destroy();
    }
    let _ = connection.flush();
    let output = heard.entered.take()?;
    let start = heard.start(&output)?;
    let scale = heard.scales.iter().rev().find(|(known, _)| *known == output).map_or(1, |(_, scale)| (*scale).max(1));
    Some((start.0 * scale, start.1 * scale))
}

/// Whether the compositor offers it.
pub fn offered() -> bool {
    let Ok(connection) = Connection::connect_to_env() else { return false };
    let Ok((globals, _queue)) = registry_queue_init::<Heard>(&connection) else { return false };
    globals.contents().with_list(|all| all.iter().any(|global| global.interface == "zwlr_screencopy_manager_v1"))
}

/// The pixels of a picture as the compositor wrote them, as blue, green,
/// red and a byte that means nothing: which is what most formats are
/// already, and the others have red and blue the other way round.
fn bgrx(format: wl_shm::Format, pixels: &mut [u8]) -> Result<(), String> {
    match format {
        wl_shm::Format::Xrgb8888 | wl_shm::Format::Argb8888 => Ok(()),
        wl_shm::Format::Xbgr8888 | wl_shm::Format::Abgr8888 => {
            pixels.chunks_exact_mut(4).for_each(|pixel| pixel.swap(0, 2));
            Ok(())
        }
        other => Err(format!("the compositor writes the screen as {other:?}, which is not read here")),
    }
}

/// The rows of a picture, each `stride` bytes of which `width` pixels are
/// the picture's, as one run of pixels from the top row down.
fn rows(pixels: &[u8], width: u32, height: u32, stride: u32, upside_down: bool) -> Vec<u8> {
    let (row, stride) = (width as usize * 4, stride as usize);
    let mut packed = Vec::with_capacity(row * height as usize);
    let each = pixels.chunks(stride).take(height as usize).map(|line| &line[..row.min(line.len())]);
    if upside_down {
        each.rev().for_each(|line| packed.extend_from_slice(line));
    } else {
        each.for_each(|line| packed.extend_from_slice(line));
    }
    packed
}

/// Reads the output that `area` is, in its own pixels.
pub fn grab(area: Area) -> Result<Shot, String> {
    let connection = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, mut queue) = registry_queue_init::<Heard>(&connection).map_err(|e| e.to_string())?;
    let handle = queue.handle();
    let manager: zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1 = globals.bind(&handle, 1..=1, ()).map_err(|_| "the compositor does not let its screen be read (no wlr-screencopy)".to_string())?;
    let shm: wl_shm::WlShm = globals.bind(&handle, 1..=1, ()).map_err(|e| e.to_string())?;
    let outputs = outputs(&globals, &handle);
    let mut heard = Heard::default();
    queue.roundtrip(&mut heard).map_err(|e| e.to_string())?;
    // The output that starts where the area does, as the compositor lays
    // them out in logical pixels; the first where none does.
    let scale = if area.scale > 0.0 { area.scale } else { 1.0 };
    let at = ((f64::from(area.x) / scale).round() as i32, (f64::from(area.y) / scale).round() as i32);
    let output = outputs.iter().find(|output| heard.start(output) == Some(at)).or(outputs.first()).cloned().ok_or("the compositor has no output")?;

    let frame = manager.capture_output(0, &output, &handle, ());
    while heard.wanted.is_none() && heard.done.is_none() {
        queue.blocking_dispatch(&mut heard).map_err(|e| e.to_string())?;
    }
    let (format, width, height, stride) = heard.wanted.ok_or("the compositor would not hand its screen over")?;
    let size = stride as usize * height as usize;
    // A file in memory, which the compositor writes into.
    let file = shared(size)?;
    // SAFETY: the mapping is of the file's whole length and is let go below.
    let mapped = unsafe { libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, std::os::fd::AsRawFd::as_raw_fd(&file), 0) };
    if mapped == libc::MAP_FAILED {
        return Err("no memory for the screen's picture".into());
    }
    let pool = shm.create_pool(file.as_fd(), size as i32, &handle, ());
    let buffer = pool.create_buffer(0, width as i32, height as i32, stride as i32, format, &handle, ());
    frame.copy(&buffer);
    let mut result = Ok(());
    while heard.done.is_none() && result.is_ok() {
        result = queue.blocking_dispatch(&mut heard).map(|_| ()).map_err(|e| e.to_string());
    }
    // SAFETY: the mapping made above, read once the compositor says it is filled.
    let mut pixels = unsafe { std::slice::from_raw_parts(mapped.cast::<u8>(), size).to_vec() };
    unsafe {
        libc::munmap(mapped, size);
    }
    buffer.destroy();
    pool.destroy();
    frame.destroy();
    let _ = connection.flush();
    result?;
    if heard.done != Some(true) {
        return Err("the compositor could not hand its screen over".into());
    }
    bgrx(format, &mut pixels)?;
    Ok(Shot { width, height, bgrx: rows(&pixels, width, height, stride, heard.upside_down) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_read_row_by_row_whatever_way_up_and_however_long_its_rows() {
        // Two pixels a row, two rows, each row with four bytes of padding.
        let written = [1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0, 3, 3, 3, 3, 4, 4, 4, 4, 0, 0, 0, 0];
        assert_eq!(rows(&written, 2, 2, 12, false), [1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4]);
        assert_eq!(rows(&written, 2, 2, 12, true), [3, 3, 3, 3, 4, 4, 4, 4, 1, 1, 1, 1, 2, 2, 2, 2]);
    }

    #[test]
    fn red_and_blue_are_put_where_a_capture_has_them() {
        let mut pixel = [10, 20, 30, 255];
        bgrx(wl_shm::Format::Xrgb8888, &mut pixel).unwrap();
        assert_eq!(pixel, [10, 20, 30, 255]);
        bgrx(wl_shm::Format::Xbgr8888, &mut pixel).unwrap();
        assert_eq!(pixel, [30, 20, 10, 255]);
        assert!(bgrx(wl_shm::Format::Rgb565, &mut pixel).is_err());
    }
}
