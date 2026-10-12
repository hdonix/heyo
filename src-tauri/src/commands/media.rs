//! Commands for screenshots, text recognition and dictation.

use serde::Serialize;
use tauri::AppHandle;

use crate::*;

#[tauri::command]
pub(crate) async fn screenshot_finish(request: screenshot::Finish, app: AppHandle) -> Result<String, String> {
    screenshot::finish(app, request).await
}

/// Whether text recognition can be used, for the settings page.
#[derive(Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub(crate) struct OcrStatus {
    installed: bool,
    installing: bool,
}

pub(crate) fn ocr_status() -> OcrStatus {
    OcrStatus { installed: ocr::installed(), installing: ocr::installing() }
}

pub(crate) fn ocr_install(app: AppHandle) -> Result<(), String> {
    ocr::install(&app)
}

pub(crate) fn dictation_status(app: AppHandle) -> dictation::Status {
    dictation::status(&app)
}

pub(crate) fn dictation_install(app: AppHandle) -> Result<(), String> {
    dictation::install(&app)
}

