//! Node.js helper processes: the extension runtime, dictation and text
//! recognition (`runtime/` in the repo). Each speaks JSON, one message a
//! line, over its stdin and stdout; what it writes to stderr joins Heyo's.
//!
//! This is the one place that finds the scripts, starts them, frames their
//! messages and notices them ending.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

/// A path as programs expect it. Windows hands out paths in a long form
/// (`\\?\C:\…`) that Node.js, for one, cannot open.
fn plain(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
}

/// Where a bundled script lives: the repository in debug builds, the
/// resource directory otherwise.
fn script(app: &AppHandle, file: &str) -> Result<PathBuf, String> {
    if cfg!(debug_assertions) {
        return Ok(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../runtime/dist")).join(file));
    }
    app.path().resource_dir().map(|dir| plain(dir.join(file))).map_err(|e| e.to_string())
}

/// What the first line of a script says it was built for: the build puts
/// `// heyo-helper <version>` there (`runtime/build.mjs`).
fn built_for(first_line: &str) -> Option<&str> {
    first_line.trim_end().strip_prefix("// heyo-helper ").filter(|version| !version.is_empty())
}

/// Why a script may not run under this Heyo: only one built together with
/// it may. `first_line` is `None` when the script could not be read.
fn refusal(script: &Path, first_line: Option<&str>, heyo: &str) -> Option<String> {
    let name = script.file_name().unwrap_or_default().to_string_lossy();
    let place = script.parent().unwrap_or(script).display();
    let fix = "Install Heyo again so that the program and its helper scripts come from the same build \
               (from source: scripts/install.sh).";
    let Some(line) = first_line else {
        return Some(format!("Heyo {heyo} cannot find its helper script {name} in {place}. {fix}"));
    };
    let theirs = match built_for(line) {
        Some(version) if version == heyo => return None,
        Some(version) => format!("Heyo {version}"),
        None => "an older Heyo that did not number its scripts".to_owned(),
    };
    Some(format!("This is Heyo {heyo}, but {name} in {place} is from {theirs}. {fix}"))
}

/// Refuses a script that did not come with this Heyo: a stale one beside a
/// new program otherwise fails later, in ways that explain nothing.
fn check(script: &Path, heyo: &str) -> Result<(), String> {
    let mut line = Vec::new();
    // The line is short; a file that is something else is not read through.
    let read = File::open(script).and_then(|file| BufReader::new(file.take(128)).read_until(b'\n', &mut line));
    let line = String::from_utf8_lossy(&line);
    match refusal(script, read.is_ok().then_some(&*line), heyo) {
        Some(why) => Err(why),
        None => Ok(()),
    }
}

/// Node.js running one of Heyo's scripts. `replaced_by` names an environment
/// variable that, when set, gives another script to run in its place.
pub fn node(app: &AppHandle, file: &str, replaced_by: &str) -> Result<Command, String> {
    let script = match std::env::var_os(replaced_by) {
        // Someone trying a script of their own: theirs to keep in step.
        Some(path) => path.into(),
        None => {
            let script = script(app, file)?;
            check(&script, &app.package_info().version.to_string())?;
            script
        }
    };
    let node = app.state::<crate::SettingsState>().0.read().unwrap().node();
    let mut command = crate::platform::common::quiet(node);
    command.arg("--no-deprecation").arg(script);
    Ok(command)
}

/// Node.js running one of Heyo's scripts for a `heyo …` invocation that is
/// not the application: no window, no running Heyo, the terminal's own
/// input and output (the developer tools, `cli::developer_tools`).
pub fn node_in_terminal(file: &str, replaced_by: &str) -> Result<Command, String> {
    let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).map_err(|e| e.to_string())?;
    let (name, version) = (config["productName"].as_str().unwrap_or_default(), config["version"].as_str().unwrap_or_default());
    let script = match std::env::var_os(replaced_by) {
        Some(path) => PathBuf::from(path),
        None => {
            let script = if cfg!(debug_assertions) {
                PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../runtime/dist")).join(file)
            } else {
                // Where the application finds its resources, asked the same way.
                let package = tauri::utils::PackageInfo {
                    name: name.to_owned(),
                    version: version.parse().map_err(|e| format!("{version}: {e}"))?,
                    authors: "",
                    description: "",
                    crate_name: env!("CARGO_PKG_NAME"),
                };
                let dir = tauri::utils::platform::resource_dir(&package, &tauri::Env::default()).map_err(|e| e.to_string())?;
                plain(dir.join(file))
            };
            check(&script, version)?;
            script
        }
    };
    // Not `quiet`: this one is meant to write to the console it was started from.
    let mut command = Command::new(crate::settings::Settings::load().node());
    command.arg("--no-deprecation").arg(script);
    Ok(command)
}

/// The Node.js the settings name, for saying which one could not be started.
pub fn node_name(app: &AppHandle) -> String {
    app.state::<crate::SettingsState>().0.read().unwrap().node()
}

/// The way to talk to a running helper. Dropping it closes the helper's
/// stdin, which ends it.
pub struct Helper {
    stdin: ChildStdin,
    child: std::sync::Arc<std::sync::Mutex<std::process::Child>>,
}

impl Helper {
    /// Starts `command`. Each message it writes goes to `on_message`, on a
    /// thread of its own; `on_exit` runs there once it has ended.
    pub fn start(
        mut command: Command,
        mut on_message: impl FnMut(Value) + Send + 'static,
        on_exit: impl FnOnce() + Send + 'static,
    ) -> std::io::Result<Helper> {
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let child = std::sync::Arc::new(std::sync::Mutex::new(child));
        let waited = child.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                match serde_json::from_str::<Value>(&line) {
                    Ok(message) => on_message(message),
                    Err(e) => eprintln!("heyo: a helper wrote something that is not a message ({e}): {line}"),
                }
            }
            let status = waited.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).wait();
            if !status.as_ref().is_ok_and(|s| s.success()) {
                eprintln!("heyo: a helper ended: {status:?}");
            }
            on_exit();
        });
        Ok(Helper { stdin, child })
    }

    pub fn send(&mut self, message: &Value) -> std::io::Result<()> {
        self.send_line(&message.to_string())
    }

    /// Sends a message that is already written out: one line of JSON.
    pub fn send_line(&mut self, message: &str) -> std::io::Result<()> {
        let mut line = String::with_capacity(message.len() + 1);
        line.push_str(message);
        line.push('\n');
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()
    }

    /// Ends a helper that no longer listens. (Closing its stdin asks it to
    /// end; this does not ask.)
    pub fn kill(&self) {
        let _ = self.child.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).kill();
    }
}

/// Runs `command` to its end and returns the messages it wrote.
pub fn run(mut command: Command) -> std::io::Result<Vec<Value>> {
    let output = command.stdin(Stdio::null()).stderr(Stdio::inherit()).output()?;
    Ok(String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| serde_json::from_str(line).ok()).collect())
}

/// Whether a message ends an installation: `installed` or `error`.
pub fn concludes(message: &Value) -> bool {
    matches!(message["type"].as_str(), Some("installed" | "error"))
}

/// Runs an installer in the background, passing its progress to `report`.
/// Whatever happens, the last thing reported is `installed` or `error`.
pub fn install(command: Result<Command, String>, report: impl Fn(Value) + Send + Sync + 'static) {
    let report = std::sync::Arc::new(report);
    let failed = |message: String| json!({ "type": "error", "message": message });
    let concluded = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let started = command.and_then(|command| {
        let (report, concluded) = (report.clone(), concluded.clone());
        let (at_exit, done) = (report.clone(), concluded.clone());
        Helper::start(
            command,
            move |message| {
                // Nothing follows the conclusion, whatever the installer says.
                if !concluded.fetch_or(concludes(&message), std::sync::atomic::Ordering::Relaxed) {
                    report(message);
                }
            },
            move || {
                if !done.load(std::sync::atomic::Ordering::Relaxed) {
                    at_exit(json!({ "type": "error", "message": "The download stopped unexpectedly" }));
                }
            },
        )
        .map_err(|e| format!("failed to start the download: {e}"))
    });
    if let Err(e) = started {
        report(failed(e));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn sh(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn messages_go_both_ways_a_line_each() {
        let (tx, rx) = mpsc::channel();
        let exited = tx.clone();
        let mut helper = Helper::start(
            sh("cat"),
            move |message| tx.send(Some(message)).unwrap(),
            move || exited.send(None).unwrap(),
        )
        .unwrap();
        helper.send(&json!({ "cmd": "start", "text": "two\nlines" })).unwrap();
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), Some(json!({ "cmd": "start", "text": "two\nlines" })));
        // Closing its stdin ends it.
        drop(helper);
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), None);
    }

    #[test]
    fn a_helper_that_does_not_listen_can_be_ended() {
        let (tx, rx) = mpsc::channel();
        // It never reads its stdin, so closing that would not end it.
        let helper = Helper::start(sh("exec sleep 600"), |_| {}, move || tx.send(()).unwrap()).unwrap();
        helper.kill();
        assert_eq!(rx.recv_timeout(WAIT), Ok(()));
    }

    #[test]
    fn what_is_not_a_message_is_passed_over() {
        let messages = run(sh(r#"echo warming up; echo '{"text":"hi"}'; echo"#)).unwrap();
        assert_eq!(messages, vec![json!({ "text": "hi" })]);
    }

    #[test]
    fn a_helper_that_cannot_start_says_so() {
        assert!(Helper::start(Command::new("/nonexistent/heyo-helper"), |_| {}, || {}).is_err());
    }

    #[test]
    fn only_a_script_built_with_this_heyo_may_run() {
        let script = Path::new("/usr/lib/Heyo/heyo-runtime.cjs");
        assert_eq!(built_for("// heyo-helper 0.2.0\n"), Some("0.2.0"));
        assert_eq!(built_for("\"use strict\";"), None);
        assert_eq!(refusal(script, Some("// heyo-helper 0.2.0\n"), "0.2.0"), None);
        let stale = refusal(script, Some("// heyo-helper 0.1.0\n"), "0.2.0").unwrap();
        assert!(stale.contains("This is Heyo 0.2.0") && stale.contains("heyo-runtime.cjs in /usr/lib/Heyo is from Heyo 0.1.0"), "{stale}");
        assert!(stale.contains("scripts/install.sh"));
        // Close is not the same.
        assert!(refusal(script, Some("// heyo-helper 0.2.0-beta.1"), "0.2.0").is_some());
        // A script from before they were numbered.
        assert!(refusal(script, Some("\"use strict\";\n"), "0.2.0").unwrap().contains("older Heyo"));
        assert!(refusal(script, None, "0.2.0").unwrap().contains("cannot find"));
    }

    #[test]
    fn the_version_is_read_from_the_script_itself() {
        let dir = std::env::temp_dir().join(format!("heyo-helper-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("heyo-ocr.cjs");
        assert!(check(&script, "0.2.0").unwrap_err().contains("cannot find"));
        std::fs::write(&script, "// heyo-helper 0.2.0\n\"use strict\";\n").unwrap();
        assert_eq!(check(&script, "0.2.0"), Ok(()));
        assert!(check(&script, "0.3.0").unwrap_err().contains("is from Heyo 0.2.0"));
        // Not a script at all, and no line to speak of.
        std::fs::write(&script, vec![0xffu8; 4096]).unwrap();
        assert!(check(&script, "0.2.0").unwrap_err().contains("older Heyo"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What the build really writes, when it has run.
    #[test]
    fn built_scripts_carry_this_version() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let version = config["version"].as_str().unwrap();
        for file in ["heyo-runtime.cjs", "heyo-dictation.cjs", "heyo-ocr.cjs", "heyo-dev.cjs", "heyo-embed.cjs"] {
            let script = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../runtime/dist")).join(file);
            if script.exists() {
                assert_eq!(check(&script, version), Ok(()), "{file}: run pnpm build:runtime");
            }
        }
    }

    fn installing(script: &str) -> Vec<Value> {
        let (tx, rx) = mpsc::channel();
        install(Ok(sh(script)), move |message| tx.send(message).unwrap());
        let mut seen = Vec::new();
        while let Ok(message) = rx.recv_timeout(WAIT) {
            let last = concludes(&message);
            seen.push(message);
            if last {
                break;
            }
        }
        // Nothing may follow the conclusion.
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
        seen
    }

    #[test]
    fn an_installation_always_concludes() {
        let done = installing(r#"echo '{"type":"progress","step":"a"}'; echo '{"type":"installed"}'"#);
        assert_eq!(done.len(), 2);
        assert_eq!(done[1]["type"], "installed");

        let died = installing(r#"echo '{"type":"progress","step":"a"}'; exit 3"#);
        assert_eq!(died.last().unwrap()["type"], "error");

        let chatty = installing(r#"echo '{"type":"error","message":"no disk"}'; echo '{"type":"progress"}'"#);
        assert_eq!(chatty, vec![json!({ "type": "error", "message": "no disk" })]);

        let (tx, rx) = mpsc::channel();
        install(Err("no Node.js".into()), move |message| tx.send(message).unwrap());
        assert_eq!(rx.recv_timeout(WAIT).unwrap(), json!({ "type": "error", "message": "no Node.js" }));
    }
}
