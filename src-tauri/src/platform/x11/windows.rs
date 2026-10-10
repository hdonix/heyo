//! Other applications' windows through X11 and the EWMH window-manager hints.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt, EventMask};
use x11rb::rust_connection::RustConnection;

use crate::platform::windows::{target, Layout, Rect, Window, WindowInfo};

struct X {
    conn: RustConnection,
    root: Window,
}

impl X {
    fn connect() -> Result<Self, String> {
        let (conn, screen) = x11rb::connect(None).map_err(|e| format!("no X11 display: {e}"))?;
        let root = conn.setup().roots[screen].root;
        Ok(X { conn, root })
    }

    fn atom(&self, name: &str) -> Atom {
        self.conn.intern_atom(false, name.as_bytes()).ok().and_then(|c| c.reply().ok()).map_or(0, |r| r.atom)
    }

    fn cardinals(&self, window: Window, property: &str) -> Vec<u32> {
        self.conn
            .get_property(false, window, self.atom(property), AtomEnum::ANY, 0, 4096)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    fn bytes(&self, window: Window, property: Atom) -> Vec<u8> {
        self.conn
            .get_property(false, window, property, AtomEnum::ANY, 0, 4096)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.value)
            .unwrap_or_default()
    }

    fn title(&self, window: Window) -> String {
        let name = self.bytes(window, self.atom("_NET_WM_NAME"));
        let name = if name.is_empty() { self.bytes(window, AtomEnum::WM_NAME.into()) } else { name };
        String::from_utf8_lossy(&name).into_owned()
    }

    /// WM_CLASS is "instance\0class\0"; the class is the readable name.
    fn class(&self, window: Window) -> String {
        let value = self.bytes(window, AtomEnum::WM_CLASS.into());
        value.split(|b| *b == 0).rfind(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).into_owned()).unwrap_or_default()
    }

    fn active(&self) -> Option<Window> {
        self.cardinals(self.root, "_NET_ACTIVE_WINDOW").first().copied().filter(|w| *w != 0)
    }

    /// Asks the window manager to do something with a window.
    fn message(&self, window: Window, kind: &str, data: [u32; 5]) -> Result<(), String> {
        let event = ClientMessageEvent::new(32, window, self.atom(kind), data);
        self.conn
            .send_event(false, self.root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, event)
            .map_err(|e| e.to_string())?;
        self.conn.flush().map_err(|e| e.to_string())
    }

    /// The X server's current time, which only arrives stamped on events:
    /// change a property on a throwaway window and read the notification.
    fn server_time(&self) -> Option<u32> {
        use x11rb::protocol::xproto::{CreateWindowAux, PropMode, WindowClass};
        use x11rb::wrapper::ConnectionExt as _;
        use x11rb::protocol::Event;
        let window = self.conn.generate_id().ok()?;
        let aux = CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE);
        self.conn.create_window(0, window, self.root, -1, -1, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &aux).ok()?;
        self.conn.change_property8(PropMode::REPLACE, window, AtomEnum::WM_NAME, AtomEnum::STRING, b"").ok()?;
        self.conn.flush().ok()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(300);
        let mut time = None;
        while time.is_none() && std::time::Instant::now() < deadline {
            match self.conn.poll_for_event().ok()? {
                Some(Event::PropertyNotify(event)) if event.window == window => time = Some(event.time),
                Some(_) => {}
                None => std::thread::sleep(std::time::Duration::from_millis(2)),
            }
        }
        let _ = self.conn.destroy_window(window);
        let _ = self.conn.flush();
        time
    }

    fn is_own(&self, window: Window) -> bool {
        self.class(window).eq_ignore_ascii_case("heyo")
    }

    /// The window's outer rectangle on screen (without invisible shadows),
    /// and the margins to add back when positioning it.
    fn frame(&self, window: Window) -> Option<(Rect, Margins)> {
        let geometry = self.conn.get_geometry(window).ok()?.reply().ok()?;
        let origin = self.conn.translate_coordinates(window, self.root, 0, 0).ok()?.reply().ok()?;
        let (x, y) = (origin.dst_x as i32, origin.dst_y as i32);
        let (width, height) = (geometry.width as i32, geometry.height as i32);
        // Client-side decorated (GTK) windows include their shadow in the X window.
        if let [left, right, top, bottom] = self.cardinals(window, "_GTK_FRAME_EXTENTS")[..] {
            let m = Margins { left: left as i32, right: right as i32, top: top as i32, bottom: bottom as i32, client_side: true };
            return Some((Rect { x: x + m.left, y: y + m.top, width: width - m.left - m.right, height: height - m.top - m.bottom }, m));
        }
        let m = match self.cardinals(window, "_NET_FRAME_EXTENTS")[..] {
            [left, right, top, bottom] => Margins { left: left as i32, right: right as i32, top: top as i32, bottom: bottom as i32, client_side: false },
            _ => Margins::default(),
        };
        Some((Rect { x: x - m.left, y: y - m.top, width: width + m.left + m.right, height: height + m.top + m.bottom }, m))
    }
}

#[derive(Clone, Copy, Default)]
struct Margins {
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
    /// Shadows drawn by the application (inside the X window), as opposed to
    /// a frame added by the window manager (outside it).
    client_side: bool,
}

/// Which keys are held down right now, as the X server's 256-bit keymap.
pub fn keys_down() -> Option<[u8; 32]> {
    let x = X::connect().ok()?;
    let reply = x.conn.query_keymap().ok()?.reply().ok()?;
    Some(reply.keys)
}

/// The window class of the active window.
pub fn active_class() -> Option<String> {
    let x = X::connect().ok()?;
    let class = x.class(x.active()?);
    (!class.is_empty()).then_some(class)
}

/// The active window, unless it is one of Heyo's own.
pub fn active_external() -> Option<Window> {
    let x = X::connect().ok()?;
    x.active().filter(|w| !x.is_own(*w))
}

/// Application windows, most recently used first; `current` (the window the
/// user came from) goes last, so the first one is the one to switch back to.
pub fn list(current: Option<Window>) -> Result<Vec<WindowInfo>, String> {
    let x = X::connect()?;
    let active = current;
    let skip_types = [x.atom("_NET_WM_WINDOW_TYPE_DOCK"), x.atom("_NET_WM_WINDOW_TYPE_DESKTOP")];
    let skip_taskbar = x.atom("_NET_WM_STATE_SKIP_TASKBAR");
    // Stacking order is bottom to top.
    let mut ids = x.cardinals(x.root, "_NET_CLIENT_LIST_STACKING");
    if ids.is_empty() {
        ids = x.cardinals(x.root, "_NET_CLIENT_LIST");
    }
    let mut windows: Vec<WindowInfo> = ids
        .into_iter()
        .rev()
        .filter(|&id| !x.cardinals(id, "_NET_WM_WINDOW_TYPE").iter().any(|t| skip_types.contains(t)))
        .filter(|&id| !x.cardinals(id, "_NET_WM_STATE").contains(&skip_taskbar))
        .filter(|&id| !x.is_own(id))
        .map(|id| WindowInfo { id, title: x.title(id), class: x.class(id), active: Some(id) == active, bounds: x.frame(id).map(|(frame, _)| frame) })
        .filter(|w| !w.title.is_empty())
        .collect();
    windows.sort_by_key(|w| w.active);
    Ok(windows)
}

/// The application windows on screen now, the topmost first: `list` without
/// the minimized ones and those on other workspaces.
pub fn shown() -> Result<Vec<WindowInfo>, String> {
    let x = X::connect()?;
    let hidden = x.atom("_NET_WM_STATE_HIDDEN");
    let workspace = x.cardinals(x.root, "_NET_CURRENT_DESKTOP").first().copied();
    let mut windows = list(None)?;
    windows.retain(|w| {
        // 0xFFFFFFFF: on every workspace. A window that says nothing is taken to be here.
        let elsewhere = match (x.cardinals(w.id, "_NET_WM_DESKTOP").first(), workspace) {
            (Some(&on), Some(current)) => on != current && on != u32::MAX,
            _ => false,
        };
        !elsewhere && !x.cardinals(w.id, "_NET_WM_STATE").contains(&hidden)
    });
    Ok(windows)
}

/// The application window used most recently before `window`: the one to
/// go back to from it.
pub fn before(window: Window) -> Option<Window> {
    let x = X::connect().ok()?;
    let skip_types = [x.atom("_NET_WM_WINDOW_TYPE_DOCK"), x.atom("_NET_WM_WINDOW_TYPE_DESKTOP")];
    let skip_taskbar = x.atom("_NET_WM_STATE_SKIP_TASKBAR");
    let mut ids = x.cardinals(x.root, "_NET_CLIENT_LIST_STACKING");
    if ids.is_empty() {
        ids = x.cardinals(x.root, "_NET_CLIENT_LIST");
    }
    // Stacking order is bottom to top: the next one down is the previous one used.
    ids.into_iter()
        .rev()
        .filter(|&id| id != window)
        .filter(|&id| !x.cardinals(id, "_NET_WM_WINDOW_TYPE").iter().any(|t| skip_types.contains(t)))
        .filter(|&id| !x.cardinals(id, "_NET_WM_STATE").contains(&skip_taskbar))
        .find(|&id| !x.is_own(id))
}

/// The windows whose class (either half of WM_CLASS) is one of `classes`,
/// most recently used first. `classes` are lowercase.
pub fn of_classes(classes: &[String]) -> Vec<Window> {
    let Ok(x) = X::connect() else { return Vec::new() };
    if classes.is_empty() {
        return Vec::new();
    }
    let skip_taskbar = x.atom("_NET_WM_STATE_SKIP_TASKBAR");
    let mut ids = x.cardinals(x.root, "_NET_CLIENT_LIST_STACKING");
    if ids.is_empty() {
        ids = x.cardinals(x.root, "_NET_CLIENT_LIST");
    }
    ids.into_iter()
        .rev()
        .filter(|&id| !x.cardinals(id, "_NET_WM_STATE").contains(&skip_taskbar))
        .filter(|&id| {
            // WM_CLASS is "instance\0class\0".
            let value = x.bytes(id, AtomEnum::WM_CLASS.into());
            value.split(|b| *b == 0).filter(|part| !part.is_empty()).any(|part| classes.contains(&String::from_utf8_lossy(part).to_lowercase()))
        })
        .collect()
}

/// Brings a window to the front, switching workspace if needed, and makes
/// sure it happened.
///
/// Window managers guard against programs stealing the focus, each in its
/// own way. GNOME's accepts a request without a timestamp as made "now", and
/// turns down one whose timestamp is older than the user's last key press —
/// which the hotkey that led here easily is. Others ignore requests without
/// a timestamp. So: ask without one, check, and ask again with the server's
/// current time if the window did not come to the front.
pub fn activate(window: Window) -> Result<(), String> {
    let x = X::connect()?;
    for attempt in 0..3 {
        let time = if attempt == 0 { x11rb::CURRENT_TIME } else { x.server_time().unwrap_or(x11rb::CURRENT_TIME) };
        if let Some(&desktop) = x.cardinals(window, "_NET_WM_DESKTOP").first().filter(|d| **d != u32::MAX) {
            if x.cardinals(x.root, "_NET_CURRENT_DESKTOP").first() != Some(&desktop) {
                x.message(x.root, "_NET_CURRENT_DESKTOP", [desktop, time, 0, 0, 0])?;
            }
        }
        // Source indication 2: a pager, which window managers obey.
        x.message(window, "_NET_ACTIVE_WINDOW", [2, time, 0, 0, 0])?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
        while std::time::Instant::now() < deadline {
            if x.active() == Some(window) {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
    }
    Err("the window manager did not bring the window to the front".into())
}

pub fn close(window: Window) -> Result<(), String> {
    let x = X::connect()?;
    x.message(window, "_NET_CLOSE_WINDOW", [x11rb::CURRENT_TIME, 2, 0, 0, 0])
}

/// How long a window manager gets to act on a request.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(120);

/// Moves a window into a layout. `monitors` are the monitors' rectangles.
pub fn apply(window: Window, layout: Layout, monitors: &[Rect]) -> Result<(), String> {
    let x = X::connect()?;
    let (frame, _) = x.frame(window).ok_or("the window is gone")?;
    let center = (frame.x + frame.width / 2, frame.y + frame.height / 2);
    let index = monitors.iter().position(|m| m.contains(center.0, center.1)).unwrap_or(0);
    let monitor = *monitors.get(index).ok_or("no monitor")?;
    // The work area excludes panels and docks.
    let work = match x.cardinals(x.root, "_NET_WORKAREA")[..] {
        [wx, wy, ww, wh, ..] => Rect { x: wx as i32, y: wy as i32, width: ww as i32, height: wh as i32 },
        _ => monitor,
    };
    let usable = |m: Rect| {
        let area = m.intersect(&work);
        if area.width > 0 && area.height > 0 { area } else { m }
    };

    let state = |action: u32, first: &str, second: &str| {
        x.message(window, "_NET_WM_STATE", [action, x.atom(first), if second.is_empty() { 0 } else { x.atom(second) }, 2, 0])
    };
    if layout == Layout::Maximize {
        return state(1, "_NET_WM_STATE_MAXIMIZED_VERT", "_NET_WM_STATE_MAXIMIZED_HORZ");
    }
    // A maximized or fullscreen window ignores move and resize requests.
    let states = x.cardinals(window, "_NET_WM_STATE");
    let restricted = ["_NET_WM_STATE_MAXIMIZED_VERT", "_NET_WM_STATE_MAXIMIZED_HORZ", "_NET_WM_STATE_FULLSCREEN"];
    if restricted.iter().any(|name| states.contains(&x.atom(name))) {
        state(0, "_NET_WM_STATE_MAXIMIZED_VERT", "_NET_WM_STATE_MAXIMIZED_HORZ")?;
        state(0, "_NET_WM_STATE_FULLSCREEN", "")?;
        // Let the window manager restore the window before moving it.
        std::thread::sleep(SETTLE);
    }
    // Its frame may differ from the maximized one.
    let (frame, margins) = x.frame(window).ok_or("the window is gone")?;

    let rect = if layout == Layout::NextDisplay {
        if monitors.len() < 2 {
            return Err("There is only one display.".into());
        }
        // Same relative place on the next monitor.
        let (from, to) = (usable(monitor), usable(monitors[(index + 1) % monitors.len()]));
        Rect {
            x: to.x + (frame.x - from.x) * to.width / from.width.max(1),
            y: to.y + (frame.y - from.y) * to.height / from.height.max(1),
            width: frame.width * to.width / from.width.max(1),
            height: frame.height * to.height / from.height.max(1),
        }
    } else {
        target(layout, usable(monitor), frame)
    };

    // With client-side decorations the X window is the visible rectangle plus
    // its shadow; otherwise the window manager places its frame at x, y and
    // the size is that of the contents.
    let (px, py, width, height) = if margins.client_side {
        (rect.x - margins.left, rect.y - margins.top, rect.width + margins.left + margins.right, rect.height + margins.top + margins.bottom)
    } else {
        (rect.x, rect.y, rect.width - margins.left - margins.right, rect.height - margins.top - margins.bottom)
    };
    // Gravity NorthWest, all four of x/y/width/height given, from a pager.
    let flags = 1 | (0b1111 << 8) | (2 << 12);
    x.message(window, "_NET_MOVERESIZE_WINDOW", [flags, px as u32, py as u32, width.max(1) as u32, height.max(1) as u32])?;

    // Some window managers drop the request now and then; ask again, the
    // plain X way, if the window did not end up where it should be.
    std::thread::sleep(SETTLE);
    let placed = x.frame(window).is_some_and(|(now, _)| {
        (now.x - rect.x).abs() <= 2 && (now.y - rect.y).abs() <= 2 && (now.width - rect.width).abs() <= 2 && (now.height - rect.height).abs() <= 2
    });
    if !placed {
        let values = ConfigureWindowAux::new().x(px).y(py).width(width.max(1) as u32).height(height.max(1) as u32);
        x.conn.configure_window(window, &values).map_err(|e| e.to_string())?;
        x.conn.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
