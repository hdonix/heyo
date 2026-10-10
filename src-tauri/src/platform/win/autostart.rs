//! Starting Heyo when the user signs in to Windows: a value named `Heyo`
//! under the user's `Run` key, which Windows runs at every sign-in. Nothing
//! else is stored: whether Heyo launches at login is whether the value is
//! there.

use std::path::Path;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ};

use super::wide;

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "Heyo";

pub fn enabled() -> bool {
    let (key, value) = (wide(KEY), wide(VALUE));
    // Asked only whether it is there: no buffer, so nothing is copied out.
    let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, null_mut(), null_mut(), null_mut()) };
    status == ERROR_SUCCESS
}

pub fn set(enabled: bool, exe: &Path) -> Result<(), String> {
    let (key, value) = (wide(KEY), wide(VALUE));
    let status = if enabled {
        let command = wide(&crate::settings::login_command(&exe.to_string_lossy()));
        let bytes = (command.len() * std::mem::size_of::<u16>()) as u32;
        unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), REG_SZ, command.as_ptr().cast(), bytes) }
    } else {
        match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) } {
            ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
            status => status,
        }
    };
    if status != ERROR_SUCCESS {
        return Err(format!("failed to change what Windows starts at sign-in: {}", std::io::Error::from_raw_os_error(status as i32)));
    }
    Ok(())
}
