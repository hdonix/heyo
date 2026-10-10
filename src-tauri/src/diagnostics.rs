//! What is left to look at when something goes wrong: Heyo's log, what a
//! panic writes into it, and the report "Copy Diagnostics" puts together.
//!
//! The log is `heyo.log` in the cache directory and holds what Heyo and its
//! helper programs write to standard error. On Linux standard error is
//! pointed at the file when nobody is reading it (started by the desktop);
//! started from a terminal or with it sent somewhere, everything goes
//! there and to the file both. (Windows points it at the same file in
//! `platform::win`.)

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::{Captures, Regex};

/// Past this size the log is set aside (one older copy is kept) and begun afresh.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const LARGE: u64 = 1_000_000;
/// How much of the log a report carries.
const LINES: usize = 200;

pub fn log_path() -> PathBuf {
    crate::paths::cache().join("heyo.log")
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn older(path: &Path) -> PathBuf {
    path.with_extension("log.1")
}

/// Sets a log that has grown past `limit` aside, replacing the one set
/// aside before. True if it did.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn rotate(path: &Path, limit: u64) -> bool {
    if !fs::metadata(path).is_ok_and(|m| m.len() > limit) {
        return false;
    }
    fs::rename(path, older(path)).is_ok()
}

/// The same for a log that is open and being written to: the file stays
/// where it is (its writers append, wherever the end is) and is emptied.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn trim(path: &Path, limit: u64) -> bool {
    if !fs::metadata(path).is_ok_and(|m| m.len() > limit) {
        return false;
    }
    let copied = fs::copy(path, older(path)).is_ok();
    copied && fs::OpenOptions::new().write(true).open(path).and_then(|file| file.set_len(0)).is_ok()
}

/// Opens the log for appending, readable by the user alone: what programs
/// say when they fail is not always fit for others to read.
fn open_log(path: &Path) -> std::io::Result<fs::File> {
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Where standard error pointed when Heyo started (a terminal, usually),
/// when it is being copied there, and the log: a panic is written to both
/// directly.
static TERMINAL: OnceLock<(fs::File, PathBuf)> = OnceLock::new();

/// Makes standard error the log file: instead of wherever it pointed when
/// nobody reads that, as well as there when somebody does.
#[cfg(target_os = "linux")]
fn keep_log(path: &Path) -> std::io::Result<()> {
    use std::io::{IsTerminal, Read};
    use std::os::fd::{AsRawFd, FromRawFd};

    let failed = std::io::Error::last_os_error;
    let mut log = open_log(path)?;
    // Somebody reads a terminal, and a file or a pipe was put there by
    // somebody on purpose. The rest is the desktop's: nothing, or its journal.
    let read = std::io::stderr().is_terminal() || {
        let mut status = std::mem::MaybeUninit::<libc::stat>::zeroed();
        unsafe { libc::fstat(2, status.as_mut_ptr()) == 0 && matches!(status.assume_init().st_mode & libc::S_IFMT, libc::S_IFREG | libc::S_IFIFO) }
    };
    if !read {
        // The file itself becomes standard error. Programs Heyo starts
        // inherit it, and a file is never closed under them.
        if unsafe { libc::dup2(log.as_raw_fd(), 2) } < 0 {
            return Err(failed());
        }
        return Ok(());
    }
    // A pipe becomes standard error; a thread copies what arrives to the
    // terminal and to the file.
    let mut ends = [0; 2];
    if unsafe { libc::pipe2(ends.as_mut_ptr(), libc::O_CLOEXEC) } < 0 {
        return Err(failed());
    }
    let (mut reader, writer) = unsafe { (fs::File::from_raw_fd(ends[0]), fs::File::from_raw_fd(ends[1])) };
    let terminal = unsafe { libc::fcntl(2, libc::F_DUPFD_CLOEXEC, 3) };
    if terminal < 0 {
        return Err(failed());
    }
    let mut terminal = unsafe { fs::File::from_raw_fd(terminal) };
    if let Ok(copy) = terminal.try_clone() {
        let _ = TERMINAL.set((copy, path.to_owned()));
    }
    if unsafe { libc::dup2(writer.as_raw_fd(), 2) } < 0 {
        return Err(failed());
    }
    drop(writer);
    PIPE.store(reader.as_raw_fd(), std::sync::atomic::Ordering::Relaxed);
    unsafe { libc::atexit(drain) };
    std::thread::Builder::new().name("log".into()).spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    // The terminal may be long gone; the file is what matters.
                    let _ = terminal.write_all(&buffer[..n]);
                    let _ = log.write_all(&buffer[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
    })?;
    Ok(())
}

/// The reading end of the pipe that standard error is, when it is one.
#[cfg(target_os = "linux")]
static PIPE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// As Heyo exits: waits a moment for what was said last to be copied out
/// of the pipe, or the last lines would be lost with the thread copying them.
#[cfg(target_os = "linux")]
extern "C" fn drain() {
    let pipe = PIPE.load(std::sync::atomic::Ordering::Relaxed);
    for _ in 0..100 {
        let mut waiting: libc::c_int = 0;
        if unsafe { libc::ioctl(pipe, libc::FIONREAD, &mut waiting) } != 0 || waiting == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    // What was read last is still to be written.
    std::thread::sleep(std::time::Duration::from_millis(5));
}

/// What a panic leaves in the log.
fn panic_report(message: &str, location: &str, thread: &str, backtrace: &str) -> String {
    format!("heyo: panicked at {location} (thread {thread}, version {}): {message}\n{}\n", env!("CARGO_PKG_VERSION"), backtrace.trim_end())
}

fn on_panic(info: &std::panic::PanicHookInfo) {
    let payload = info.payload();
    let message = payload.downcast_ref::<&str>().map(|s| (*s).to_owned()).or_else(|| payload.downcast_ref::<String>().cloned()).unwrap_or_else(|| "(no message)".into());
    let location = info.location().map_or_else(|| "an unknown place".to_owned(), |l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
    let thread = std::thread::current();
    write_report(&panic_report(&message, &location, thread.name().unwrap_or("unnamed"), &std::backtrace::Backtrace::force_capture().to_string()));
}

fn write_report(report: &str) {
    match TERMINAL.get() {
        // Standard error is a pipe a thread empties, and a panic may end
        // the process before it has: write to both ends directly.
        Some((terminal, log)) => {
            let _ = (&*terminal).write_all(report.as_bytes());
            if let Ok(mut log) = open_log(log) {
                let _ = log.write_all(report.as_bytes());
            }
        }
        None => drop(std::io::stderr().write_all(report.as_bytes())),
    }
}

/// Begins keeping the log, and has panics written into it. Called as
/// Heyo starts, once the cache directory is where it will stay.
pub fn start() {
    #[cfg(target_os = "linux")]
    {
        let path = log_path();
        rotate(&path, LARGE);
        match keep_log(&path) {
            Ok(()) => {
                // Heyo runs for weeks: look now and then whether the log has grown large.
                let _ = std::thread::Builder::new().name("log-size".into()).spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(1800));
                    trim(&path, LARGE);
                });
            }
            Err(e) => eprintln!("heyo: no log is kept at {}: {e}", path.display()),
        }
    }
    std::panic::set_hook(Box::new(on_panic));
}

// --- Copy Diagnostics ----------------------------------------------------------

/// The last `count` lines of a text.
fn tail(text: &str, count: usize) -> &str {
    let mut seen = 0;
    for (at, _) in text.trim_end().rmatch_indices('\n') {
        seen += 1;
        if seen == count {
            return &text[at + 1..];
        }
    }
    text
}

fn patterns() -> &'static [(Regex, &'static str)] {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let pattern = |source: &str, with| (Regex::new(source).expect("a valid pattern"), with);
        vec![
            // A password in a URL.
            pattern(r"://[^/\s:@]+:[^/\s@]+@", "://[redacted]@"),
            // Something named as a secret, whatever it looks like:
            // `token=…`, `"apiKey": "…"`, `Authorization: Bearer …`.
            pattern(
                r#"(?i)((?:api[_-]?key|token|secret|passw(?:or)?d|passphrase|authorization|credential|cookie|signature)[a-z_]*["']?\s*[:=]\s*["']?(?:bearer\s+|basic\s+)?)[^\s"',;&]+"#,
                "$1[redacted]",
            ),
            pattern(r"(?i)\b(bearer|basic)\s+[a-z0-9._~+/=-]{8,}", "$1 [redacted]"),
            // The shapes keys come in: Anthropic and OpenAI, GitHub, Slack, AWS, Google, JWTs.
            pattern(r"\b(?:sk|pk|rk)-[A-Za-z0-9_-]{8,}", "[redacted]"),
            pattern(r"\b(?:gh[pousr]_|github_pat_|glpat-|xox[abprs]-)[A-Za-z0-9_-]{8,}", "[redacted]"),
            pattern(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b", "[redacted]"),
            pattern(r"\bAIza[0-9A-Za-z_-]{30,}", "[redacted]"),
            pattern(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]*", "[redacted]"),
        ]
    })
}

/// Text with what looks like a secret taken out and the home directory
/// written as `~`, so a report can be pasted where others read it.
pub fn scrub(text: &str, home: &str) -> String {
    let mut out = match home.trim_end_matches(['/', '\\']) {
        "" => text.to_owned(),
        home => text.replace(home, "~"),
    };
    for (pattern, with) in patterns() {
        out = pattern.replace_all(&out, *with).into_owned();
    }
    // Anything else long and random-looking: letters and digits both, no
    // word in it. Paths and sentences are neither.
    static LONG: OnceLock<Regex> = OnceLock::new();
    let long = LONG.get_or_init(|| Regex::new(r"[A-Za-z0-9_+=-]{32,}").expect("a valid pattern"));
    long.replace_all(&out, |found: &Captures| {
        let token = &found[0];
        let (digits, letters) = (token.bytes().any(|b| b.is_ascii_digit()), token.bytes().any(|b| b.is_ascii_alphabetic()));
        if digits && letters { "[redacted]".to_owned() } else { token.to_owned() }
    })
    .into_owned()
}

/// The distribution's name out of `/etc/os-release`.
fn pretty_name(os_release: &str) -> Option<String> {
    let value = os_release.lines().find_map(|line| line.strip_prefix("PRETTY_NAME="))?;
    Some(value.trim().trim_matches(['"', '\'']).to_owned()).filter(|name| !name.is_empty())
}

fn system() -> String {
    let mut parts = vec![format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)];
    parts.extend(fs::read_to_string("/etc/os-release").ok().as_deref().and_then(pretty_name));
    parts.extend(fs::read_to_string("/proc/sys/kernel/osrelease").ok().map(|kernel| format!("kernel {}", kernel.trim())));
    parts.join(", ")
}

fn session() -> String {
    let variable = |name: &str| format!("{name}={}", std::env::var(name).unwrap_or_default());
    ["XDG_SESSION_TYPE", "XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP"].map(variable).join(" ")
}

/// Everything worth knowing about this Heyo and where it runs, for a bug
/// report. Runs Node.js to ask its version: not for the main thread.
pub fn report(app: &tauri::AppHandle) -> String {
    use tauri::Manager;
    let node = app.state::<crate::SettingsState>().0.read().unwrap().node();
    let node = match crate::settings::node_version(&node) {
        Ok(version) => version,
        Err(why) => format!("not usable ({why})"),
    };
    let capabilities = serde_json::to_string(&crate::platform::capabilities()).unwrap_or_default();
    let path = log_path();
    let log = fs::read(&path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    let facts = [
        format!("Heyo {} ({})", app.package_info().version, if cfg!(debug_assertions) { "debug" } else { "release" }),
        format!("System: {}", system()),
        format!("Session: {}", session()),
        format!("Platform: {capabilities}"),
        format!("Node.js: {node}"),
        format!("Web view: {}", tauri::webview_version().unwrap_or_else(|e| format!("unknown ({e})"))),
        match &log {
            Ok(log) => format!("Log ({}, last {LINES} lines):\n{}", path.display(), tail(log, LINES).trim_end()),
            Err(e) => format!("Log ({}): none ({e})", path.display()),
        },
    ];
    scrub(&facts.join("\n"), &crate::paths::home().to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_do_not_survive_scrubbing() {
        let home = "/home/ada";
        let secrets = [
            "sk-ant-api03-AbCdEf123456_ghIJkl-7890mnopQRst",
            "ANTHROPIC_API_KEY=sk-ant-api03-zzzz1111yyyy2222",
            r#"{"apiKey": "hunter2hunter2", "model": "x"}"#,
            "Authorization: Bearer abc.def-ghi_123456",
            "authorization: Basic dXNlcjpwYXNzd29yZA==",
            "fetch https://api.example.com/v1?access_token=1f9a8b7c&page=2 failed",
            "password: correct horse",
            "https://ada:s3cr3t@git.example.com/repo.git",
            "ghp_0123456789abcdefghijABCDEFGHIJ012345",
            "xoxb-1234567890-abcdefghij",
            "AKIAIOSFODNN7EXAMPLE",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
            "session 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08 opened",
            "refresh_token='AQAAAAEAABAAAwAAAAUAAAAHAAAACQAAAAsAAAAN'",
        ];
        let kept = ["hunter2", "s3cr3t", "sk-ant", "abc.def", "dXNlcjpw", "1f9a8b7c", "correct", "ghp_", "xoxb", "AKIA", "eyJ", "9f86d081", "AQAAAAEAAB"];
        for secret in secrets {
            let scrubbed = scrub(&format!("heyo: a helper said: {secret}"), home);
            assert!(scrubbed.contains("[redacted]"), "{secret} → {scrubbed}");
            for part in kept {
                assert!(!scrubbed.contains(part), "{secret} → {scrubbed}");
            }
        }
    }

    #[test]
    fn what_explains_a_problem_survives_scrubbing() {
        let home = "/home/ada";
        let log = "heyo: started\n\
            heyo: a helper ended: Ok(ExitStatus(unix_wait_status(256)))\n\
            heyo: failed to write /home/ada/.config/heyo/settings.json: Permission denied (os error 13)\n\
            Error: Cannot find module '/home/ada/.local/share/heyo/extensions/some-very-long-extension-name-indeed/index.js'\n\
            at /usr/lib/aarch64-linux-gnu/webkit2gtk-4.1/injected-bundle/libwebkit2gtkinjectedbundle.so\n\
            heyo: panicked at src/platform/x11/windows.rs:240:13 (thread main, version 0.1.0): index out of bounds\n\
            Platform: {\"session\":\"x11\",\"windows\":\"full\",\"holdKeys\":[]}\n\
            the token bucket refilled; 3 secret santa commands found";
        let scrubbed = scrub(log, home);
        assert!(!scrubbed.contains("/home/ada"));
        assert!(!scrubbed.contains("[redacted]"), "{scrubbed}");
        assert_eq!(scrubbed, log.replace("/home/ada", "~"));
        // No home to speak of changes nothing (and replaces no empty string).
        assert_eq!(scrub("heyo: started", ""), "heyo: started");
        assert_eq!(scrub("C:\\Users\\ada\\AppData\\heyo.log", "C:\\Users\\ada\\"), "~\\AppData\\heyo.log");
    }

    #[test]
    fn a_report_carries_the_end_of_the_log() {
        let log = (1..=500).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n") + "\n";
        let last = tail(&log, 200);
        assert_eq!(last.lines().count(), 200);
        assert!(last.starts_with("line 301\n") && last.ends_with("line 500\n"));
        assert_eq!(tail("one\ntwo\n", 200), "one\ntwo\n");
        assert_eq!(tail("one\ntwo", 1), "two");
        assert_eq!(tail("", 5), "");
    }

    #[test]
    fn a_large_log_is_set_aside_and_begun_afresh() {
        let dir = std::env::temp_dir().join(format!("heyo-log-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("heyo.log");
        // No log yet: nothing to do, and opening one makes its directory.
        assert!(!rotate(&path, 10));
        open_log(&path).unwrap().write_all(b"short").unwrap();
        assert!(!rotate(&path, 10) && !trim(&path, 10));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }

        // At a start: the file moves aside.
        open_log(&path).unwrap().write_all(b" and then much longer").unwrap();
        assert!(rotate(&path, 10));
        assert!(!path.exists());
        assert_eq!(fs::read_to_string(older(&path)).unwrap(), "short and then much longer");

        // While open: it is emptied where it is, and its writer goes on.
        let mut writer = open_log(&path).unwrap();
        writer.write_all(b"a second long stretch of log").unwrap();
        assert!(trim(&path, 10));
        assert_eq!(fs::read_to_string(older(&path)).unwrap(), "a second long stretch of log");
        writer.write_all(b"next").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "next");
        let _ = fs::remove_dir_all(&dir);
    }

    /// `cargo test -- --ignored standard_error` (alone: it takes the test
    /// program's standard error for good).
    #[test]
    #[ignore = "takes over standard error; run alone"]
    #[cfg(target_os = "linux")]
    fn standard_error_reaches_the_log() {
        let dir = std::env::temp_dir().join(format!("heyo-stderr-test-{}", std::process::id()));
        let path = dir.join("heyo.log");
        keep_log(&path).unwrap();
        // (`eprintln!` in a test goes to the test harness, not to standard error.)
        std::io::stderr().write_all(b"heyo: said by Heyo\n").unwrap();
        // A helper's standard error is Heyo's.
        std::process::Command::new("sh").args(["-c", "echo said by a helper >&2"]).status().unwrap();
        on_panic_report_for_test();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let log = fs::read_to_string(&path).unwrap();
        assert!(log.contains("heyo: said by Heyo\n"), "{log}");
        assert!(log.contains("said by a helper\n"), "{log}");
        assert!(log.contains("heyo: panicked at here:1:1 (thread"), "{log}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Writes a panic's report the way the hook does, without panicking.
    #[cfg(target_os = "linux")]
    fn on_panic_report_for_test() {
        write_report(&panic_report("for the test", "here:1:1", "test", "0: nowhere"));
    }

    #[test]
    fn a_panic_is_written_with_where_and_how() {
        let report = panic_report("index out of bounds", "src/root.rs:10:5", "main", "   0: heyo::root::search\n   1: main\n\n");
        assert!(report.starts_with("heyo: panicked at src/root.rs:10:5 (thread main, version "));
        assert!(report.contains("): index out of bounds\n   0: heyo::root::search\n"));
        assert!(report.ends_with("1: main\n"));
    }

    #[test]
    fn the_distribution_is_read_from_os_release() {
        let file = "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nID=ubuntu\n";
        assert_eq!(pretty_name(file).as_deref(), Some("Ubuntu 24.04.1 LTS"));
        assert_eq!(pretty_name("PRETTY_NAME=Arch Linux\n").as_deref(), Some("Arch Linux"));
        assert_eq!(pretty_name("NAME=x\n"), None);
    }
}
