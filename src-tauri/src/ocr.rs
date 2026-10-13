//! Text recognition for screenshots, through the Node helper
//! (`runtime/src/ocr.ts`). The engine is downloaded on demand.

use crate::i18n::{t, tf};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter};

use crate::helper::{self, InstallProgress};
use crate::wire::event;

static INSTALLING: AtomicBool = AtomicBool::new(false);

pub fn dir() -> PathBuf {
    crate::paths::data().join("ocr")
}

pub fn installed() -> bool {
    dir().join("node_modules/@gutenye/ocr-node").exists()
}

pub fn installing() -> bool {
    INSTALLING.load(Ordering::Relaxed)
}

fn command(app: &AppHandle, arg: &str) -> Result<Command, String> {
    let mut command = helper::node(app, "heyo-ocr.cjs", "HEYO_OCR")?;
    command.arg(arg).env("HEYO_OCR_DIR", dir());
    Ok(command)
}

/// The text in an image, in reading order.
pub fn recognize(app: &AppHandle, image: &Path) -> Result<String, String> {
    if !installed() {
        return Err(t("Text recognition is not set up yet. Download it in Heyo Settings → Screenshots.").into());
    }
    let messages = helper::run(command(app, &image.to_string_lossy())?).map_err(|e| tf("failed to run text recognition: {0}", &[&e]))?;
    let result = messages.last().ok_or(t("text recognition returned nothing"))?;
    match result["error"].as_str() {
        Some(error) => Err(error.to_owned()),
        None => Ok(result["text"].as_str().unwrap_or_default().to_owned()),
    }
}

/// Downloads the engine, reporting to the settings window as
/// `heyo://ocr-install` events.
pub fn install(app: &AppHandle) -> Result<(), String> {
    if INSTALLING.swap(true, Ordering::Relaxed) {
        return Ok(());
    }
    let _ = app.emit(event::OCR_INSTALL, InstallProgress::Progress { step: t("Text recognition engine").into(), received: None, total: None });
    let window = app.clone();
    helper::install(command(app, "install"), move |progress| {
        if progress.concludes() {
            INSTALLING.store(false, Ordering::Relaxed);
        }
        let _ = window.emit(event::OCR_INSTALL, progress);
    });
    Ok(())
}
