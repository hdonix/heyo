//! Reading the screen where a Wayland compositor lets a program ask for it
//! outright: the protocol wlr-screencopy, which Sway, Hyprland and the
//! other wlroots compositors offer. (GNOME does not: there Heyo's Shell
//! extension reads the screen. A desktop that offers neither has the
//! portal's screenshot, which asks the user each time: not built.)
//!
//! One output is read, into a buffer the compositor fills, in the
//! output's own pixels. A connection is made for it and let go after.

use std::os::fd::{AsFd, FromRawFd, OwnedFd};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols_wlr::screencopy::v1::client::{zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1};

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
    /// Where each output starts on the desktop.
    starts: Vec<(wl_output::WlOutput, (i32, i32))>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Heard {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_output::WlOutput, ()> for Heard {
    fn event(heard: &mut Self, output: &wl_output::WlOutput, event: wl_output::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_output::Event::Geometry { x, y, .. } = event {
            heard.starts.push((output.clone(), (x, y)));
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
unheard!(wl_shm::WlShm, wl_shm_pool::WlShmPool, wl_buffer::WlBuffer, zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1);

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
    let outputs: Vec<wl_output::WlOutput> = globals.contents().with_list(|all| all.iter().filter(|global| global.interface == "wl_output").map(|global| globals.registry().bind(global.name, global.version.min(2), &handle, ())).collect());
    let mut heard = Heard::default();
    queue.roundtrip(&mut heard).map_err(|e| e.to_string())?;
    // The output that starts where the area does, as the compositor lays
    // them out in logical pixels; the first where none does.
    let scale = if area.scale > 0.0 { area.scale } else { 1.0 };
    let at = ((f64::from(area.x) / scale).round() as i32, (f64::from(area.y) / scale).round() as i32);
    let output = heard.starts.iter().find(|(_, start)| *start == at).map(|(output, _)| output.clone()).or_else(|| outputs.first().cloned()).ok_or("the compositor has no output")?;

    let frame = manager.capture_output(0, &output, &handle, ());
    while heard.wanted.is_none() && heard.done.is_none() {
        queue.blocking_dispatch(&mut heard).map_err(|e| e.to_string())?;
    }
    let (format, width, height, stride) = heard.wanted.ok_or("the compositor would not hand its screen over")?;
    let size = stride as usize * height as usize;
    // A file in memory, which the compositor writes into.
    // SAFETY: a new descriptor, owned from here; the mapping is of its whole length and is let go below.
    let (file, mapped) = unsafe {
        let descriptor = libc::memfd_create(c"heyo-capture".as_ptr(), libc::MFD_CLOEXEC);
        if descriptor < 0 || libc::ftruncate(descriptor, size as libc::off_t) != 0 {
            return Err("no memory for the screen's picture".into());
        }
        let file = OwnedFd::from_raw_fd(descriptor);
        let mapped = libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, descriptor, 0);
        if mapped == libc::MAP_FAILED {
            return Err("no memory for the screen's picture".into());
        }
        (file, mapped)
    };
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
