//! What an extension may ask Heyo for, and who said so
//! (docs/extension-api.md, section 5).
//!
//! An extension written for Heyo lists the permissions it may come to use
//! in its manifest (`heyo.permissions`); one it did not list is refused
//! without asking. Of those it listed, some are the user's to give: the
//! palette asks the first time, and Allow is kept in
//! `$XDG_CONFIG_HOME/heyo/permissions.json` until it is taken back in the
//! settings. A refusal is not kept: the extension is told No, and the
//! question comes again another time.
//!
//! This is about what Heyo hands over or does on an extension's behalf. It
//! is consent and a record, not a sandbox: extensions run in one Node.js
//! process with the runtime, can read files and start programs themselves,
//! and could name another extension as the one asking. Nothing here, and
//! nothing the user is shown, may say otherwise.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use ts_rs::TS;

use crate::confirm::{self, Consent};
use crate::extensions::Heyo;
use crate::wire::{CaptureTarget, CaptureWhole, ErrorCode, ExtError, ExtRequest, Route};

/// The version of the extension API this Heyo provides.
pub const API_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum Permission {
    #[serde(rename = "windows")]
    Windows,
    #[serde(rename = "screen.capture")]
    ScreenCapture,
    #[serde(rename = "screen.text")]
    ScreenText,
    #[serde(rename = "microphone")]
    Microphone,
    #[serde(rename = "clipboard.history")]
    ClipboardHistory,
    #[serde(rename = "memory.read")]
    MemoryRead,
    /// Nothing uses it yet (`Memory.add`).
    #[serde(rename = "memory.write")]
    MemoryWrite,
    #[serde(rename = "files.search")]
    FilesSearch,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "input.paste")]
    InputPaste,
    #[serde(rename = "ai")]
    Ai,
    /// Nothing uses it yet (search providers).
    #[serde(rename = "search.provider")]
    SearchProvider,
    /// Live activities (docs/live-activities.md, 6.7): listing it is
    /// enough, and the user has a switch per extension in the settings.
    #[serde(rename = "activity")]
    Activity,
}

impl Permission {
    pub const ALL: [Permission; 13] = [
        Permission::Windows,
        Permission::ScreenCapture,
        Permission::ScreenText,
        Permission::Microphone,
        Permission::ClipboardHistory,
        Permission::MemoryRead,
        Permission::MemoryWrite,
        Permission::FilesSearch,
        Permission::System,
        Permission::InputPaste,
        Permission::Ai,
        Permission::SearchProvider,
        Permission::Activity,
    ];

    /// As manifests and the file of grants write it.
    pub fn name(self) -> &'static str {
        match self {
            Permission::Windows => "windows",
            Permission::ScreenCapture => "screen.capture",
            Permission::ScreenText => "screen.text",
            Permission::Microphone => "microphone",
            Permission::ClipboardHistory => "clipboard.history",
            Permission::MemoryRead => "memory.read",
            Permission::MemoryWrite => "memory.write",
            Permission::FilesSearch => "files.search",
            Permission::System => "system",
            Permission::InputPaste => "input.paste",
            Permission::Ai => "ai",
            Permission::SearchProvider => "search.provider",
            Permission::Activity => "activity",
        }
    }

    pub fn named(name: &str) -> Option<Permission> {
        Permission::ALL.into_iter().find(|permission| permission.name() == name)
    }

    /// Whether it is the user's to give, the first time it is used. The
    /// rest only work on what the extension itself supplies, or do what
    /// Raycast's API lets any extension do.
    fn asks(self) -> bool {
        !matches!(self, Permission::ScreenText | Permission::FilesSearch | Permission::InputPaste | Permission::Ai | Permission::Activity)
    }

    /// Whether Raycast's own API reaches the same thing, so that an
    /// extension from the store, which has no `heyo` manifest, may ask.
    fn through_raycast(self) -> bool {
        self == Permission::Windows
    }

    /// What it lets an extension do, in words that follow "allow it to".
    pub fn allows(self) -> &'static str {
        match self {
            Permission::Windows => "see, switch, move and close the windows of other applications",
            Permission::ScreenCapture => "take pictures of the screen without you selecting what",
            Permission::ScreenText => "read the text in images it gives Heyo",
            Permission::Microphone => "record the microphone and turn what is said into text",
            Permission::ClipboardHistory => "read your clipboard history",
            Permission::MemoryRead => "search what Heyo remembers, which includes your clipboard history",
            Permission::MemoryWrite => "add entries of its own to what Heyo remembers",
            Permission::FilesSearch => "search the names of your files",
            Permission::System => "lock or suspend the computer, change the volume, the sound device and the appearance, and empty the trash",
            Permission::InputPaste => "paste into the application in front",
            Permission::Ai => "use the AI model set up in Heyo",
            Permission::SearchProvider => "see what you type in the root search and add results to it",
            Permission::Activity => "show what it is doing in Heyo's In Progress section, and tell you when it is done",
        }
    }
}

/// The permissions a manifest lists. Names this Heyo does not know are a
/// newer Heyo's, and are passed over.
pub fn listed(heyo: &Heyo) -> BTreeSet<Permission> {
    heyo.permissions.iter().filter_map(|name| Permission::named(name)).collect()
}

/// What a request needs before it is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Need {
    pub permission: Permission,
    /// Listing it in the manifest is enough: the user does the thing by
    /// hand (selecting an area of the screen), or nothing of theirs is
    /// handed over (a notification, whether the desktop is dark).
    pub unasked: bool,
    /// The function of the API the request is made through, as an
    /// extension's author wrote it: what a refusal names.
    pub member: &'static str,
}

/// The permission a request needs, if any, and the function it is asked
/// through. The tests write both out (`shared/bindings/needs.ts`): the
/// runtime's declaration of those functions (`runtime/src/heyo/members.ts`)
/// takes the permission from there and is held to the names.
pub fn needed(request: &ExtRequest) -> Option<Need> {
    use ExtRequest::*;
    use Permission as P;
    let asked = |permission, member| Some(Need { permission, unasked: false, member });
    let unasked = |permission, member| Some(Need { permission, unasked: true, member });
    match request {
        // Asking what is possible, and ending what one started, is free.
        Unsubscribe { .. } | Supported { .. } | DictationInstalled | DictationStop { .. } | ActivityPresentation => None,
        WindowsList => asked(P::Windows, "Windows.list"),
        WindowsActive => asked(P::Windows, "Windows.active"),
        WindowsActivate { .. } => asked(P::Windows, "Windows.activate"),
        WindowsClose { .. } => asked(P::Windows, "Windows.close"),
        WindowsArrange { .. } => asked(P::Windows, "Windows.arrange"),
        WindowsSetBounds { .. } => asked(P::Windows, "Windows.setBounds"),
        WindowsMonitors => asked(P::Windows, "Windows.monitors"),
        // Raycast's, which has no module for it.
        FrontmostApplication => asked(P::Windows, "getFrontmostApplication"),
        ScreenCapture { target: CaptureTarget::Whole(CaptureWhole::Selection) } => unasked(P::ScreenCapture, "Screen.capture"),
        ScreenCapture { .. } => asked(P::ScreenCapture, "Screen.capture"),
        ScreenRecognizeText { .. } => asked(P::ScreenText, "Screen.recognizeText"),
        DictationListen { .. } => asked(P::Microphone, "Dictation.listen"),
        MemorySearch { .. } => asked(P::MemoryRead, "Memory.search"),
        MemoryGet { .. } => asked(P::MemoryRead, "Memory.get"),
        ClipboardHistoryCopy { .. } => asked(P::ClipboardHistory, "ClipboardHistory.copy"),
        FilesSearch { .. } => asked(P::FilesSearch, "Files.search"),
        SystemRun { .. } => asked(P::System, "System.run"),
        SystemAudioDevices => asked(P::System, "System.audioDevices"),
        SystemUseAudioDevice { .. } => asked(P::System, "System.useAudioDevice"),
        SystemAppearance => unasked(P::System, "System.appearance"),
        SystemNotify { .. } => unasked(P::System, "System.notify"),
        ClipboardPaste { .. } => asked(P::InputPaste, "Clipboard.paste"),
        AiProvider => asked(P::Ai, "AI.provider"),
        ActivityStart { .. } => unasked(P::Activity, "LiveActivity.start"),
        // What `start`, `list` and `get` hand over does these two.
        ActivityUpdate { .. } => unasked(P::Activity, "LiveActivity.Handle.update"),
        ActivityEnd { .. } => unasked(P::Activity, "LiveActivity.Handle.end"),
        ActivityList => unasked(P::Activity, "LiveActivity.list"),
        ActivityGet { .. } => unasked(P::Activity, "LiveActivity.get"),
        ActivityOnEvent => unasked(P::Activity, "LiveActivity.onEvent"),
    }
}

/// Whose a request is, by the name and the directory it gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identity {
    /// One of Heyo's own commands.
    Heyo,
    /// An extension, to be looked up where it says it is installed.
    Extension,
    /// Something installed that goes by Heyo's name.
    Pretender,
}

/// Heyo's own commands are trusted with everything and never asked about,
/// so being one must not be a matter of what a manifest says. They are
/// built into the runtime and installed nowhere: the runtime sends their
/// requests under Heyo's name with no directory, and gives every command
/// that was loaded from a directory that directory. An extension whose
/// manifest takes the name `@heyo` therefore comes with a directory, and is
/// refused rather than taken for Heyo or for anyone else.
///
/// (As for everything here: this holds for requests the runtime writes. An
/// extension that writes to the host behind the runtime's back can say what
/// it likes, until extensions run in processes of their own.)
pub fn identity(extension: &str, dir: &str) -> Identity {
    match (extension == crate::extensions::BUILTIN, dir.is_empty()) {
        (true, true) => Identity::Heyo,
        (true, false) => Identity::Pretender,
        (false, _) => Identity::Extension,
    }
}

/// What deciding goes by, beside the manifest and the grants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Context {
    /// One of Heyo's own commands asks.
    pub builtin: bool,
    /// The command runs in the menu bar or the background, where nobody can
    /// be asked.
    pub background: bool,
    pub route: Route,
    /// See [`Need::unasked`].
    pub unasked: bool,
    /// The user said No a moment ago.
    pub refused: bool,
    /// The user turned this off in the settings (live activities have a
    /// switch per extension, and one for all).
    pub switched_off: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The extension was written for a newer Heyo.
    Newer,
    /// The manifest does not list the permission.
    Undeclared,
    /// It would have to be asked for, and nobody is there to ask.
    Background,
    /// The user said No.
    Refused,
    /// The user turned it off in the settings.
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// The user is asked now.
    Ask,
    Deny { code: ErrorCode, reason: Reason },
}

/// Whether an extension may use a permission: given the `heyo` field of its
/// manifest (none for an extension from the Raycast Store), what the user
/// has allowed it, and the circumstances.
pub fn decide(heyo: Option<&Heyo>, permission: Permission, granted: &BTreeSet<Permission>, context: Context) -> Decision {
    let denied = |reason| Decision::Deny { code: ErrorCode::Denied, reason };
    if context.switched_off {
        return denied(Reason::Off);
    }
    if context.builtin {
        return Decision::Allow;
    }
    if heyo.is_some_and(|heyo| heyo.api_version > API_VERSION) {
        return Decision::Deny { code: ErrorCode::Unsupported, reason: Reason::Newer };
    }
    let declared = heyo.is_some_and(|heyo| listed(heyo).contains(&permission));
    // What Raycast's API offers needs no manifest, or no store extension
    // would run; it is asked about all the same.
    let through_raycast = context.route == Route::Raycast && permission.through_raycast();
    if !declared && !through_raycast {
        return denied(Reason::Undeclared);
    }
    if declared && (!permission.asks() || context.unasked) {
        return Decision::Allow;
    }
    if granted.contains(&permission) {
        return Decision::Allow;
    }
    if context.background {
        return denied(Reason::Background);
    }
    if context.refused {
        return denied(Reason::Refused);
    }
    Decision::Ask
}

/// What the extension is told when it is refused.
fn refusal(reason: Reason, title: &str, need: Need) -> String {
    let permission = need.permission;
    match reason {
        Reason::Newer => format!("{title} was written for a newer Heyo (this one provides version {API_VERSION} of the extension API)."),
        Reason::Undeclared => format!("{} needs the \"{}\" permission; add it to \"heyo.permissions\" in package.json.", need.member, permission.name()),
        Reason::Background => format!("{title} has not been allowed to {}. Open one of its commands in Heyo to be asked.", permission.allows()),
        Reason::Refused => format!("{title} was not allowed to {}.", permission.allows()),
        Reason::Off => format!("{title} is not allowed to {}: it is turned off in Heyo Settings.", permission.allows()),
    }
}

/// What the palette asks. It says what is given and what is not.
fn consent(title: &str, permission: Permission) -> Consent {
    Consent {
        title: format!("Allow {title} to {}?", permission.allows()),
        message: "Heyo will not ask again; you can take this back in Settings, on the extension's page.\n\n\
                  This is about what Heyo hands over or does for the extension. It does not confine it: an extension is a program on this computer, and can read your files and start other programs by itself."
            .into(),
        yes: "Allow",
        no: "Deny",
    }
}

// --- What the user has allowed ---------------------------------------------

/// The permissions the user has allowed, by the name of the extension.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Grants(BTreeMap<String, BTreeSet<Permission>>);

impl Grants {
    /// Reads the file's text. What cannot be read grants nothing.
    fn read(text: &str) -> Grants {
        let stored: BTreeMap<String, Vec<String>> = serde_json::from_str(text).unwrap_or_default();
        Grants(
            stored
                .into_iter()
                .map(|(extension, names)| (extension, names.iter().filter_map(|name| Permission::named(name)).collect::<BTreeSet<_>>()))
                .filter(|(_, permissions)| !permissions.is_empty())
                .collect(),
        )
    }

    fn text(&self) -> String {
        let stored: BTreeMap<&str, Vec<&str>> = self.0.iter().map(|(extension, permissions)| (extension.as_str(), permissions.iter().map(|p| p.name()).collect())).collect();
        serde_json::to_string_pretty(&stored).expect("names are plain text")
    }

    pub fn of(&self, extension: &str) -> BTreeSet<Permission> {
        self.0.get(extension).cloned().unwrap_or_default()
    }

    fn allow(&mut self, extension: &str, permission: Permission) {
        self.0.entry(extension.to_owned()).or_default().insert(permission);
    }

    fn revoke(&mut self, extension: &str, permission: Permission) {
        if let Some(permissions) = self.0.get_mut(extension) {
            permissions.remove(&permission);
            if permissions.is_empty() {
                self.0.remove(extension);
            }
        }
    }

    fn forget(&mut self, extension: &str) {
        self.0.remove(extension);
    }
}

fn load(path: &Path) -> Grants {
    std::fs::read_to_string(path).map(|text| Grants::read(&text)).unwrap_or_default()
}

/// Writes the grants where only the user can read them, whole or not at all.
fn save(path: &Path, grants: &Grants) -> Result<(), String> {
    use std::io::Write;
    let failed = |e: std::io::Error| format!("could not save {}: {e}", path.display());
    std::fs::create_dir_all(path.parent().ok_or("the grants have no directory")?).map_err(failed)?;
    let tmp = path.with_extension("json.tmp");
    // A file left from before would keep the mode it had.
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(&tmp).and_then(|mut file| file.write_all(grants.text().as_bytes())).map_err(failed)?;
    std::fs::rename(&tmp, path).map_err(failed)
}

/// What asking goes by, beside the manifest: what the user has allowed, what
/// they refused a moment ago, and the one question at a time.
pub(crate) struct Granted {
    /// Where the grants are kept. A test's are kept nowhere.
    file: fn() -> Option<PathBuf>,
    /// The grants as last read or written; the file is this process's alone.
    grants: Mutex<Option<Grants>>,
    /// One question at a time: the palette has one dialog, and the second of
    /// two requests for the same thing is answered by the first's answer.
    asking: Mutex<()>,
    refused: Mutex<Vec<(String, Permission, Instant)>>,
}

/// The user's own, kept with the configuration.
pub(crate) static GRANTED: Granted = Granted::kept(|| Some(crate::paths::permissions()));

/// Takes back all that an extension was allowed: it is being uninstalled,
/// and another of its name is not it.
pub(crate) fn forget(extension: &str) {
    if let Err(e) = GRANTED.change(|grants| grants.forget(extension)) {
        eprintln!("heyo: {e}");
    }
}

// --- Asking -------------------------------------------------------------------

/// The extension a request comes from, as far as deciding goes.
#[derive(Debug, Clone)]
pub(crate) struct Caller {
    /// The manifest's `name`.
    pub name: String,
    pub title: String,
    pub heyo: Option<Heyo>,
    pub builtin: bool,
    pub background: bool,
    pub route: Route,
    /// What the index calls the extension: its directory, or `@heyo`.
    pub location: String,
}

/// For how long a No holds. An extension that asks again at once (on every
/// render, say) must not put the question back up as fast as it is refused.
const REFUSAL: Duration = Duration::from_secs(60);

/// How long an answer is waited for before the question counts as left.
const PATIENCE: Duration = Duration::from_secs(300);

/// Puts a question up in the palette, bringing it forward for as long as
/// the question takes if it was away, and waits for the answer.
pub(crate) fn ask(app: &AppHandle, consent: Consent) -> bool {
    crate::palette::up_during(app, || {
        let (answered, answer) = mpsc::channel();
        confirm::ask_consent(app, consent, move |yes| {
            let _ = answered.send(yes);
        });
        answer.recv_timeout(PATIENCE).unwrap_or(false)
    })
}

impl Granted {
    pub(crate) const fn kept(file: fn() -> Option<PathBuf>) -> Granted {
        Granted { file, grants: Mutex::new(None), asking: Mutex::new(()), refused: Mutex::new(Vec::new()) }
    }

    fn with_grants<T>(&self, f: impl FnOnce(&mut Grants) -> T) -> T {
        let mut grants = self.grants.lock().unwrap_or_else(|e| e.into_inner());
        f(grants.get_or_insert_with(|| (self.file)().map(|path| load(&path)).unwrap_or_default()))
    }

    fn change(&self, f: impl FnOnce(&mut Grants)) -> Result<(), String> {
        self.with_grants(|grants| {
            let before = grants.clone();
            f(grants);
            if *grants == before {
                return Ok(());
            }
            match (self.file)() {
                Some(path) => save(&path, grants).inspect_err(|_| *grants = before),
                None => Ok(()),
            }
        })
    }

    fn refused_lately(&self, extension: &str, permission: Permission) -> bool {
        let mut refused = self.refused.lock().unwrap_or_else(|e| e.into_inner());
        refused.retain(|(_, _, at)| at.elapsed() < REFUSAL);
        refused.iter().any(|(name, which, _)| name == extension && *which == permission)
    }

    fn decision(&self, caller: &Caller, need: Need, switched_off: bool) -> Decision {
        let context = Context {
            builtin: caller.builtin,
            background: caller.background,
            route: caller.route,
            unasked: need.unasked,
            refused: self.refused_lately(&caller.name, need.permission),
            switched_off,
        };
        decide(caller.heyo.as_ref(), need.permission, &self.with_grants(|grants| grants.of(&caller.name)), context)
    }

    /// Makes sure the caller may have what `need` names, asking the user
    /// (`ask`) where that is theirs to say. `switched_off`: the settings
    /// have it off for this caller. Blocks while the question is up: call
    /// it off the thread that reads the runtime's messages.
    pub(crate) fn check(&self, caller: &Caller, need: Need, switched_off: bool, ask: &dyn Fn(Consent) -> bool) -> Result<(), ExtError> {
        let settle = |decision| match decision {
            Decision::Allow => Some(Ok(())),
            Decision::Deny { code, reason } => Some(Err(ExtError::new(code, refusal(reason, &caller.title, need)))),
            Decision::Ask => None,
        };
        if let Some(settled) = settle(self.decision(caller, need, switched_off)) {
            return settled;
        }
        let _one = self.asking.lock().unwrap_or_else(|e| e.into_inner());
        // It may have been answered while this request waited its turn.
        if let Some(settled) = settle(self.decision(caller, need, switched_off)) {
            return settled;
        }
        if !ask(consent(&caller.title, need.permission)) {
            self.refused.lock().unwrap_or_else(|e| e.into_inner()).push((caller.name.clone(), need.permission, Instant::now()));
            return Err(ExtError::new(ErrorCode::Denied, refusal(Reason::Refused, &caller.title, need)));
        }
        // Allowed now whether or not it could be written down.
        if let Err(e) = self.change(|grants| grants.allow(&caller.name, need.permission)) {
            eprintln!("heyo: {e}");
        }
        Ok(())
    }

    /// Has the user confirm something that is asked every time (restarting
    /// the computer), whoever asks and whatever they were allowed.
    pub(crate) fn confirm_each_time(&self, caller: &Caller, title: String, yes: &'static str, ask: &dyn Fn(Consent) -> bool) -> Result<(), ExtError> {
        if caller.background {
            return Err(ExtError::new(ErrorCode::Denied, format!("{title} Nobody can be asked from the menu bar or the background.")));
        }
        let _one = self.asking.lock().unwrap_or_else(|e| e.into_inner());
        let message = if caller.builtin { String::new() } else { format!("{} asks for it.", caller.title) };
        match ask(Consent { title, message, yes, no: "Cancel" }) {
            true => Ok(()),
            false => Err(ExtError::new(ErrorCode::Cancelled, "It was called off.")),
        }
    }
}

// --- For the settings ---------------------------------------------------------

/// A permission of an extension, as its page in the settings shows it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "PermissionRow")]
pub struct Row {
    pub permission: Permission,
    /// What it lets the extension do, after "allow it to".
    #[ts(type = "string")]
    pub allows: &'static str,
    /// `allowed` by the user (and theirs to take back), `asks` when first
    /// used, or `free`: listing it is enough.
    #[ts(type = "\"allowed\" | \"asks\" | \"free\"")]
    pub state: &'static str,
}

/// What an extension lists and what it was allowed.
fn rows(heyo: Option<&Heyo>, granted: &BTreeSet<Permission>) -> Vec<Row> {
    let declared = heyo.map(listed).unwrap_or_default();
    Permission::ALL
        .into_iter()
        .filter(|permission| declared.contains(permission) || granted.contains(permission))
        .map(|permission| Row {
            permission,
            allows: permission.allows(),
            state: match () {
                _ if granted.contains(&permission) => "allowed",
                _ if permission.asks() => "asks",
                _ => "free",
            },
        })
        .collect()
}

/// The permissions of the extension installed in `id`.
pub(crate) fn extension_permissions(id: String) -> Vec<Row> {
    let Some(declared) = crate::extensions::declared(Path::new(&id)) else { return Vec::new() };
    rows(declared.heyo.as_ref(), &GRANTED.with_grants(|grants| grants.of(&declared.name)))
}

/// Takes back what an extension was allowed; it asks again when it next needs it.
pub(crate) fn revoke_extension_permission(extension: String, permission: Permission) -> Result<(), String> {
    GRANTED.change(|grants| grants.revoke(&extension, permission))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use Permission::*;

    fn heyo(permissions: &[&str]) -> Heyo {
        Heyo { api_version: 1, permissions: permissions.iter().map(|p| (*p).to_owned()).collect(), platforms: None }
    }

    const DENIED: fn(Reason) -> Decision = |reason| Decision::Deny { code: ErrorCode::Denied, reason };

    #[test]
    fn a_permission_is_decided_by_the_manifest_the_grants_and_the_circumstances() {
        let palette = Context::default();
        let background = Context { background: true, ..palette };
        let raycast = Context { route: Route::Raycast, ..palette };
        let by_hand = Context { unasked: true, ..palette };
        let refused = Context { refused: true, ..palette };
        let builtin = Context { builtin: true, background: true, ..palette };
        let off = Context { switched_off: true, ..palette };
        let none: &[Permission] = &[];
        let lists = |permissions: &[&str]| Some(heyo(permissions));

        // (manifest, permission, granted, context) -> decision
        let cases: &[(Option<Heyo>, Permission, &[Permission], Context, Decision)] = &[
            // Listed and the user's to give: asked once, then allowed.
            (lists(&["windows"]), Windows, none, palette, Decision::Ask),
            (lists(&["windows"]), Windows, &[Windows], palette, Decision::Allow),
            (lists(&["windows"]), Windows, &[Windows], background, Decision::Allow),
            (lists(&["microphone", "memory.read"]), Microphone, none, palette, Decision::Ask),
            (lists(&["microphone", "memory.read"]), MemoryRead, &[Microphone], palette, Decision::Ask),
            (lists(&["clipboard.history"]), ClipboardHistory, none, palette, Decision::Ask),
            (lists(&["system"]), System, none, palette, Decision::Ask),
            // Nobody is there to ask in the background.
            (lists(&["windows"]), Windows, none, background, DENIED(Reason::Background)),
            (lists(&["microphone"]), Microphone, &[Windows], background, DENIED(Reason::Background)),
            // A No holds for a while.
            (lists(&["windows"]), Windows, none, refused, DENIED(Reason::Refused)),
            (lists(&["windows"]), Windows, &[Windows], refused, Decision::Allow),
            // Not listed: refused without asking, whatever was allowed before.
            (lists(&["windows"]), Microphone, none, palette, DENIED(Reason::Undeclared)),
            (lists(&[]), ClipboardHistory, &[ClipboardHistory], palette, DENIED(Reason::Undeclared)),
            (lists(&["memory.read"]), ClipboardHistory, none, palette, DENIED(Reason::Undeclared)),
            (None, MemoryRead, none, palette, DENIED(Reason::Undeclared)),
            (None, Ai, none, palette, DENIED(Reason::Undeclared)),
            (None, ScreenCapture, none, by_hand, DENIED(Reason::Undeclared)),
            // Listing is enough where nothing of the user's is handed over.
            (lists(&["screen.text", "files.search", "ai", "input.paste"]), ScreenText, none, palette, Decision::Allow),
            (lists(&["screen.text", "files.search", "ai", "input.paste"]), FilesSearch, none, background, Decision::Allow),
            (lists(&["screen.text", "files.search", "ai", "input.paste"]), Ai, none, refused, Decision::Allow),
            (lists(&["screen.text", "files.search", "ai", "input.paste"]), InputPaste, none, palette, Decision::Allow),
            // The screen: an area the user selects is theirs to give by
            // selecting it; anything else is asked about.
            (lists(&["screen.capture"]), ScreenCapture, none, by_hand, Decision::Allow),
            (lists(&["screen.capture"]), ScreenCapture, none, Context { background: true, ..by_hand }, Decision::Allow),
            (lists(&["screen.capture"]), ScreenCapture, none, palette, Decision::Ask),
            (lists(&["screen.capture"]), ScreenCapture, &[ScreenCapture], palette, Decision::Allow),
            // A store extension reaches windows through Raycast's API: asked
            // the first time, like any other, and only for that.
            (None, Windows, none, raycast, Decision::Ask),
            (None, Windows, &[Windows], raycast, Decision::Allow),
            (None, Windows, none, Context { background: true, ..raycast }, DENIED(Reason::Background)),
            (None, Windows, none, Context { refused: true, ..raycast }, DENIED(Reason::Refused)),
            (None, Windows, none, palette, DENIED(Reason::Undeclared)),
            (None, ClipboardHistory, none, raycast, DENIED(Reason::Undeclared)),
            (lists(&["memory.read"]), Windows, none, raycast, Decision::Ask),
            // "Unasked" is for what was listed, not a way round the question.
            (None, Windows, none, Context { unasked: true, ..raycast }, Decision::Ask),
            // Live activities: listing is enough, in the background too; a
            // switch in the settings turns them off, for Heyo's own as well.
            (lists(&["activity"]), Activity, none, palette, Decision::Allow),
            (lists(&["activity"]), Activity, none, background, Decision::Allow),
            (lists(&["activity"]), Activity, none, Context { unasked: true, ..background }, Decision::Allow),
            (lists(&["windows"]), Activity, none, palette, DENIED(Reason::Undeclared)),
            (None, Activity, none, raycast, DENIED(Reason::Undeclared)),
            (lists(&["activity"]), Activity, none, off, DENIED(Reason::Off)),
            (lists(&["activity"]), Activity, &[Activity], off, DENIED(Reason::Off)),
            (None, Activity, none, Context { switched_off: true, ..builtin }, DENIED(Reason::Off)),
            // Heyo's own commands are Heyo.
            (None, Microphone, none, builtin, Decision::Allow),
            (None, System, none, builtin, Decision::Allow),
            // An extension for a newer Heyo may expect what this one lacks.
            (Some(Heyo { api_version: 2, ..heyo(&["windows"]) }), Windows, &[Windows], palette, Decision::Deny { code: ErrorCode::Unsupported, reason: Reason::Newer }),
        ];
        for (manifest, permission, granted, context, expected) in cases {
            let granted: BTreeSet<Permission> = granted.iter().copied().collect();
            assert_eq!(decide(manifest.as_ref(), *permission, &granted, *context), *expected, "{manifest:?} {permission:?} {granted:?} {context:?}");
        }
    }

    #[test]
    fn only_what_comes_from_no_directory_under_heyos_name_is_heyo() {
        assert_eq!(identity("@heyo", ""), Identity::Heyo);
        // An extension installed somewhere whose manifest says "@heyo".
        for dir in ["/home/u/.local/share/heyo/extensions/@heyo", "/x/tiles", ".", " "] {
            assert_eq!(identity("@heyo", dir), Identity::Pretender, "{dir:?}");
        }
        // Any other name is an extension's, to be checked against the
        // manifest where it says it is; saying it is nowhere gains nothing.
        for (name, dir) in [("tiles", "/x/tiles"), ("tiles", ""), ("", ""), ("@Heyo", ""), ("@heyo ", ""), ("heyo", "")] {
            assert_eq!(identity(name, dir), Identity::Extension, "{name:?} {dir:?}");
        }

        // What being Heyo gives: everything, unasked, in the background too,
        // with no manifest and no grants. An extension in the same
        // circumstances is given nothing.
        let nothing = BTreeSet::new();
        for permission in Permission::ALL {
            for background in [false, true] {
                let own = Context { builtin: true, background, refused: true, ..Context::default() };
                assert_eq!(decide(None, permission, &nothing, own), Decision::Allow, "{permission:?}");
                let other = Context { builtin: false, ..own };
                assert!(matches!(decide(None, permission, &nothing, other), Decision::Deny { .. }), "{permission:?}");
            }
        }
    }

    #[test]
    fn nothing_is_allowed_to_an_extension_that_lists_nothing() {
        for permission in Permission::ALL {
            for heyo in [None, Some(heyo(&[]))] {
                for unasked in [false, true] {
                    let decision = decide(heyo.as_ref(), permission, &BTreeSet::new(), Context { unasked, ..Context::default() });
                    assert_eq!(decision, DENIED(Reason::Undeclared), "{permission:?}");
                }
            }
        }
    }

    #[test]
    fn a_permission_is_known_by_the_name_manifests_use() {
        for permission in Permission::ALL {
            assert_eq!(Permission::named(permission.name()), Some(permission));
            assert_eq!(serde_json::to_value(permission).unwrap(), permission.name());
            assert_eq!(serde_json::from_value::<Permission>(permission.name().into()).unwrap(), permission);
            assert!(!permission.allows().is_empty());
        }
        assert_eq!(Permission::named("everything"), None);
        // What a manifest lists that this Heyo does not know is passed over.
        assert_eq!(listed(&heyo(&["windows", "teleport", "ai"])), BTreeSet::from([Windows, Ai]));
    }

    /// One of every request as the runtime writes it; more where what is
    /// asked for changes what it needs.
    fn requests() -> Vec<Value> {
        let rect = json!({ "x": 0, "y": 0, "width": 1, "height": 1 });
        vec![
            json!({ "method": "ext.unsubscribe", "params": { "subscription": 1 } }),
            json!({ "method": "ext.supported", "params": { "module": "windows" } }),
            json!({ "method": "ext.windows.list" }),
            json!({ "method": "ext.windows.active" }),
            json!({ "method": "ext.windows.activate", "params": { "id": 1 } }),
            json!({ "method": "ext.windows.close", "params": { "id": 1 } }),
            json!({ "method": "ext.windows.arrange", "params": { "id": 1, "layout": "center" } }),
            json!({ "method": "ext.windows.setBounds", "params": { "id": 1, "bounds": rect } }),
            json!({ "method": "ext.windows.monitors" }),
            json!({ "method": "ext.frontmostApplication" }),
            json!({ "method": "ext.screen.capture", "params": { "target": "selection" } }),
            json!({ "method": "ext.screen.capture", "params": { "target": "screen" } }),
            json!({ "method": "ext.screen.capture", "params": { "target": { "window": 1 } } }),
            json!({ "method": "ext.screen.capture", "params": { "target": { "rect": rect } } }),
            json!({ "method": "ext.screen.recognizeText", "params": { "path": "a.png" } }),
            json!({ "method": "ext.dictation.installed" }),
            json!({ "method": "ext.dictation.listen", "params": {} }),
            json!({ "method": "ext.dictation.stop", "params": { "subscription": 1 } }),
            json!({ "method": "ext.memory.search", "params": { "byMeaning": true } }),
            json!({ "method": "ext.memory.get", "params": { "id": "clipboard:1" } }),
            json!({ "method": "ext.clipboardHistory.copy", "params": { "id": "clipboard:1" } }),
            json!({ "method": "ext.files.search", "params": { "query": "a" } }),
            json!({ "method": "ext.system.run", "params": { "action": "lock-screen" } }),
            json!({ "method": "ext.system.run", "params": { "action": "restart" } }),
            json!({ "method": "ext.system.appearance" }),
            json!({ "method": "ext.system.notify", "params": { "title": "a" } }),
            json!({ "method": "ext.system.audioDevices" }),
            json!({ "method": "ext.system.useAudioDevice", "params": { "kind": "input", "name": "mic" } }),
            json!({ "method": "ext.clipboard.paste", "params": { "text": "a" } }),
            json!({ "method": "ext.ai.provider" }),
            json!({ "method": "ext.activity.start", "params": { "attributes": { "id": "a", "name": "Build", "icon": "hammer-16" }, "content": { "title": "Building" }, "seq": 1 } }),
            json!({ "method": "ext.activity.update", "params": { "id": "a", "content": {}, "seq": 2 } }),
            json!({ "method": "ext.activity.end", "params": { "id": "a" } }),
            json!({ "method": "ext.activity.list" }),
            json!({ "method": "ext.activity.get", "params": { "id": "a" } }),
            json!({ "method": "ext.activity.presentation" }),
            json!({ "method": "ext.activity.onEvent" }),
        ]
    }

    fn need(message: Value) -> Option<Need> {
        needed(&ExtRequest::read(message.clone()).unwrap_or_else(|e| panic!("{message}: {e}")))
    }

    /// Every method that was sampled, with the permission a request of it
    /// needs and the function it is asked through.
    fn needs() -> BTreeMap<String, Option<(Permission, &'static str)>> {
        let mut needs = BTreeMap::new();
        for message in requests() {
            let method = message["method"].as_str().unwrap().to_owned();
            let need = need(message).map(|need| (need.permission, need.member));
            // Whatever is asked for with it, a method is one function's and takes one permission.
            assert_eq!(*needs.entry(method.clone()).or_insert(need), need, "{method}");
        }
        needs
    }

    #[test]
    fn every_request_the_wire_declares_says_what_it_needs() {
        use ts_rs::TS;
        // The methods, out of the type as the runtime is given it.
        let declared = ExtRequest::inline(&ts_rs::Config::default());
        let declared: BTreeSet<&str> = declared.split("\"method\": \"").skip(1).filter_map(|rest| rest.split('"').next()).collect();
        assert!(declared.len() > 30, "{declared:?}");
        let needs = needs();
        assert_eq!(needs.keys().map(String::as_str).collect::<BTreeSet<_>>(), declared);

        // Asking what is possible, and ending what one started, is free.
        let free: Vec<&str> = needs.iter().filter(|(_, need)| need.is_none()).map(|(method, _)| method.as_str()).collect();
        assert_eq!(free, ["ext.activity.presentation", "ext.dictation.installed", "ext.dictation.stop", "ext.supported", "ext.unsubscribe"]);
        // The rest name the function an author called, which is one of a
        // module but for what Raycast's API reaches.
        for (method, need) in &needs {
            let Some((_, member)) = need else { continue };
            assert!(member.contains('.') || method == "ext.frontmostApplication", "{method}: {member}");
            assert!(!member.starts_with("ext."), "{method}: {member}");
        }
    }

    /// Writes what every request needs for the runtime and the developer
    /// tools to read, next to the generated types.
    #[test]
    fn export_needs() {
        let mut out = String::from("// Generated from src-tauri/src/permissions.rs by `cargo test`. Do not edit.\n\n");
        out.push_str("/** The permissions there are, as a manifest lists them. */\nexport const permissions = [");
        out.push_str(&Permission::ALL.map(|permission| format!("{:?}", permission.name())).join(", "));
        out.push_str("] as const;\n\n");
        out.push_str("/**\n * What each request of an extension's needs before it is answered: the\n * permission, and the function of the API it is asked through, which is\n * what a refusal names. Null where it needs nothing.\n */\nexport const needs = {\n");
        for (method, need) in needs() {
            match need {
                Some((permission, member)) => out.push_str(&format!("  {method:?}: {{ permission: {:?}, member: {member:?} }},\n", permission.name())),
                None => out.push_str(&format!("  {method:?}: null,\n")),
            }
        }
        out.push_str("} as const;\n");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../shared/bindings/needs.ts");
        if std::fs::read_to_string(path).ok().as_deref() != Some(&out) {
            std::fs::write(path, out).unwrap();
        }
    }

    #[test]
    fn what_the_user_does_by_hand_or_gives_nothing_away_is_not_asked_about() {
        let rect = json!({ "x": 0, "y": 0, "width": 1, "height": 1 });
        let of = |method: &str, params: Value| need(json!({ "method": method, "params": params })).map(|need| (need.permission, need.unasked));
        // An area the user selects is theirs to give by selecting it.
        assert_eq!(of("ext.screen.capture", json!({ "target": "selection" })), Some((ScreenCapture, true)));
        for target in [json!("screen"), json!({ "window": 1 }), json!({ "rect": rect })] {
            assert_eq!(of("ext.screen.capture", json!({ "target": target })), Some((ScreenCapture, false)));
        }
        // A notification, and whether the desktop is dark.
        assert_eq!(of("ext.system.appearance", Value::Null), Some((System, true)));
        assert_eq!(of("ext.system.notify", json!({ "title": "a" })), Some((System, true)));
        for action in ["lock-screen", "restart"] {
            assert_eq!(of("ext.system.run", json!({ "action": action })), Some((System, false)));
        }
        assert_eq!(of("ext.system.audioDevices", Value::Null), Some((System, false)));
        // Live activities have a switch in the settings instead.
        for message in requests() {
            let method = message["method"].as_str().unwrap().to_owned();
            if let Some(need) = need(message).filter(|need| need.permission == Activity) {
                assert!(need.unasked, "{method}");
            }
        }
        assert_eq!(of("ext.windows.list", Value::Null), Some((Windows, false)));
        assert_eq!(of("ext.memory.search", json!({})), Some((MemoryRead, false)));
        assert_eq!(of("ext.dictation.listen", json!({})), Some((Microphone, false)));
    }

    #[test]
    fn a_refusal_tells_the_author_what_to_do() {
        let of = |method: &str, params: Value| need(json!({ "method": method, "params": params })).unwrap();
        let list = of("ext.windows.list", Value::Null);
        assert_eq!(
            refusal(Reason::Undeclared, "Window Sessions", list),
            "Windows.list needs the \"windows\" permission; add it to \"heyo.permissions\" in package.json."
        );
        // By the name the author wrote, which the method's is not always.
        let start = of("ext.activity.start", json!({ "attributes": { "id": "a", "name": "Build", "icon": "hammer-16" }, "content": { "title": "Building" }, "seq": 1 }));
        assert_eq!(
            refusal(Reason::Undeclared, "CI", start),
            "LiveActivity.start needs the \"activity\" permission; add it to \"heyo.permissions\" in package.json."
        );
        assert_eq!(of("ext.clipboardHistory.copy", json!({ "id": "clipboard:1" })).member, "ClipboardHistory.copy");
        assert_eq!(of("ext.ai.provider", Value::Null).member, "AI.provider");
        assert_eq!(of("ext.frontmostApplication", Value::Null).member, "getFrontmostApplication");
        let background = refusal(Reason::Background, "Tiles", list);
        assert!(background.starts_with("Tiles has not been allowed to see, switch, move and close the windows"));
        assert!(refusal(Reason::Newer, "Tiles", list).contains("newer Heyo"));
        assert!(refusal(Reason::Off, "Tiles", start).contains("turned off in Heyo Settings"));
    }

    #[test]
    fn the_question_names_the_extension_and_what_it_would_be_given() {
        let asked = consent("Window Sessions", Windows);
        assert_eq!(asked.title, "Allow Window Sessions to see, switch, move and close the windows of other applications?");
        assert_eq!((asked.yes, asked.no), ("Allow", "Deny"));
        // It says what the answer does not do, and never promises confinement.
        assert!(asked.message.contains("does not confine it"));
        for word in ["sandbox", "isolat", "secure", "safe"] {
            assert!(!asked.message.to_lowercase().contains(word), "{word}");
            assert!(Permission::ALL.iter().all(|p| !p.allows().contains(word)), "{word}");
        }
    }

    #[test]
    fn grants_are_kept_by_extension_and_taken_back_one_by_one() {
        let mut grants = Grants::default();
        grants.allow("tiles", Windows);
        grants.allow("tiles", Microphone);
        grants.allow("notes", ClipboardHistory);
        assert_eq!(grants.of("tiles"), BTreeSet::from([Windows, Microphone]));
        assert_eq!(grants.of("weather"), BTreeSet::new());

        // What is written is read back the same.
        let text = grants.text();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&text).unwrap(), serde_json::json!({ "notes": ["clipboard.history"], "tiles": ["windows", "microphone"] }));
        assert_eq!(Grants::read(&text), grants);

        grants.revoke("tiles", Windows);
        assert_eq!(grants.of("tiles"), BTreeSet::from([Microphone]));
        grants.revoke("tiles", Microphone);
        grants.revoke("tiles", Microphone);
        grants.forget("notes");
        assert_eq!(grants, Grants::default());
    }

    #[test]
    fn a_file_that_cannot_be_read_grants_nothing() {
        assert_eq!(Grants::read("not json"), Grants::default());
        assert_eq!(Grants::read(r#"{ "tiles": "windows" }"#), Grants::default());
        // A permission of a newer Heyo is not one of this one's.
        let read = Grants::read(r#"{ "tiles": ["windows", "teleport"], "odd": ["teleport"] }"#);
        assert_eq!(read.of("tiles"), BTreeSet::from([Windows]));
        assert_eq!(read.of("odd"), BTreeSet::new());
        assert_eq!(read.0.len(), 1);
    }

    #[test]
    fn the_grants_are_saved_for_the_user_alone() {
        let dir = std::env::temp_dir().join(format!("heyo-permissions-test-{}", std::process::id()));
        let path = dir.join("config").join("permissions.json");
        assert_eq!(load(&path), Grants::default());
        let mut grants = Grants::default();
        grants.allow("tiles", Windows);
        save(&path, &grants).unwrap();
        grants.allow("tiles", System);
        save(&path, &grants).unwrap();
        assert_eq!(load(&path), grants);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_settings_show_what_is_listed_and_what_was_allowed() {
        let manifest = heyo(&["windows", "screen.text", "microphone"]);
        let shown = rows(Some(&manifest), &BTreeSet::from([Windows]));
        let states: Vec<(Permission, &str)> = shown.iter().map(|row| (row.permission, row.state)).collect();
        assert_eq!(states, [(Windows, "allowed"), (ScreenText, "free"), (Microphone, "asks")]);
        // A store extension shows what it was allowed through Raycast's API.
        assert_eq!(rows(None, &BTreeSet::from([Windows])).len(), 1);
        assert_eq!(rows(None, &BTreeSet::new()), []);
        assert_eq!(serde_json::to_value(&shown[0]).unwrap()["permission"], "windows");
    }
}
