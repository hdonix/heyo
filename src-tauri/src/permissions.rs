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
use std::path::Path;
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
}

impl Permission {
    pub const ALL: [Permission; 12] = [
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
        }
    }

    pub fn named(name: &str) -> Option<Permission> {
        Permission::ALL.into_iter().find(|permission| permission.name() == name)
    }

    /// Whether it is the user's to give, the first time it is used. The
    /// rest only work on what the extension itself supplies, or do what
    /// Raycast's API lets any extension do.
    fn asks(self) -> bool {
        !matches!(self, Permission::ScreenText | Permission::FilesSearch | Permission::InputPaste | Permission::Ai)
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
}

/// The permission a request needs, if any.
pub fn needed(request: &ExtRequest) -> Option<Need> {
    use ExtRequest::*;
    let asked = |permission| Some(Need { permission, unasked: false });
    let unasked = |permission| Some(Need { permission, unasked: true });
    match request {
        // Asking what is possible, and ending what one started, is free.
        Unsubscribe { .. } | Supported { .. } | DictationInstalled | DictationStop { .. } => None,
        WindowsList | WindowsActive | WindowsActivate { .. } | WindowsClose { .. } | WindowsArrange { .. } | WindowsSetBounds { .. } | WindowsMonitors | FrontmostApplication => {
            asked(Permission::Windows)
        }
        ScreenCapture { target: CaptureTarget::Whole(CaptureWhole::Selection) } => unasked(Permission::ScreenCapture),
        ScreenCapture { .. } => asked(Permission::ScreenCapture),
        ScreenRecognizeText { .. } => asked(Permission::ScreenText),
        DictationListen { .. } => asked(Permission::Microphone),
        MemorySearch { .. } | MemoryGet { .. } => asked(Permission::MemoryRead),
        ClipboardHistorySearch { .. } | ClipboardHistoryCopy { .. } => asked(Permission::ClipboardHistory),
        FilesSearch { .. } => asked(Permission::FilesSearch),
        SystemRun { .. } | SystemAudioDevices | SystemUseAudioDevice { .. } => asked(Permission::System),
        SystemAppearance | SystemNotify { .. } => unasked(Permission::System),
        ClipboardPaste { .. } => asked(Permission::InputPaste),
        AiProvider => asked(Permission::Ai),
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

/// The function of `@heyo/api` behind a request, for telling its author.
fn interface(method: &str) -> String {
    let path = method.strip_prefix("ext.").unwrap_or(method);
    match path.split_once('.') {
        Some(("ai", function)) => format!("AI.{function}"),
        Some((module, function)) => {
            let mut letters = module.chars();
            let first = letters.next().map(|c| c.to_ascii_uppercase()).unwrap_or_default();
            format!("{first}{}.{function}", letters.as_str())
        }
        None => path.to_owned(),
    }
}

/// What the extension is told when it is refused.
fn refusal(reason: Reason, title: &str, permission: Permission, method: &str) -> String {
    match reason {
        Reason::Newer => format!("{title} was written for a newer Heyo (this one provides version {API_VERSION} of the extension API)."),
        Reason::Undeclared => {
            format!("{} needs the \"{}\" permission; add it to \"heyo.permissions\" in package.json.", interface(method), permission.name())
        }
        Reason::Background => format!("{title} has not been allowed to {}. Open one of its commands in Heyo to be asked.", permission.allows()),
        Reason::Refused => format!("{title} was not allowed to {}.", permission.allows()),
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

/// The grants as last read or written; the file is this process's alone.
static GRANTS: Mutex<Option<Grants>> = Mutex::new(None);

fn with_grants<T>(f: impl FnOnce(&mut Grants) -> T) -> T {
    let mut grants = GRANTS.lock().unwrap_or_else(|e| e.into_inner());
    f(grants.get_or_insert_with(|| load(&crate::paths::permissions())))
}

fn change(f: impl FnOnce(&mut Grants)) -> Result<(), String> {
    with_grants(|grants| {
        let before = grants.clone();
        f(grants);
        if *grants == before {
            return Ok(());
        }
        save(&crate::paths::permissions(), grants).inspect_err(|_| *grants = before)
    })
}

/// Takes back all that an extension was allowed: it is being uninstalled,
/// and another of its name is not it.
pub(crate) fn forget(extension: &str) {
    if let Err(e) = change(|grants| grants.forget(extension)) {
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
}

/// One question at a time: the palette has one dialog, and the second of
/// two requests for the same thing is answered by the first's answer.
static ASKING: Mutex<()> = Mutex::new(());

/// For how long a No holds. An extension that asks again at once (on every
/// render, say) must not put the question back up as fast as it is refused.
const REFUSAL: Duration = Duration::from_secs(60);
static REFUSED: Mutex<Vec<(String, Permission, Instant)>> = Mutex::new(Vec::new());

/// How long an answer is waited for before the question counts as left.
const PATIENCE: Duration = Duration::from_secs(300);

fn refused_lately(extension: &str, permission: Permission) -> bool {
    let mut refused = REFUSED.lock().unwrap_or_else(|e| e.into_inner());
    refused.retain(|(_, _, at)| at.elapsed() < REFUSAL);
    refused.iter().any(|(name, which, _)| name == extension && *which == permission)
}

fn decision(caller: &Caller, need: Need) -> Decision {
    let context = Context {
        builtin: caller.builtin,
        background: caller.background,
        route: caller.route,
        unasked: need.unasked,
        refused: refused_lately(&caller.name, need.permission),
    };
    decide(caller.heyo.as_ref(), need.permission, &with_grants(|grants| grants.of(&caller.name)), context)
}

/// Puts a question up in the palette, bringing it forward for as long as
/// the question takes if it was away, and waits for the answer.
fn ask(app: &AppHandle, consent: Consent) -> bool {
    let window = crate::main_window(app);
    let away = !window.is_visible().unwrap_or(true);
    if away {
        let _ = window.show();
        window.focus();
    }
    let (answered, answer) = mpsc::channel();
    confirm::ask_consent(app, consent, move |yes| {
        let _ = answered.send(yes);
    });
    let yes = answer.recv_timeout(PATIENCE).unwrap_or(false);
    if away {
        let _ = window.hide();
    }
    yes
}

/// Makes sure the caller may have what `need` names, asking the user where
/// that is theirs to say. Blocks while the question is up: call it off the
/// thread that reads the runtime's messages.
pub(crate) fn check(app: &AppHandle, caller: &Caller, need: Need, method: &str) -> Result<(), ExtError> {
    let settle = |decision| match decision {
        Decision::Allow => Some(Ok(())),
        Decision::Deny { code, reason } => Some(Err(ExtError::new(code, refusal(reason, &caller.title, need.permission, method)))),
        Decision::Ask => None,
    };
    if let Some(settled) = settle(decision(caller, need)) {
        return settled;
    }
    let _one = ASKING.lock().unwrap_or_else(|e| e.into_inner());
    // It may have been answered while this request waited its turn.
    if let Some(settled) = settle(decision(caller, need)) {
        return settled;
    }
    if !ask(app, consent(&caller.title, need.permission)) {
        REFUSED.lock().unwrap_or_else(|e| e.into_inner()).push((caller.name.clone(), need.permission, Instant::now()));
        return Err(ExtError::new(ErrorCode::Denied, refusal(Reason::Refused, &caller.title, need.permission, method)));
    }
    // Allowed now whether or not it could be written down.
    if let Err(e) = change(|grants| grants.allow(&caller.name, need.permission)) {
        eprintln!("heyo: {e}");
    }
    Ok(())
}

/// Has the user confirm something that is asked every time (restarting the
/// computer), whoever asks and whatever they were allowed.
pub(crate) fn confirm_each_time(app: &AppHandle, caller: &Caller, title: String, yes: &'static str) -> Result<(), ExtError> {
    if caller.background {
        return Err(ExtError::new(ErrorCode::Denied, format!("{title} Nobody can be asked from the menu bar or the background.")));
    }
    let _one = ASKING.lock().unwrap_or_else(|e| e.into_inner());
    let message = if caller.builtin { String::new() } else { format!("{} asks for it.", caller.title) };
    match ask(app, Consent { title, message, yes, no: "Cancel" }) {
        true => Ok(()),
        false => Err(ExtError::new(ErrorCode::Cancelled, "It was called off.")),
    }
}

// --- For the settings ---------------------------------------------------------

/// A permission of an extension, as its page in the settings shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub permission: Permission,
    /// What it lets the extension do, after "allow it to".
    pub allows: &'static str,
    /// `allowed` by the user (and theirs to take back), `asks` when first
    /// used, or `free`: listing it is enough.
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
#[tauri::command(async)]
pub(crate) fn extension_permissions(id: String) -> Vec<Row> {
    let Some(declared) = crate::extensions::declared(Path::new(&id)) else { return Vec::new() };
    rows(declared.heyo.as_ref(), &with_grants(|grants| grants.of(&declared.name)))
}

/// Takes back what an extension was allowed; it asks again when it next needs it.
#[tauri::command(async)]
pub(crate) fn revoke_extension_permission(extension: String, permission: Permission) -> Result<(), String> {
    change(|grants| grants.revoke(&extension, permission))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::WindowLayout;
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

    #[test]
    fn every_request_says_what_it_needs() {
        use crate::platform::system::Action;
        use crate::platform::windows::Rect;
        use ExtRequest as R;
        let rect = Rect { x: 0, y: 0, width: 1, height: 1 };
        let asked = |permission| Some(Need { permission, unasked: false });
        let unasked = |permission| Some(Need { permission, unasked: true });
        let cases = [
            (R::Unsubscribe { subscription: 1 }, None),
            (R::Supported { module: crate::wire::Module::Windows }, None),
            (R::WindowsList, asked(Windows)),
            (R::WindowsActive, asked(Windows)),
            (R::WindowsActivate { id: 1 }, asked(Windows)),
            (R::WindowsClose { id: 1 }, asked(Windows)),
            (R::WindowsArrange { id: 1, layout: WindowLayout::Named("center".into()) }, asked(Windows)),
            (R::WindowsSetBounds { id: 1, bounds: rect }, asked(Windows)),
            (R::WindowsMonitors, asked(Windows)),
            (R::FrontmostApplication, asked(Windows)),
            (R::ScreenCapture { target: CaptureTarget::Whole(CaptureWhole::Selection) }, unasked(ScreenCapture)),
            (R::ScreenCapture { target: CaptureTarget::Whole(CaptureWhole::Screen) }, asked(ScreenCapture)),
            (R::ScreenCapture { target: CaptureTarget::Window { window: 1 } }, asked(ScreenCapture)),
            (R::ScreenCapture { target: CaptureTarget::Rect { rect } }, asked(ScreenCapture)),
            (R::ScreenRecognizeText { path: "a.png".into() }, asked(ScreenText)),
            (R::DictationInstalled, None),
            (R::DictationListen { language: None, indicator: None }, asked(Microphone)),
            (R::DictationStop { subscription: 1 }, None),
            (R::MemorySearch { text: None, sources: None, kinds: None, since: None, until: None, limit: None, by_meaning: Some(true) }, asked(MemoryRead)),
            (R::MemoryGet { id: "clipboard:1".into() }, asked(MemoryRead)),
            (R::ClipboardHistorySearch { text: None, kind: None, limit: None }, asked(ClipboardHistory)),
            (R::ClipboardHistoryCopy { id: "clipboard:1".into() }, asked(ClipboardHistory)),
            (R::FilesSearch { query: "a".into(), limit: None }, asked(FilesSearch)),
            (R::SystemRun { action: Action::LockScreen }, asked(System)),
            (R::SystemRun { action: Action::Restart }, asked(System)),
            (R::SystemAudioDevices, asked(System)),
            (R::SystemUseAudioDevice { kind: crate::platform::system::AudioKind::Input, name: "mic".into() }, asked(System)),
            (R::SystemAppearance, unasked(System)),
            (R::SystemNotify { title: "a".into(), body: None }, unasked(System)),
            (R::ClipboardPaste { text: "a".into(), html: None }, asked(InputPaste)),
            (R::AiProvider, asked(Ai)),
        ];
        for (request, expected) in cases {
            assert_eq!(needed(&request), expected, "{request:?}");
        }
    }

    #[test]
    fn a_refusal_tells_the_author_what_to_do() {
        assert_eq!(
            refusal(Reason::Undeclared, "Window Sessions", Windows, "ext.windows.list"),
            "Windows.list needs the \"windows\" permission; add it to \"heyo.permissions\" in package.json."
        );
        assert_eq!(interface("ext.clipboardHistory.search"), "ClipboardHistory.search");
        assert_eq!(interface("ext.ai.provider"), "AI.provider");
        assert_eq!(interface("ext.frontmostApplication"), "frontmostApplication");
        let background = refusal(Reason::Background, "Tiles", Windows, "ext.windows.list");
        assert!(background.starts_with("Tiles has not been allowed to see, switch, move and close the windows"));
        assert!(refusal(Reason::Newer, "Tiles", Windows, "ext.windows.list").contains("newer Heyo"));
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
