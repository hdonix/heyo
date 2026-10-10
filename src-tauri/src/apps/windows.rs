//! Applications on Windows: the shortcuts under the Start menu's
//! `Programs` folders (the user's and everybody's), and the applications
//! from the Store, which have no shortcut files.
//!
//! A shortcut is read for the program it starts (whose name is how the
//! application's windows are found when it is already running) and for
//! its icon, which is kept as a PNG file in Heyo's cache.
//!
//! The Store's applications are known only to the shell. PowerShell lists
//! them (`Get-StartApps`), which takes a second or two: so the list is
//! kept in a file, read from there, and brought up to date in the
//! background, when Heyo starts and every half hour. When it has changed,
//! `fingerprint` changes with it and the applications are read again.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::env;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use walkdir::WalkDir;

use super::shortcut::{self, Shortcut};
use super::App;
use crate::platform::win::icons::{self, Source};

/// What an application from the Store is started by, before its id.
const PACKAGED: &str = r"shell:AppsFolder\";

/// The Store's applications are looked up again this often: installing
/// one changes no folder that could be watched.
const LOOK_AGAIN: Duration = Duration::from_secs(30 * 60);

fn start_menus() -> Vec<PathBuf> {
    ["APPDATA", "PROGRAMDATA"].into_iter().filter_map(env::var_os).map(|base| PathBuf::from(base).join("Microsoft/Windows/Start Menu/Programs")).collect()
}

fn shortcuts() -> Vec<PathBuf> {
    start_menus()
        .iter()
        .flat_map(|dir| WalkDir::new(dir).follow_links(true).into_iter().flatten())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")))
        .collect()
}

/// The applications' icons, as PNG files named by what they were made from.
pub fn icon_dir() -> PathBuf {
    let dir = crate::paths::cache().join("icons");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// The list of the Store's applications as PowerShell last printed it.
fn packaged_list() -> PathBuf {
    crate::paths::cache().join("store-apps.json")
}

/// Asks PowerShell for the Store's applications, unless it is being asked
/// already, and keeps the answer if it is a new one.
fn look_for_packaged() {
    static LOOKING: AtomicBool = AtomicBool::new(false);
    if LOOKING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        // UTF-8, whatever the console's encoding: names are not all ASCII.
        const SCRIPT: &str = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; Get-StartApps | ConvertTo-Json -Compress";
        let output = crate::platform::common::quiet("powershell").args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]).output();
        match output {
            Ok(output) if output.status.success() => {
                let list = packaged_list();
                // Only an answer that can be read, and only one that says
                // something new (in whatever order): the file changing is
                // what has the applications read again.
                let sorted = |text: &str| {
                    let mut listed = shortcut::packaged(text);
                    listed.sort();
                    listed
                };
                let found = sorted(&String::from_utf8_lossy(&output.stdout));
                if !found.is_empty() && found != sorted(&std::fs::read_to_string(&list).unwrap_or_default()) {
                    let _ = std::fs::create_dir_all(crate::paths::cache());
                    if let Err(e) = std::fs::write(&list, &output.stdout) {
                        eprintln!("heyo: could not keep the list of Store applications: {e}");
                    }
                }
            }
            Ok(output) => eprintln!("heyo: PowerShell did not list the Store applications: {}", String::from_utf8_lossy(&output.stderr).trim()),
            Err(e) => eprintln!("heyo: could not run PowerShell to list the Store applications: {e}"),
        }
        LOOKING.store(false, Ordering::SeqCst);
    });
}

/// Changes when a shortcut appears or goes, or the Store's applications
/// change. Asked every few seconds, it is also what has those looked up
/// again now and then.
pub fn fingerprint() -> u64 {
    static LOOKED: Mutex<Option<Instant>> = Mutex::new(None);
    let due = {
        let mut looked = LOOKED.lock().unwrap();
        let due = looked.is_none_or(|at| at.elapsed() >= LOOK_AGAIN);
        if due {
            *looked = Some(Instant::now());
        }
        due
    };
    if due {
        look_for_packaged();
    }
    let mut hasher = DefaultHasher::new();
    let mut paths = shortcuts();
    paths.sort();
    paths.hash(&mut hasher);
    std::fs::read(packaged_list()).unwrap_or_default().hash(&mut hasher);
    hasher.finish()
}

/// Where the icon made from `source` is kept. A shortcut that changes
/// (and so may its icon) gets a new name.
fn icon_file(dir: &Path, source: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    if let Ok(metadata) = std::fs::metadata(source) {
        metadata.len().hash(&mut hasher);
        metadata.modified().ok().hash(&mut hasher);
    }
    dir.join(format!("{:016x}.png", hasher.finish()))
}

/// The icon at `file`, made now from the first of `sources` that gives
/// one unless it is there already.
fn icon(file: PathBuf, sources: &[Source]) -> Option<PathBuf> {
    if file.is_file() {
        return Some(file);
    }
    icons::save(sources, &file).ok().map(|()| file)
}

fn variable(name: &str) -> Option<String> {
    env::var(name).ok()
}

/// The application a shortcut file stands for.
fn from_shortcut(path: PathBuf, name: String, icons: &Path) -> App {
    let said = std::fs::read(&path).ok().and_then(|bytes| shortcut::parse(&bytes)).unwrap_or_default();
    let Shortcut { target, arguments, description, icon_file: icon_source, icon_index } = said;
    let target = target.map(|target| shortcut::expand(&target, variable));
    let link = path.to_string_lossy().into_owned();
    // The icon the shortcut names, else its program's own, else whatever
    // the shell shows for the program or (with an arrow on it) the shortcut.
    let mut sources: Vec<Source> = Vec::new();
    if let Some(file) = icon_source {
        sources.push(Source::Inside(shortcut::expand(&file, variable), icon_index));
    }
    if let Some(target) = &target {
        sources.push(Source::Inside(target.clone(), 0));
        sources.push(Source::Shell(target.clone()));
    }
    sources.push(Source::Shell(link.clone()));
    App {
        id: format!("{}.lnk", name.to_lowercase()),
        generic_name: None,
        // Not the ones that only name a text inside some library.
        comment: description.filter(|text| !text.starts_with('@')),
        keywords: Vec::new(),
        exec: link,
        icon: icon(icon_file(icons, &path), &sources),
        terminal: false,
        working_dir: None,
        startup_wm_class: target.as_deref().and_then(|target| shortcut::window_program(target, arguments.as_deref())),
        name,
        path,
    }
}

/// An application from the Store.
fn from_package(name: String, id: String, icons: &Path) -> App {
    let item = format!("{PACKAGED}{id}");
    // Nothing on disk says when its icon changes; its id has to do.
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    App {
        icon: icon(icons.join(format!("{:016x}.png", hasher.finish())), &[Source::Item(item.clone())]),
        id,
        name,
        generic_name: None,
        comment: None,
        keywords: Vec::new(),
        exec: item.clone(),
        terminal: false,
        working_dir: None,
        path: PathBuf::from(item),
        // Its windows belong to a host all such applications share.
        startup_wm_class: None,
    }
}

pub fn discover() -> Vec<App> {
    // One at a time: two would write the same icon files.
    static DISCOVERING: Mutex<()> = Mutex::new(());
    let _one = DISCOVERING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let icons = icon_dir();
    // The shell's ways of finding icons want COM.
    let _com = crate::platform::win::Com::begin(true);
    let mut apps: Vec<App> = Vec::new();
    for path in shortcuts() {
        let Some(name) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else { continue };
        // The user's own shortcut stands for one of the same name that everybody has.
        if shortcut::beside_the_program(&name) || apps.iter().any(|app| app.name.eq_ignore_ascii_case(&name)) {
            continue;
        }
        apps.push(from_shortcut(path, name, &icons));
    }
    let listed = std::fs::read_to_string(packaged_list()).unwrap_or_default();
    for (name, id) in shortcut::packaged(&listed) {
        if !apps.iter().any(|app| app.name.eq_ignore_ascii_case(&name)) {
            apps.push(from_package(name, id, &icons));
        }
    }
    // Icons of what is no longer installed, or was changed.
    let kept: HashSet<&Path> = apps.iter().filter_map(|app| app.icon.as_deref()).collect();
    for entry in std::fs::read_dir(&icons).into_iter().flatten().flatten() {
        if !kept.contains(entry.path().as_path()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

/// Opens the shortcut, as the Start menu would.
pub fn launch(app: &App) -> Result<(), String> {
    if app.exec.starts_with(PACKAGED) {
        // Explorer knows what to do with such a name; what it returns says nothing.
        let mut child = crate::platform::common::quiet("explorer.exe").arg(&app.exec).spawn().map_err(|e| format!("failed to open {}: {e}", app.name))?;
        std::thread::spawn(move || child.wait());
        return Ok(());
    }
    crate::platform::common::open(&app.path)
}
