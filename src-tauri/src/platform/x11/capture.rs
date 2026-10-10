//! Reading the screen straight from the X server.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};

use crate::platform::capture::{Area, Shot};

pub fn grab(area: Area) -> Result<Shot, String> {
    let (conn, screen) = x11rb::connect(None).map_err(|e| e.to_string())?;
    let root = conn.setup().roots[screen].root;
    let reply = conn
        .get_image(ImageFormat::Z_PIXMAP, root, area.x as i16, area.y as i16, area.width as u16, area.height as u16, !0)
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| e.to_string())?;
    if reply.data.len() != (area.width * area.height * 4) as usize {
        return Err(format!("unsupported screen format (depth {})", reply.depth));
    }
    Ok(Shot { width: area.width, height: area.height, bgrx: reply.data })
}
