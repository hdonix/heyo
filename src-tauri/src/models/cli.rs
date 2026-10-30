//! A model reached through a command-line tool on this computer: Claude
//! Code (`claude`) or Codex (`codex`), each signed in to its maker's
//! service with the user's own account, which is what it is used for.
//! What each is told and what it says back is the tool's own
//! (`claude_code.rs`, `codex.rs`); this is what both share: where the
//! executable is, and running it once per question.
//!
//! Both are coding agents, and Heyo wants nothing of them but text: each
//! is run with every tool it has that can be turned off turned off, in an
//! empty directory of its own, its configuration's extras (instruction
//! files, MCP servers, hooks, plugins) left out where it allows that, and
//! the question on its standard input (never on the command line, which
//! other users can read and which is limited in length).
//!
//! An answer called off (the future dropped) ends the tool and whatever it
//! started: it runs as a process group of its own on Unix, and on Windows
//! its tree is ended with `taskkill /T`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use super::{ModelError, Question};

/// Which of the two tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    ClaudeCode,
    Codex,
}

impl Tool {
    /// Its name, as its maker writes it (never translated).
    pub fn name(self) -> &'static str {
        match self {
            Tool::ClaudeCode => "Claude Code",
            Tool::Codex => "Codex",
        }
    }

    /// The command it is installed as.
    pub fn program(self) -> &'static str {
        match self {
            Tool::ClaudeCode => "claude",
            Tool::Codex => "codex",
        }
    }

    /// What signs in to it, typed in a terminal.
    pub fn sign_in(self) -> &'static str {
        match self {
            Tool::ClaudeCode => "claude auth login",
            Tool::Codex => "codex login",
        }
    }
}

/// A command-line tool, and the model to ask it for.
#[derive(Debug, Clone, PartialEq)]
pub struct Cli {
    pub(super) tool: Tool,
    /// The executable the settings name; empty: found (`executable`).
    path: String,
    /// Empty: the tool's own default.
    pub(super) model: String,
}

impl Cli {
    pub fn new(tool: Tool, path: &str, model: &str) -> Cli {
        Cli { tool, path: path.trim().to_owned(), model: model.trim().to_owned() }
    }

    pub async fn answer(&self, question: &Question, on_text: &mut (dyn FnMut(&str) + Send)) -> Result<String, ModelError> {
        let program = executable(self.tool, &self.path).ok_or_else(|| ModelError::CliNotFound { tool: self.tool, at: self.path.clone() })?;
        let scratch = Scratch::new(self.tool).map_err(|e| self.failed(e.to_string()))?;
        match self.tool {
            Tool::ClaudeCode => super::claude_code::answer(self, &program, &scratch, question, on_text).await,
            Tool::Codex => super::codex::answer(self, &program, &scratch, question, on_text).await,
        }
    }

    pub(super) fn failed(&self, message: impl Into<String>) -> ModelError {
        ModelError::CliFailed { tool: self.tool, message: message.into() }
    }

    /// What went wrong, from what the tool said of it: not being signed
    /// in is told apart from the rest, whose words are passed on.
    pub(super) fn failure(&self, said: &str) -> ModelError {
        let lower = said.to_lowercase();
        let signed_out = ["not logged in", "please run /login", "run /login", "codex login", "invalid api key", "401 unauthorized", "authentication_failed", "oauth token has expired"];
        if signed_out.iter().any(|sign| lower.contains(sign)) {
            return ModelError::CliNotSignedIn(self.tool);
        }
        self.failed(said)
    }

    /// The tool ended without an answer: why, in its own words where it
    /// left any (`stderr`), else by how it ended.
    pub(super) fn ended_without_answer(&self, ran: &Ran) -> ModelError {
        let said = last_words(&ran.stderr);
        match ran.status {
            Some(status) if status.success() && said.is_empty() => ModelError::CliSaidNothing(self.tool),
            Some(status) if said.is_empty() => self.failed(status.to_string()),
            _ if said.is_empty() => ModelError::CliSaidNothing(self.tool),
            _ => self.failure(&said),
        }
    }
}

/// What the tool is given for a question: the system prompt goes beside
/// it, in a file, as both read it from one.
pub(super) fn system(question: &Question) -> &str {
    // Without one, each would fall back to its coding agent's own.
    match question.system.trim() {
        "" => "You are a helpful assistant.",
        _ => &question.system,
    }
}

/// The user's message: text to work on is fenced, so that it reads as
/// material and not as a request; in a conversation, the turns before it
/// are written out ahead of it, as neither tool can be handed them as
/// turns of its own.
pub(super) fn message(question: &Question) -> String {
    let Some(history) = &question.history else {
        return format!("<text>\n{}\n</text>", question.prompt);
    };
    if history.is_empty() {
        return question.prompt.clone();
    }
    let mut said = String::from("The conversation so far:\n\n<conversation>\n");
    for turn in history {
        let role = if turn.role == "assistant" { "assistant" } else { "user" };
        said.push_str(&format!("<{role}>\n{}\n</{role}>\n", turn.content));
    }
    said.push_str(&format!("</conversation>\n\nReply to the user's next message:\n\n{}", question.prompt));
    said
}

// --- Finding the executable ------------------------------------------------------

/// The executable of `tool`: the one `setting` names (a path, or a command
/// looked for as the tool's own is), else the tool's command on `PATH` or
/// where its installers put it.
pub fn executable(tool: Tool, setting: &str) -> Option<PathBuf> {
    let setting = setting.trim();
    if setting.is_empty() {
        return search(tool.program(), &places());
    }
    let given = Path::new(setting);
    if given.components().count() > 1 || given.is_absolute() {
        return runnable(given).then(|| given.to_path_buf());
    }
    search(setting, &places())
}

/// Where a command is looked for: `PATH`, then where the tools' installers
/// put them, which a desktop session's `PATH` may not have (a GUI
/// application on macOS is not given the shell's).
fn places() -> Vec<PathBuf> {
    let mut places: Vec<PathBuf> = std::env::var_os("PATH").map(|path| std::env::split_paths(&path).collect()).unwrap_or_default();
    #[cfg(windows)]
    {
        let under = |variable: &str, rest: &str| std::env::var_os(variable).map(|base| PathBuf::from(base).join(rest));
        // npm's global commands, and the native installers'.
        places.extend([under("APPDATA", "npm"), under("USERPROFILE", ".local\\bin")].into_iter().flatten());
    }
    #[cfg(not(windows))]
    {
        let home = crate::paths::home();
        places.extend([home.join(".local/bin"), home.join(".claude/local"), home.join(".npm-global/bin"), "/opt/homebrew/bin".into(), "/usr/local/bin".into()]);
    }
    places
}

/// `name` in the first of `places` that has it. On Windows a command is
/// an `.exe`, or npm's `.cmd` shim around a script.
fn search(name: &str, places: &[PathBuf]) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) && Path::new(name).extension().is_none() { [".exe", ".cmd", ".bat"].iter().map(|ending| format!("{name}{ending}")).collect() } else { vec![name.to_owned()] };
    places.iter().flat_map(|place| names.iter().map(move |name| place.join(name))).find(|candidate| runnable(candidate))
}

fn runnable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    path.is_file()
}

/// The executable the settings name and its version, as the settings
/// window shows it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "CliFound")]
pub struct Found {
    pub path: String,
    /// What `--version` printed.
    pub version: String,
}

/// The executable of `tool` and its version, or why there is none to use.
/// Asked on every settings change, so the answer is remembered a while.
pub fn found(tool: Tool, setting: &str) -> Result<Found, String> {
    type Cached = (Tool, String, Instant, Result<Found, String>);
    static LAST: std::sync::Mutex<Option<Cached>> = std::sync::Mutex::new(None);
    if let Some((was, path, at, result)) = &*LAST.lock().unwrap() {
        if *was == tool && path == setting && at.elapsed() < Duration::from_secs(60) {
            return result.clone();
        }
    }
    let result = look(tool, setting);
    *LAST.lock().unwrap() = Some((tool, setting.to_owned(), Instant::now(), result.clone()));
    result
}

fn look(tool: Tool, setting: &str) -> Result<Found, String> {
    use crate::i18n::tf;
    let Some(path) = executable(tool, setting) else {
        return Err(match setting.trim() {
            "" => tf("{0} was not found. Install it, or name its executable here.", &[&tool.name()]),
            named => tf("There is no executable at {0}.", &[&named]),
        });
    };
    let shown = path.to_string_lossy().into_owned();
    let mut child = crate::platform::common::quiet(&path).arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("{shown}: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(tf("{0} --version did not answer", &[&shown]));
            }
        }
    }
    let output = child.wait_with_output().map_err(|e| format!("{shown}: {e}"))?;
    let version = String::from_utf8_lossy(&output.stdout).lines().next().unwrap_or_default().trim().to_owned();
    if !output.status.success() || version.is_empty() {
        return Err(tf("{0} --version failed", &[&shown]));
    }
    Ok(Found { path: shown, version })
}

// --- Running it ------------------------------------------------------------------

/// A directory of its own for one question: `work`, empty, is where the
/// tool runs; beside it, the files it is given (the system prompt,
/// pictures). Gone with it.
pub(super) struct Scratch(PathBuf);

impl Scratch {
    fn new(tool: Tool) -> std::io::Result<Scratch> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!("heyo-{}-{}-{}", tool.program(), std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        // What is asked is nobody else's to read.
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(dir.join("work"))?;
        Ok(Scratch(dir))
    }

    pub(super) fn work(&self) -> PathBuf {
        self.0.join("work")
    }

    /// Writes a file beside the working directory.
    pub(super) fn file(&self, name: &str, contents: &[u8]) -> std::io::Result<PathBuf> {
        let path = self.0.join(name);
        std::fs::write(&path, contents)?;
        Ok(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// How a run ended.
pub(super) struct Ran {
    /// How the tool exited; none when it was ended once it had answered.
    pub status: Option<ExitStatus>,
    /// The end of what it wrote to its standard error.
    pub stderr: String,
}

/// A running tool. Dropped, it ends, and so does everything it started.
struct Running(tokio::process::Child);

impl Drop for Running {
    fn drop(&mut self) {
        // None once it has been waited for: then it ended by itself, and
        // its process group may be another's by now.
        let Some(pid) = self.0.id() else { return };
        #[cfg(unix)]
        // SAFETY: a signal to the process group the tool leads (`process_group(0)`).
        unsafe {
            libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
        }
        #[cfg(windows)]
        {
            // Before the tool itself ends (`kill_on_drop`), while its
            // children can still be found from it.
            let _ = crate::platform::common::quiet("taskkill").args(["/T", "/F", "/PID", &pid.to_string()]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
    }
}

/// Runs `program` with `args` in `scratch`, `stdin` written to it, and
/// hands `each` every line it writes until `each` says it has the answer
/// (false) or the tool ends.
pub(super) async fn run(cli: &Cli, program: &Path, scratch: &Scratch, args: Vec<OsString>, stdin: Vec<u8>, mut each: impl FnMut(&str) -> bool) -> Result<Ran, ModelError> {
    let mut command = crate::platform::common::quiet(program);
    command.args(&args).current_dir(scratch.work()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // Claude Code refuses to start inside another of its sessions, which is
    // what this variable says Heyo was started from.
    command.env_remove("CLAUDECODE");
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let mut child = command.spawn();
    // An executable just written (an update of the tool, as it happens) is
    // busy while a process started elsewhere meanwhile still has it open.
    for _ in 0..10 {
        match &child {
            Err(e) if e.raw_os_error() == Some(26) && cfg!(unix) => std::thread::sleep(Duration::from_millis(20)),
            _ => break,
        }
        child = command.spawn();
    }
    let child = child.map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ModelError::CliNotFound { tool: cli.tool, at: program.to_string_lossy().into_owned() },
        _ => cli.failed(format!("{}: {e}", program.display())),
    })?;
    let mut child = Running(child);

    let mut input = child.0.stdin.take().expect("stdin is piped");
    tauri::async_runtime::spawn(async move {
        // It may end before reading all of it; what it said then tells why.
        let _ = input.write_all(&stdin).await;
    });
    let mut errors = child.0.stderr.take().expect("stderr is piped");
    let stderr = tauri::async_runtime::spawn(async move {
        // All of it is read, so that the tool never waits on a full pipe;
        // the end is kept.
        let (mut kept, mut chunk) = (Vec::new(), [0u8; 8192]);
        while let Ok(read @ 1..) = errors.read(&mut chunk).await {
            kept.extend_from_slice(&chunk[..read]);
            if kept.len() > 65536 {
                kept.drain(..kept.len() - 32768);
            }
        }
        String::from_utf8_lossy(&kept).into_owned()
    });

    let mut lines = BufReader::new(child.0.stdout.take().expect("stdout is piped")).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if !each(&line) {
                    // Answered: what it does after (summaries, saving) is not waited for.
                    return Ok(Ran { status: None, stderr: String::new() });
                }
            }
            Ok(None) => break,
            Err(e) => return Err(cli.failed(e.to_string())),
        }
    }
    let status = child.0.wait().await.ok();
    drop(child);
    let stderr = stderr.await.unwrap_or_default();
    Ok(Ran { status, stderr })
}

/// The last few lines a tool wrote to its standard error, without colours:
/// what it said when it failed.
fn last_words(stderr: &str) -> String {
    static COLOURS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").unwrap());
    let plain = COLOURS.replace_all(stderr, "");
    // Codex says this before it reads the question.
    let lines: Vec<&str> = plain.lines().map(str::trim).filter(|line| !line.is_empty() && *line != "Reading prompt from stdin...").collect();
    let said = lines[lines.len().saturating_sub(3)..].join(" ");
    match said.char_indices().nth(400) {
        Some((cut, _)) => format!("{}…", &said[..cut]),
        None => said,
    }
}

/// The value of a `-c key=value` override, as a TOML string.
pub(super) fn toml_string(value: &str) -> String {
    // A literal string takes a Windows path as it is; one with a quote of
    // its own in it is written as a basic string, which JSON's is.
    if value.contains(['\'', '\n', '\r']) {
        serde_json::to_string(value).expect("a string is JSON")
    } else {
        format!("'{value}'")
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::models::{Model, Turn};

    /// A stand-in for a tool: a shell script that keeps its arguments and
    /// what it was given on its standard input beside itself, then does
    /// what `body` says.
    pub struct Stub {
        pub dir: PathBuf,
        name: String,
    }

    impl Stub {
        pub fn new(name: &str, body: &str) -> Stub {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!("heyo-stub-{}-{}-{}", name, std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let script = format!("#!/bin/sh\nhere=\"{}\"\nfor arg in \"$@\"; do printf '%s\\n' \"$arg\"; done > \"$here/args\"\npwd > \"$here/cwd\"\ncat > \"$here/stdin\"\n{body}\n", dir.display());
            let path = dir.join(name);
            std::fs::write(&path, script).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            Stub { dir, name: name.into() }
        }

        /// The stub, doing `body` too at its end.
        pub fn then(self, body: &str) -> Stub {
            let path = self.dir.join(&self.name);
            let script = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, format!("{script}{body}\n")).unwrap();
            self
        }

        pub fn path(&self, name: &str) -> String {
            self.dir.join(name).to_string_lossy().into_owned()
        }

        pub fn args(&self) -> Vec<String> {
            std::fs::read_to_string(self.dir.join("args")).unwrap().lines().map(str::to_owned).collect()
        }

        pub fn stdin(&self) -> String {
            std::fs::read_to_string(self.dir.join("stdin")).unwrap()
        }

        pub fn kept(&self, name: &str) -> String {
            std::fs::read_to_string(self.dir.join(name)).unwrap_or_default()
        }
    }

    impl Drop for Stub {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    pub fn ask(model: &Model, question: &Question) -> (Result<String, ModelError>, Vec<String>) {
        let mut seen = Vec::new();
        let answer = super::super::mock::block_on(model.answer(question, &mut |text| seen.push(text.to_owned())));
        (answer, seen)
    }

    /// Whether a process is still there (a zombie is not).
    #[cfg(unix)]
    pub fn alive(pid: u32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| !stat.rsplit(')').next().unwrap_or_default().trim_start().starts_with('Z'))
    }

    /// An answer called off ends the tool and what it started: the stub
    /// writes the beginning of an answer, starts a child that would
    /// outlive it, and waits.
    #[cfg(unix)]
    pub fn calling_off_ends_the_tool_and_its_children(tool: Tool, program: &str, beginning: &str) {
        let body = format!("sleep 300 &\necho $! > \"$here/child\"\necho $$ > \"$here/tool\"\nprintf '%s\\n' '{beginning}'\nwait");
        let stub = Stub::new(program, &body);
        let model = Model::Cli(Cli::new(tool, &stub.path(program), ""));
        let (tx, rx) = std::sync::mpsc::channel();
        let asking = tauri::async_runtime::spawn(async move {
            let mut tell = move |text: &str| drop(tx.send(text.to_owned()));
            model.answer(&Question::material("", "x"), &mut tell).await
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), "Once");
        let (tool_pid, child_pid): (u32, u32) = (stub.kept("tool").trim().parse().unwrap(), stub.kept("child").trim().parse().unwrap());
        assert!(alive(tool_pid) && alive(child_pid));
        asking.abort();
        let deadline = Instant::now() + Duration::from_secs(5);
        while alive(tool_pid) || alive(child_pid) {
            assert!(Instant::now() < deadline, "the tool ({}) or its child ({}) is still running", alive(tool_pid), alive(child_pid));
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn text_to_work_on_is_fenced_and_a_conversation_written_out_before_the_message() {
        assert_eq!(message(&Question::material("Translate.", "hello")), "<text>\nhello\n</text>");
        let empty = Question { prompt: "Hi?".into(), history: Some(vec![]), ..Question::default() };
        assert_eq!(message(&empty), "Hi?");
        let going = Question {
            prompt: "And three?".into(),
            history: Some(vec![Turn { role: "user".into(), content: "One?".into() }, Turn { role: "assistant".into(), content: "1.".into() }]),
            ..Question::default()
        };
        assert_eq!(message(&going), "The conversation so far:\n\n<conversation>\n<user>\nOne?\n</user>\n<assistant>\n1.\n</assistant>\n</conversation>\n\nReply to the user's next message:\n\nAnd three?");
        // Neither tool is left to its coding agent's own instructions.
        assert_eq!(system(&Question::material("  ", "x")), "You are a helpful assistant.");
        assert_eq!(system(&Question::material("Be brief.", "x")), "Be brief.");
    }

    #[test]
    fn a_path_goes_to_codex_as_a_toml_string_it_reads_back_as_written() {
        assert_eq!(toml_string(r"C:\Users\Ann\AppData\Local\Temp\heyo\system.md"), r"'C:\Users\Ann\AppData\Local\Temp\heyo\system.md'");
        assert_eq!(toml_string(r"C:\Users\O'Brien\system.md"), r#""C:\\Users\\O'Brien\\system.md""#);
        assert_eq!(toml_string("never"), "'never'");
    }

    #[cfg(unix)]
    #[test]
    fn the_executable_is_the_one_named_else_found_on_the_path() {
        let stub = Stub::new("claude", "true");
        let places = [PathBuf::from("/nonexistent"), stub.dir.clone()];
        assert_eq!(search("claude", &places), Some(stub.dir.join("claude")));
        assert_eq!(search("codex", &places), None);
        // A file that cannot be run is not one.
        std::fs::write(stub.dir.join("codex"), "").unwrap();
        assert_eq!(search("codex", &places), None);
        // Named by a path, it is that file or nothing.
        assert_eq!(executable(Tool::ClaudeCode, &stub.path("claude")), Some(stub.dir.join("claude")));
        assert_eq!(executable(Tool::ClaudeCode, &stub.path("missing")), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_settings_are_shown_the_executable_and_its_version_or_why_there_is_none() {
        let stub = Stub::new("claude", "");
        std::fs::write(stub.dir.join("claude"), "#!/bin/sh\necho '2.1.302 (Claude Code)'\n").unwrap();
        assert_eq!(look(Tool::ClaudeCode, &stub.path("claude")), Ok(Found { path: stub.path("claude"), version: "2.1.302 (Claude Code)".into() }));
        assert_eq!(look(Tool::ClaudeCode, "/nonexistent/claude"), Err("There is no executable at /nonexistent/claude.".into()));
        std::fs::write(stub.dir.join("claude"), "#!/bin/sh\nexit 3\n").unwrap();
        assert_eq!(look(Tool::ClaudeCode, &stub.path("claude")), Err(format!("{} --version failed", stub.path("claude"))));
    }

    #[test]
    fn what_a_tool_said_when_it_failed_is_its_last_lines_without_colours() {
        assert_eq!(last_words("Reading prompt from stdin...\n\x1b[31mError:\x1b[0m one\n\ntwo\nthree\nfour\n"), "two three four");
        assert_eq!(last_words(""), "");
        assert_eq!(last_words(&"x".repeat(500)).chars().count(), 401);
    }

    #[test]
    fn being_signed_out_is_told_from_other_failures() {
        let cli = Cli::new(Tool::Codex, "", "");
        assert_eq!(cli.failure("unexpected status 401 Unauthorized: Missing bearer"), ModelError::CliNotSignedIn(Tool::Codex));
        assert_eq!(cli.failure("Not logged in · Please run /login"), ModelError::CliNotSignedIn(Tool::Codex));
        assert_eq!(cli.failure("model 'gpt-9' does not exist"), ModelError::CliFailed { tool: Tool::Codex, message: "model 'gpt-9' does not exist".into() });
    }
}
