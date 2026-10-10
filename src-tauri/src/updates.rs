//! Newer versions of Heyo, fetched and installed by Heyo itself.
//!
//! Off unless a release build turns it on: the `updater` feature compiles
//! the updater in, and the build's configuration must name where releases
//! are published and the key they are signed with (`plugins.updater`, see
//! `packaging/README.md`). A build without both never asks anyone anything.
//!
//! Only a copy that owns its own files can replace them: the AppImage, and
//! the Windows installer's. A package is updated by the package manager, a
//! source build by building again.

use tauri::AppHandle;

/// Whether this build was told where its updates come from.
fn configured(app: &AppHandle) -> bool {
    cfg!(feature = "updater") && app.config().plugins.0.contains_key("updater")
}

/// Whether this copy can replace itself.
fn self_contained() -> bool {
    cfg!(windows) || std::env::var_os("APPIMAGE").is_some()
}

/// Whether Heyo can look for a newer version of itself.
pub fn available(app: &AppHandle) -> bool {
    configured(app) && self_contained()
}

/// Makes the updater ready, when this build has one. It does nothing by
/// itself: Heyo looks for a new version only when asked to.
pub fn start(app: &AppHandle) {
    #[cfg(feature = "updater")]
    if available(app) {
        if let Err(e) = app.plugin(tauri_plugin_updater::Builder::new().build()) {
            eprintln!("heyo: the updater could not start: {e}");
        }
    }
    #[cfg(not(feature = "updater"))]
    let _ = app;
}

#[cfg(feature = "updater")]
async fn newer(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    use tauri_plugin_updater::UpdaterExt;
    if !available(app) {
        return Err("This copy of Heyo does not update itself.".into());
    }
    app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| format!("Could not look for a new version: {e}"))
}

/// The version that can be installed, if there is one newer than this.
#[tauri::command]
pub(crate) async fn check_for_update(app: AppHandle) -> Result<Option<String>, String> {
    #[cfg(feature = "updater")]
    return Ok(newer(&app).await?.map(|update| update.version));
    #[cfg(not(feature = "updater"))]
    {
        let _ = app;
        Err("This copy of Heyo does not update itself.".into())
    }
}

/// Downloads the newer version, checks its signature, installs it and
/// starts Heyo again.
#[tauri::command]
pub(crate) async fn install_update(app: AppHandle) -> Result<(), String> {
    #[cfg(feature = "updater")]
    {
        let Some(update) = newer(&app).await? else { return Ok(()) };
        update.download_and_install(|_, _| {}, || {}).await.map_err(|e| format!("Could not install Heyo {}: {e}", update.version))?;
        app.restart();
    }
    #[cfg(not(feature = "updater"))]
    {
        let _ = app;
        Err("This copy of Heyo does not update itself.".into())
    }
}
