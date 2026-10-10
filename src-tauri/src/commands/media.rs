//! Commands for screenshots, text recognition and dictation.

use serde::Serialize;
use tauri::AppHandle;

use crate::*;

#[tauri::command]
pub(crate) async fn screenshot_finish(request: screenshot::Finish, app: AppHandle) -> Result<String, String> {
    screenshot::finish(app, request).await
}

#[derive(Serialize)]
pub(crate) struct OcrStatus {
    installed: bool,
    installing: bool,
}

#[tauri::command]
pub(crate) fn ocr_status() -> OcrStatus {
    OcrStatus { installed: ocr::installed(), installing: ocr::installing() }
}

#[tauri::command]
pub(crate) fn ocr_install(app: AppHandle) -> Result<(), String> {
    ocr::install(&app)
}

#[tauri::command(async)]
pub(crate) fn dictation_status(app: AppHandle) -> dictation::Status {
    dictation::status(&app)
}

#[tauri::command]
pub(crate) fn dictation_install(app: AppHandle) -> Result<(), String> {
    dictation::install(&app)
}

#[tauri::command(async)]
pub(crate) fn dictation_toggle(app: AppHandle) {
    dictation::toggle(&app);
}

#[tauri::command(async)]
pub(crate) fn dictation_cancel(app: AppHandle) {
    dictation::cancel(&app);
}
