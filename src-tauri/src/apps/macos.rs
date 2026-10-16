//! Applications installed as macOS `.app` bundles.

use std::collections::hash_map::DefaultHasher;
use std::env;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use plist::Value;
use walkdir::{DirEntry, WalkDir};

use super::App;

fn roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    if let Some(home) = env::var_os("HOME") {
        roots.insert(0, PathBuf::from(home).join("Applications"));
    }
    roots
}

fn enter(entry: &DirEntry) -> bool {
    // Yield the bundle itself, then do not walk its Contents: helper apps
    // inside another application's bundle are not installed applications.
    entry.depth() == 0
        || entry.path().parent().is_none_or(|parent| {
            parent.extension().is_none_or(|extension| extension != "app")
        })
}

fn bundles() -> Vec<PathBuf> {
    roots()
        .into_iter()
        .flat_map(|root| {
            WalkDir::new(root)
                .min_depth(1)
                .max_depth(4)
                .follow_links(true)
                .into_iter()
                .filter_entry(enter)
                .flatten()
                .filter(|entry| {
                    entry.file_type().is_dir()
                        && entry
                            .path()
                            .extension()
                            .is_some_and(|extension| extension == "app")
                })
                .map(DirEntry::into_path)
        })
        .collect()
}

fn string<'a>(dictionary: &'a plist::Dictionary, key: &str) -> Option<&'a str> {
    dictionary
        .get(key)
        .and_then(Value::as_string)
        .filter(|value| !value.is_empty())
}

fn icon(resources: &Path, named: Option<&str>) -> Option<PathBuf> {
    let named = named?;
    let path = resources.join(named);
    if path.is_file() {
        return Some(path);
    }
    (Path::new(named).extension().is_none())
        .then(|| resources.join(format!("{named}.icns")))
        .filter(|path| path.is_file())
}

fn webview_icon(source: PathBuf) -> Option<PathBuf> {
    if !source.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("icns")) {
        return Some(source);
    }

    // WebKit does not display an ICNS file in an <img>. Keep a small PNG in
    // Heyo's recreatable cache, keyed by the source and its modification time.
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    std::fs::metadata(&source)
        .and_then(|metadata| metadata.modified())
        .ok()
        .hash(&mut hasher);
    let directory = crate::paths::cache().join("app-icons");
    let converted = directory.join(format!("{:016x}.png", hasher.finish()));
    if converted.is_file() {
        return Some(converted);
    }
    std::fs::create_dir_all(&directory).ok()?;
    Command::new("sips")
        .args(["-s", "format", "png", "-Z", "128"])
        .arg(&source)
        .arg("--out")
        .arg(&converted)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .filter(|status| status.success())?;
    converted.is_file().then_some(converted)
}

fn application(path: PathBuf) -> Option<App> {
    let info = Value::from_file(path.join("Contents/Info.plist")).ok()?;
    let info = info.as_dictionary()?;
    let name = string(info, "CFBundleDisplayName")
        .or_else(|| string(info, "CFBundleName"))
        .map(str::to_owned)
        .or_else(|| {
            path.file_stem()
                .map(|name| name.to_string_lossy().into_owned())
        })?;
    let identifier = string(info, "CFBundleIdentifier")
        .unwrap_or(&name)
        .to_owned();
    let executable = string(info, "CFBundleExecutable")?;
    let resources = path.join("Contents/Resources");
    Some(App {
        id: format!("{identifier}.app"),
        name,
        generic_name: None,
        comment: None,
        keywords: Vec::new(),
        exec: path
            .join("Contents/MacOS")
            .join(executable)
            .to_string_lossy()
            .into_owned(),
        icon: icon(&resources, string(info, "CFBundleIconFile")).and_then(webview_icon),
        terminal: false,
        working_dir: None,
        startup_wm_class: Some(identifier),
        path,
    })
}

pub fn fingerprint() -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut paths = bundles();
    paths.sort();
    for path in paths {
        path.hash(&mut hasher);
        std::fs::metadata(path.join("Contents/Info.plist"))
            .and_then(|metadata| metadata.modified())
            .ok()
            .hash(&mut hasher);
    }
    hasher.finish()
}

pub fn discover() -> Vec<App> {
    let mut apps: Vec<App> = bundles().into_iter().filter_map(application).collect();
    apps.sort_by_cached_key(|app| app.name.to_lowercase());
    apps.dedup_by(|a, b| a.id == b.id);
    apps
}

pub fn launch(app: &App) -> Result<(), String> {
    let mut child = Command::new("open")
        .arg("-a")
        .arg(&app.path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("failed to launch {}: {error}", app.name))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_icon_name_without_an_extension_finds_the_icns_file() {
        let dir = env::temp_dir().join(format!("heyo-macos-icon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("AppIcon.icns"), b"icon").unwrap();
        assert_eq!(icon(&dir, Some("AppIcon")), Some(dir.join("AppIcon.icns")));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
