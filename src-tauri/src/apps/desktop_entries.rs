//! Applications from freedesktop `.desktop` entries.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use walkdir::WalkDir;

use super::App;

/// Data directories in precedence order (`$XDG_DATA_HOME` first).
fn data_dirs() -> Vec<PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut dirs = vec![env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))];

    let system = env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(system.split(':').map(PathBuf::from));

    // Flatpak and Snap exports are missing from XDG_DATA_DIRS when Heyo is
    // started outside a desktop session (e.g. from an SSH shell).
    for extra in [
        home.join(".local/share/flatpak/exports/share"),
        PathBuf::from("/var/lib/flatpak/exports/share"),
        PathBuf::from("/var/lib/snapd/desktop"),
    ] {
        if !dirs.contains(&extra) {
            dirs.push(extra);
        }
    }
    dirs
}

/// A fingerprint of the installed applications: the names and modification
/// times of the desktop files. Cheap to compute, and different whenever an
/// application was installed, removed or changed.
pub fn fingerprint() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for dir in data_dirs() {
        let root = dir.join("applications");
        for entry in walkdir::WalkDir::new(&root).max_depth(3).follow_links(true).into_iter().flatten() {
            if entry.path().extension().is_some_and(|e| e == "desktop") {
                entry.path().hash(&mut hasher);
                if let Some(modified) = entry.metadata().ok().and_then(|m| m.modified().ok()) {
                    modified.hash(&mut hasher);
                }
            }
        }
    }
    hasher.finish()
}

pub fn discover() -> Vec<App> {
    let desktops: Vec<String> = env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let icon_theme = freedesktop_icons::default_theme_gtk();

    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in data_dirs() {
        let root = dir.join("applications");
        for entry in WalkDir::new(&root).follow_links(true).into_iter().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "desktop") {
                continue;
            }
            let Ok(rel) = path.strip_prefix(&root) else { continue };
            let id = rel.to_string_lossy().replace('/', "-");
            // A higher-precedence directory may shadow (or hide) this entry.
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(app) = parse(&id, path, &desktops, icon_theme.as_deref()) {
                apps.push(app);
            }
        }
    }
    apps.sort_by_cached_key(|a| a.name.to_lowercase());
    apps
}

fn parse(id: &str, path: &Path, desktops: &[String], icon_theme: Option<&str>) -> Option<App> {
    let text = fs::read_to_string(path).ok()?;
    let entry = parse_group(&text, "Desktop Entry");
    let locales = locale_variants();
    let get = |key: &str| entry.get(key).map(String::as_str);
    let localized = |key: &str| {
        locales
            .iter()
            .find_map(|l| entry.get(&format!("{key}[{l}]")))
            .or_else(|| entry.get(key))
            .cloned()
    };
    let is_true = |key: &str| get(key) == Some("true");

    if get("Type") != Some("Application") || is_true("NoDisplay") || is_true("Hidden") {
        return None;
    }
    let shown_in = |key: &str| {
        get(key).map(|v| v.split(';').any(|d| !d.is_empty() && desktops.iter().any(|c| c == d)))
    };
    // With no known desktop (e.g. started from an SSH shell), show everything.
    let only_elsewhere = !desktops.is_empty() && shown_in("OnlyShowIn") == Some(false);
    if only_elsewhere || shown_in("NotShowIn") == Some(true) {
        return None;
    }
    if let Some(try_exec) = get("TryExec") {
        find_executable(try_exec)?;
    }

    Some(App {
        id: id.to_owned(),
        name: localized("Name")?,
        generic_name: localized("GenericName"),
        comment: localized("Comment"),
        keywords: localized("Keywords")
            .map(|k| k.split(';').filter(|s| !s.is_empty()).map(str::to_owned).collect())
            .unwrap_or_default(),
        exec: get("Exec")?.to_owned(),
        icon: get("Icon").and_then(|name| resolve_icon(name, icon_theme)),
        terminal: is_true("Terminal"),
        working_dir: get("Path").filter(|p| !p.is_empty()).map(PathBuf::from),
        startup_wm_class: get("StartupWMClass").filter(|c| !c.is_empty()).map(str::to_owned),
        path: path.to_owned(),
    })
}

/// Parses `key=value` pairs from one `[group]` of a desktop file.
fn parse_group(text: &str, group: &str) -> HashMap<String, String> {
    let mut in_group = false;
    let mut map = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_group = line == format!("[{group}]");
        } else if in_group && !line.starts_with('#') {
            if let Some((k, v)) = line.split_once('=') {
                map.entry(k.trim().to_owned()).or_insert_with(|| unescape(v.trim()));
            }
        }
    }
    map
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            // Keep other escapes (e.g. `\;`, `\"`) for later stages.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// `zh_CN.UTF-8@foo` -> `["zh_CN@foo", "zh_CN", "zh@foo", "zh"]`, per the spec.
fn locale_variants() -> Vec<String> {
    let raw = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|k| env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default();
    let (base, modifier) = match raw.split_once('@') {
        Some((b, m)) => (b, Some(m)),
        None => (raw.as_str(), None),
    };
    let base = base.split('.').next().unwrap_or_default();
    let lang = base.split('_').next().unwrap_or_default();
    if lang.is_empty() || lang == "C" || lang == "POSIX" {
        return Vec::new();
    }

    let mut out = Vec::new();
    for b in [base, lang] {
        if let Some(m) = modifier {
            out.push(format!("{b}@{m}"));
        }
        out.push(b.to_owned());
    }
    out.dedup();
    out
}

fn resolve_icon(name: &str, theme: Option<&str>) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() {
        return path.exists().then(|| path.to_owned());
    }
    let mut lookup = freedesktop_icons::lookup(name).with_size(64).with_cache();
    if let Some(theme) = theme {
        lookup = lookup.with_theme(theme);
    }
    lookup
        .find()
        .or_else(|| freedesktop_icons::lookup(name).with_size(64).with_cache().find())
        .or_else(|| {
            ["png", "svg", "xpm"]
                .iter()
                .map(|ext| PathBuf::from(format!("/usr/share/pixmaps/{name}.{ext}")))
                .find(|p| p.exists())
        })
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_owned());
    }
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// Expands the `Exec` key into an argv, dropping file/URL field codes.
fn expand_exec(app: &App) -> Option<Vec<String>> {
    let mut argv = Vec::new();
    for token in shlex::split(&app.exec)? {
        match token.as_str() {
            "%f" | "%F" | "%u" | "%U" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {}
            "%i" => {
                if let Some(icon) = &app.icon {
                    argv.push("--icon".into());
                    argv.push(icon.to_string_lossy().into_owned());
                }
            }
            _ => {
                let mut out = String::new();
                let mut chars = token.chars();
                while let Some(c) = chars.next() {
                    if c != '%' {
                        out.push(c);
                        continue;
                    }
                    match chars.next() {
                        Some('%') => out.push('%'),
                        Some('c') => out.push_str(&app.name),
                        Some('k') => out.push_str(&app.path.to_string_lossy()),
                        _ => {}
                    }
                }
                argv.push(out);
            }
        }
    }
    (!argv.is_empty()).then_some(argv)
}

fn terminal_prefix() -> Vec<String> {
    if let Ok(term) = env::var("TERMINAL") {
        if find_executable(&term).is_some() {
            return vec![term, "-e".into()];
        }
    }
    for (bin, flag) in [
        ("x-terminal-emulator", "-e"),
        ("gnome-terminal", "--"),
        ("kgx", "--"),
        ("konsole", "-e"),
        ("alacritty", "-e"),
        ("kitty", "--"),
        ("foot", "--"),
        ("xterm", "-e"),
    ] {
        if find_executable(bin).is_some() {
            return vec![bin.into(), flag.into()];
        }
    }
    Vec::new()
}

pub fn launch(app: &App) -> Result<(), String> {
    let mut argv = expand_exec(app).ok_or_else(|| format!("invalid Exec line: {}", app.exec))?;
    if app.terminal {
        let mut prefix = terminal_prefix();
        if prefix.is_empty() {
            return Err("no terminal emulator found".into());
        }
        prefix.append(&mut argv);
        argv = prefix;
    }

    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Detach from Heyo's process group so the app outlives us.
        .process_group(0);
    if let Some(dir) = &app.working_dir {
        cmd.current_dir(dir);
    }
    let mut child = cmd.spawn().map_err(|e| format!("failed to launch {}: {e}", argv[0]))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(exec: &str) -> App {
        App {
            id: "t.desktop".into(),
            name: "Test".into(),
            generic_name: None,
            comment: None,
            keywords: vec![],
            exec: exec.into(),
            icon: None,
            terminal: false,
            working_dir: None,
            startup_wm_class: None,
            path: "/x/t.desktop".into(),
        }
    }

    #[test]
    fn exec_field_codes() {
        assert_eq!(expand_exec(&app("firefox %u")).unwrap(), ["firefox"]);
        assert_eq!(
            expand_exec(&app(r#"sh -c "echo 100%% %c" %F"#)).unwrap(),
            ["sh", "-c", "echo 100% Test"]
        );
    }

    #[test]
    fn group_parsing() {
        let text = "[Desktop Entry]\nName=A\\sB\n[Desktop Action new]\nName=Other\n";
        assert_eq!(parse_group(text, "Desktop Entry")["Name"], "A B");
    }

    #[test]
    fn window_classes_come_from_the_desktop_file_and_the_program() {
        let mut firefox = app("env MOZ_ENABLE_WAYLAND=1 /usr/bin/firefox %u");
        firefox.id = "org.mozilla.firefox.desktop".into();
        firefox.startup_wm_class = Some("Firefox".into());
        assert_eq!(firefox.window_classes(), ["firefox", "org.mozilla.firefox"]);

        // A shared launcher does not identify the application.
        let mut snap = app("snap run spotify");
        snap.id = "spotify_spotify.desktop".into();
        assert_eq!(snap.window_classes(), ["spotify_spotify"]);

        let mut top = app("htop");
        top.terminal = true;
        assert!(top.window_classes().is_empty());
    }
}
