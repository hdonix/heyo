//! What Heyo asks of the computer rather than of a window: locking and
//! powering down, the volume, the appearance, the trash, opening things
//! with the applications meant for them, the audio devices, and the
//! processes that are running.
//!
//! The desktop decides how ([`Desktop::system`] and friends); without a way
//! of its own it uses the freedesktop tools here.

use crate::i18n::{t, tf};
use std::collections::HashMap;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::quiet;
use super::desktop::desktop;

/// Something to do to the computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum Action {
    LockScreen,
    Sleep,
    Restart,
    ShutDown,
    LogOut,
    EmptyTrash,
    ToggleDarkMode,
    VolumeUp,
    VolumeDown,
    ToggleMute,
}

/// Does it. What comes back is what to tell the user, where there is
/// something to tell ("Volume 40%").
pub fn run(action: Action) -> Result<Option<String>, String> {
    desktop().system(action)
}

/// Whether the desktop shows applications dark, as "Toggle Dark Mode" reads it.
pub fn dark() -> Result<bool, String> {
    #[cfg(windows)]
    return Ok(!tool("reg", &["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "/v", "AppsUseLightTheme"])?.contains("0x1"));
    #[cfg(not(windows))]
    Ok(tool("gsettings", &["get", "org.gnome.desktop.interface", "color-scheme"])?.contains("prefer-dark"))
}

/// Opens a file, folder or URL: with `application` (a path to a program or
/// to a desktop entry) when given, else with whatever opens such things.
pub fn open(target: &str, application: Option<&str>) -> Result<(), String> {
    desktop().open(target, application)
}

pub fn trash(paths: &[String]) -> Result<(), String> {
    desktop().trash(paths)
}

/// The id of the application that opens `target` by default.
pub fn default_application(target: &str) -> Option<String> {
    desktop().default_application(target)
}

/// Which way sound goes through a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum AudioKind {
    Output,
    Input,
}

/// A speaker or a microphone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct AudioDevice {
    /// What the sound server calls it; what `use_audio_device` takes.
    pub name: String,
    pub description: String,
    pub kind: AudioKind,
    /// Whether sound goes through it now.
    pub current: bool,
}

/// A running program of the user's.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct ProcessInfo {
    pub pid: u32,
    /// Percent of one processor.
    pub cpu: f32,
    /// Bytes of memory in use.
    #[ts(type = "number")]
    pub memory: u64,
    pub name: String,
    pub command: String,
    /// The TCP ports it listens on.
    pub ports: Vec<u16>,
}

/// The speakers and microphones, outputs first.
pub fn audio_devices() -> Result<Vec<AudioDevice>, String> {
    desktop().audio_devices()
}

/// Sends sound through (or takes it from) the device of that name.
pub fn use_audio_device(kind: AudioKind, name: &str) -> Result<(), String> {
    desktop().use_audio_device(kind, name)
}

/// The user's processes, the busiest first.
pub fn processes() -> Result<Vec<ProcessInfo>, String> {
    desktop().processes()
}

/// Asks a process to quit; with `force`, ends it without asking.
pub fn end_process(pid: u32, force: bool) -> Result<(), String> {
    // 0 would be every process of Heyo's group, 1 the system's first, and
    // ending Heyo from within Heyo helps nobody.
    if pid <= 1 || pid == std::process::id() {
        return Err(format!("process {pid} is not one to end from here"));
    }
    desktop().end_process(pid, force)
}

/// Runs a tool to its end and returns what it printed.
pub(super) fn tool(program: &str, args: &[&str]) -> Result<String, String> {
    let output = quiet(program).args(args).output().map_err(|e| format!("could not run {program}: {e}"))?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        return Err(match said.trim() {
            "" => format!("{program} failed ({})", output.status),
            said => said.to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Starts a program and leaves it running.
pub(super) fn start(program: &str, args: &[&str]) -> Result<(), String> {
    // What is started outlives Heyo and is not Heyo's to log.
    let mut command = quiet(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = command.spawn().map_err(|e| format!("could not start {program}: {e}"))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// The first percentage in what `pactl get-sink-volume` prints.
fn volume_percent(output: &str) -> Option<&str> {
    let end = output.find('%')?;
    let start = output[..end].rfind(|c: char| !c.is_ascii_digit()).map_or(0, |i| i + 1);
    (start < end).then(|| &output[start..end])
}

fn volume(change: &str) -> Result<Option<String>, String> {
    tool("pactl", &["set-sink-volume", "@DEFAULT_SINK@", change])?;
    let now = tool("pactl", &["get-sink-volume", "@DEFAULT_SINK@"])?;
    Ok(Some(tf("Volume {0}%", &[&volume_percent(&now).unwrap_or("?")])))
}

/// The freedesktop way: systemd, PulseAudio/PipeWire, GLib and GNOME's settings.
pub(super) fn freedesktop(action: Action) -> Result<Option<String>, String> {
    const SCHEME: [&str; 2] = ["org.gnome.desktop.interface", "color-scheme"];
    let done = |_| None;
    match action {
        Action::LockScreen => tool("loginctl", &["lock-session"]).map(done),
        Action::Sleep => tool("systemctl", &["suspend"]).map(done),
        Action::Restart => tool("systemctl", &["reboot"]).map(done),
        Action::ShutDown => tool("systemctl", &["poweroff"]).map(done),
        Action::LogOut => tool("gnome-session-quit", &["--logout", "--no-prompt"]).map(done),
        Action::EmptyTrash => tool("gio", &["trash", "--empty"]).map(|_| Some(t("Trash emptied").into())),
        Action::ToggleDarkMode => {
            let dark = tool("gsettings", &["get", SCHEME[0], SCHEME[1]])?.contains("prefer-dark");
            tool("gsettings", &["set", SCHEME[0], SCHEME[1], if dark { "default" } else { "prefer-dark" }])?;
            Ok(Some(if dark { t("Light appearance") } else { t("Dark appearance") }.into()))
        }
        Action::VolumeUp => volume("+5%"),
        Action::VolumeDown => volume("-5%"),
        Action::ToggleMute => {
            tool("pactl", &["set-sink-mute", "@DEFAULT_SINK@", "toggle"])?;
            let muted = tool("pactl", &["get-sink-mute", "@DEFAULT_SINK@"])?.contains("yes");
            Ok(Some(if muted { t("Muted") } else { t("Unmuted") }.into()))
        }
    }
}

pub(super) fn freedesktop_open(target: &str, application: Option<&str>) -> Result<(), String> {
    match application {
        None => super::common::open(target),
        Some(entry) if entry.ends_with(".desktop") => start("gio", &["launch", entry, target]),
        Some(program) => start(program, &[target]),
    }
}

pub(super) fn freedesktop_trash(paths: &[String]) -> Result<(), String> {
    paths.iter().try_for_each(|path| tool("gio", &["trash", path]).map(drop))
}

pub(super) fn freedesktop_default_application(target: &str) -> Option<String> {
    let kind = tool("xdg-mime", &["query", "filetype", target]).ok()?;
    let id = tool("xdg-mime", &["query", "default", kind.trim()]).ok()?;
    Some(id.trim().to_owned()).filter(|id| !id.is_empty())
}

// --- Audio devices, through PulseAudio's tools (PipeWire answers them too) -----

/// The devices in what `pactl -f json list sinks` (or `sources`) prints.
fn parse_devices(json: &str, kind: AudioKind, current: &str) -> Result<Vec<AudioDevice>, String> {
    #[derive(Deserialize)]
    struct Listed {
        name: String,
        #[serde(default)]
        description: String,
    }
    let listed: Vec<Listed> = serde_json::from_str(json).map_err(|e| format!("pactl did not list the devices as expected: {e}"))?;
    Ok(listed
        .into_iter()
        // A monitor is the loopback of an output, not a microphone.
        .filter(|d| kind == AudioKind::Output || !d.name.ends_with(".monitor"))
        .map(|d| AudioDevice {
            current: d.name == current.trim(),
            description: if d.description.is_empty() { d.name.clone() } else { d.description },
            name: d.name,
            kind,
        })
        .collect())
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(super) fn freedesktop_audio_devices() -> Result<Vec<AudioDevice>, String> {
    let mut devices = parse_devices(&tool("pactl", &["-f", "json", "list", "sinks"])?, AudioKind::Output, &tool("pactl", &["get-default-sink"])?)?;
    devices.extend(parse_devices(&tool("pactl", &["-f", "json", "list", "sources"])?, AudioKind::Input, &tool("pactl", &["get-default-source"])?)?);
    Ok(devices)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(super) fn freedesktop_use_audio_device(kind: AudioKind, name: &str) -> Result<(), String> {
    let set = match kind {
        AudioKind::Output => "set-default-sink",
        AudioKind::Input => "set-default-source",
    };
    // After `--`, a name is a name whatever it begins with.
    tool("pactl", &[set, "--", name]).map(drop)
}

// --- Processes, through ps, ss and kill ------------------------------------------

/// The processes in what `ps -o pid=,pcpu=,rss=,comm=` prints, in its order.
/// The name comes last because it may hold spaces ("Web Content").
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_processes(listing: &str) -> Vec<ProcessInfo> {
    listing
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let cpu = fields.next()?.parse().ok()?;
            // Kibibytes.
            let memory = fields.next()?.parse::<u64>().ok()? * 1024;
            let name = fields.collect::<Vec<_>>().join(" ");
            (!name.is_empty()).then(|| ProcessInfo { pid, cpu, memory, command: name.clone(), name, ports: Vec::new() })
        })
        .collect()
}

/// The command lines in what `ps -o pid=,args=` prints, by process.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_command_lines(listing: &str) -> HashMap<u32, String> {
    listing
        .lines()
        .filter_map(|line| {
            let (pid, command) = line.trim_start().split_once(char::is_whitespace)?;
            Some((pid.parse().ok()?, command.trim().to_owned()))
        })
        .collect()
}

/// The TCP ports each process listens on, in what `ss -ltnpH` prints:
/// `LISTEN 0 511 127.0.0.1:3000 0.0.0.0:* users:(("node",pid=123,fd=20))`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_listening(listing: &str) -> HashMap<u32, Vec<u16>> {
    let mut ports: HashMap<u32, Vec<u16>> = HashMap::new();
    for line in listing.lines() {
        let Some(port) = line.split_whitespace().nth(3).and_then(|local| local.rsplit_once(':')?.1.parse::<u16>().ok()) else { continue };
        for owner in line.split("pid=").skip(1) {
            let digits: String = owner.chars().take_while(char::is_ascii_digit).collect();
            let Ok(pid) = digits.parse() else { continue };
            let of_process = ports.entry(pid).or_default();
            // Listening on IPv4 and IPv6 is one port, not two.
            if !of_process.contains(&port) {
                of_process.push(port);
            }
        }
    }
    ports
}

/// Puts the three listings together, leaving out the process `but`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn join_processes(stats: &str, command_lines: &str, listening: &str, but: u32) -> Vec<ProcessInfo> {
    let mut commands = parse_command_lines(command_lines);
    let mut ports = parse_listening(listening);
    parse_processes(stats)
        .into_iter()
        .filter(|process| process.pid != but)
        .map(|mut process| {
            if let Some(command) = commands.remove(&process.pid) {
                process.command = command;
            }
            process.ports = ports.remove(&process.pid).unwrap_or_default();
            process
        })
        .collect()
}

#[cfg(target_os = "linux")]
pub(super) fn freedesktop_processes() -> Result<Vec<ProcessInfo>, String> {
    use std::os::unix::fs::MetadataExt;
    let user = std::fs::metadata("/proc/self").map_err(|e| format!("could not tell whose processes to list: {e}"))?.uid().to_string();
    let stats = tool("ps", &["-o", "pid=,pcpu=,rss=,comm=", "--sort=-pcpu", "-u", &user])?;
    let command_lines = tool("ps", &["-o", "pid=,args=", "-u", &user])?;
    // Without `ss` the list is still worth having.
    let listening = tool("ss", &["-ltnpH"]).unwrap_or_default();
    Ok(join_processes(&stats, &command_lines, &listening, std::process::id()))
}

#[cfg(target_os = "linux")]
pub(super) fn freedesktop_end_process(pid: u32, force: bool) -> Result<(), String> {
    tool("kill", &[if force { "-KILL" } else { "-TERM" }, "--", &pid.to_string()]).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::desktop::fake::Fake;

    #[test]
    fn reads_the_volume_pactl_reports() {
        let said = "Volume: front-left: 26214 /  40% / -23.88 dB,   front-right: 26214 /  40% / -23.88 dB";
        assert_eq!(volume_percent(said), Some("40"));
        assert_eq!(volume_percent("Volume: mono: 65536 / 100% / 0.00 dB"), Some("100"));
        assert_eq!(volume_percent("no sink"), None);
        assert_eq!(volume_percent("%"), None);
    }

    #[test]
    fn an_action_is_named_as_its_command_is() {
        let read = |name: &str| serde_json::from_value::<Action>(name.into());
        assert_eq!(read("lock-screen").unwrap(), Action::LockScreen);
        assert_eq!(read("toggle-dark-mode").unwrap(), Action::ToggleDarkMode);
        assert!(read("format-disk").is_err());
    }

    #[test]
    fn the_desktop_is_asked_not_a_tool() {
        let fake = Fake::with_windows(&[]);
        fake.during(|| {
            assert_eq!(run(Action::VolumeUp), Ok(None));
            open("/tmp/a.txt", Some("editor.desktop")).unwrap();
            trash(&["/tmp/a.txt".into()]).unwrap();
        });
        assert_eq!(fake.asked(), ["system VolumeUp", "open /tmp/a.txt with editor.desktop", "trash /tmp/a.txt"]);
    }

    #[test]
    fn reads_the_devices_pactl_lists() {
        let sinks = r#"[{"index":71,"state":"SUSPENDED","name":"auto_null","description":"Dummy Output","mute":false,"monitor_source":"auto_null.monitor"},
            {"index":80,"name":"alsa_output.usb-Focusrite.analog-stereo","description":"Scarlett Solo","properties":{"node.name":"x"}}]"#;
        let outputs = parse_devices(sinks, AudioKind::Output, "alsa_output.usb-Focusrite.analog-stereo\n").unwrap();
        assert_eq!(outputs.iter().map(|d| (d.description.as_str(), d.current)).collect::<Vec<_>>(), [("Dummy Output", false), ("Scarlett Solo", true)]);

        // An output's monitor is listed among the sources but is no microphone.
        let sources = r#"[{"name":"auto_null.monitor","description":"Monitor of Dummy Output"},{"name":"alsa_input.pci.analog-stereo","description":"Built-in Microphone"},{"name":"bare"}]"#;
        let inputs = parse_devices(sources, AudioKind::Input, "bare").unwrap();
        assert_eq!(inputs.iter().map(|d| d.description.as_str()).collect::<Vec<_>>(), ["Built-in Microphone", "bare"]);
        assert!(inputs[1].current && inputs.iter().all(|d| d.kind == AudioKind::Input));

        assert!(parse_devices("Connection failure: Connection refused", AudioKind::Output, "").is_err());
        assert_eq!(parse_devices("[]", AudioKind::Output, "").unwrap(), []);
    }

    const PS: &str = "1774499  100 1176860 rustc\n  33266 99.0 235712 python3\n   3274  8.2 1441952 Web Content\n     12  0.0      0 \n garbage\n   4000  0.5   2048 node\n";
    const ARGS: &str = "   2777 /usr/lib/systemd/systemd --user\n  33266 python3 -m http.server 8000\n   3274 /usr/lib/firefox/firefox -contentproc -isForBrowser\n   4000 node server.js\n";
    const SS: &str = concat!(
        "LISTEN 0      32           127.0.1.1:53    0.0.0.0:*                                            \n",
        "LISTEN 0      511          127.0.0.1:3000  0.0.0.0:* users:((\"node\",pid=4000,fd=20))    \n",
        "LISTEN 0      511              [::1]:3000     [::]:* users:((\"node\",pid=4000,fd=21))    \n",
        "LISTEN 0      5                    *:8000        *:* users:((\"python3\",pid=33266,fd=3),(\"python3\",pid=33267,fd=3))\n",
        "LISTEN 0      128            0.0.0.0:9229  0.0.0.0:* users:((\"node\",pid=4000,fd=25))\n",
    );

    #[test]
    fn reads_the_processes_ps_lists() {
        let processes = parse_processes(PS);
        assert_eq!(processes.iter().map(|p| p.pid).collect::<Vec<_>>(), [1774499, 33266, 3274, 4000]);
        assert_eq!(processes[0].cpu, 100.0);
        assert_eq!(processes[1].memory, 235712 * 1024);
        // A name with a space in it is still one name.
        assert_eq!(processes[2].name, "Web Content");
        assert_eq!(parse_command_lines(ARGS)[&33266], "python3 -m http.server 8000");
    }

    #[test]
    fn reads_the_ports_ss_lists() {
        let ports = parse_listening(SS);
        assert_eq!(ports[&4000], [3000, 9229]);
        assert_eq!(ports[&33266], [8000]);
        assert_eq!(ports[&33267], [8000]);
        // A socket whose owner ss does not name belongs to nobody here.
        assert_eq!(ports.len(), 3);
        assert!(parse_listening("").is_empty());
    }

    #[test]
    fn a_process_is_listed_with_its_command_line_and_ports() {
        let processes = join_processes(PS, ARGS, SS, 3274);
        assert_eq!(processes.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["rustc", "python3", "node"]);
        // Gone between the two listings: its name stands in for the command line.
        assert_eq!(processes[0].command, "rustc");
        assert_eq!(processes[1].command, "python3 -m http.server 8000");
        assert_eq!(processes[2].ports, [3000, 9229]);
        assert!(processes[0].ports.is_empty());
    }

    #[test]
    fn some_processes_are_not_to_be_ended() {
        let fake = Fake::with_windows(&[]);
        fake.during(|| {
            assert!(end_process(0, true).is_err());
            assert!(end_process(1, false).is_err());
            assert!(end_process(std::process::id(), false).is_err());
            end_process(4000, false).unwrap();
            use_audio_device(AudioKind::Input, "mic").unwrap();
        });
        assert_eq!(fake.asked(), ["end 4000", "use Input mic"]);
    }
}
