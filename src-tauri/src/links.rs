//! `raycast://` links. Extensions sign in to services through OAuth apps
//! registered with Raycast's redirect addresses, which end in a
//! `raycast://oauth?…` or `com.raycast:/oauth?…` link. Heyo registers itself
//! as the desktop's handler for those schemes so the sign-in can finish here.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DESKTOP_FILE: &str = "heyo-links.desktop";
const SCHEMES: &[&str] = &["raycast", "com.raycast"];

fn applications_dir() -> PathBuf {
    crate::paths::data_home().join("applications")
}

fn desktop_entry(exe: &str) -> String {
    let mime: String = SCHEMES.iter().map(|s| format!("x-scheme-handler/{s};")).collect();
    let exe = shlex::try_quote(exe).map(|q| q.into_owned()).unwrap_or_else(|_| exe.into());
    format!(
        "[Desktop Entry]\nType=Application\nName=Heyo\nComment=Finishes sign-ins started by Heyo extensions\n\
         Exec={exe} --link %u\nNoDisplay=true\nMimeType={mime}\n"
    )
}

/// Makes this executable the handler for the link schemes. Cheap when
/// nothing changed, so it runs at every start.
pub fn register() {
    let Ok(exe) = std::env::current_exe() else { return };
    let entry = desktop_entry(&exe.to_string_lossy());
    let dir = applications_dir();
    let file = dir.join(DESKTOP_FILE);
    if fs::read_to_string(&file).is_ok_and(|current| current == entry) {
        return;
    }
    if fs::create_dir_all(&dir).and_then(|()| fs::write(&file, entry)).is_err() {
        return;
    }
    std::thread::spawn(|| {
        for scheme in SCHEMES {
            let mime = format!("x-scheme-handler/{scheme}");
            let _ = Command::new("xdg-mime").args(["default", DESKTOP_FILE, &mime]).status();
        }
    });
}

/// Whether `link` is a sign-in redirect, the only kind of link handled so far.
pub fn is_oauth(link: &str) -> bool {
    ["raycast://oauth?", "com.raycast:/oauth?"].iter().any(|prefix| link.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_sign_in_redirects_only() {
        assert!(is_oauth("raycast://oauth?code=abc&package_name=Extension&state=xyz"));
        assert!(is_oauth("com.raycast:/oauth?package_name=Extension&code=abc"));
        assert!(!is_oauth("raycast://extensions/someone/something/command"));
        assert!(!is_oauth("https://example.com/raycast://oauth?code=abc"));
    }

    #[test]
    fn desktop_entry_declares_the_schemes() {
        let entry = desktop_entry("/home/me/my apps/heyo");
        assert!(entry.contains("Exec='/home/me/my apps/heyo' --link %u"));
        assert!(entry.contains("MimeType=x-scheme-handler/raycast;x-scheme-handler/com.raycast;"));
    }
}
