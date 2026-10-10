//! The messages that cross between Heyo's three tiers: the host (this
//! process), the extension runtime (Node.js) and the webview.
//!
//! Each is declared here once. `cargo test` writes the TypeScript side into
//! `shared/bindings/`, so a message one tier sends is one the other knows.
//! What the runtime tells the webview passes through the host unread; those
//! messages are declared where both ends can see them, in `shared/wire.ts`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Declares the events the host emits to its windows, and lists them for the
/// TypeScript side.
macro_rules! events {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        /// The events the host emits to its windows.
        pub mod event {
            $($(#[$doc])* pub const $name: &str = $value;)*
        }
        #[cfg(test)]
        const EVENTS: &[(&str, &str)] = &[$((stringify!($name), $value)),*];
    };
}

events! {
    /// The palette came up.
    SHOWN = "heyo://shown";
    /// The items of the root search changed.
    INDEX_READY = "heyo://index-ready";
    /// The settings changed; carries the new `SettingsView`.
    SETTINGS_CHANGED = "heyo://settings-changed";
    /// Open a quicklink or snippet by id (`heyo --open`).
    OPEN_ITEM = "heyo://open-item";
    /// A message from the extension runtime (`RuntimeMessage`).
    RUNTIME = "heyo://runtime";
    /// The state of a dictation, for its window.
    DICTATION = "heyo://dictation";
    /// Progress of installing the speech engine.
    DICTATION_INSTALL = "heyo://dictation-install";
    /// Progress of installing the text-recognition engine.
    OCR_INSTALL = "heyo://ocr-install";
    /// A capture is ready for the screenshot overlay.
    SCREENSHOT = "heyo://screenshot";
    /// A snippet's keyword was typed in another application: the palette
    /// fills in the text's placeholders and answers with `snippet_expanded`.
    EXPAND_SNIPPET = "heyo://expand-snippet";
    /// Progress of downloading what finds clipboard entries by meaning.
    CLIPBOARD_MEANING_INSTALL = "heyo://clipboard-meaning-install";
    /// The live activities changed; carries the ordered `ActivityView[]`.
    ACTIVITIES = "heyo://activities";
}

/// What the extension runtime asks of the host for itself: what Raycast's API
/// is made of (the clipboard, opening things, the palette), and what is
/// Heyo's own business (its index of extensions, the settings of the model,
/// the keeping of the clipboard history). Whatever an extension could ask
/// for too is not here but in `ExtRequest`, and Heyo's own commands ask for
/// it the same way.
#[derive(Debug, PartialEq, Deserialize, TS)]
#[serde(tag = "method", content = "params")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum HostRequest {
    #[serde(rename = "host.hide")]
    Hide,
    /// Brings the palette back without resetting it to the root search.
    #[serde(rename = "host.show")]
    Show,
    #[serde(rename = "host.clipboard.write")]
    ClipboardWrite {
        text: String,
        #[ts(optional)]
        html: Option<String>,
        /// An extension copying a secret says so; it stays out of the history.
        #[serde(default)]
        #[ts(optional)]
        concealed: Option<bool>,
    },
    #[serde(rename = "host.clipboard.read")]
    ClipboardRead,
    /// Reads the installed extensions again.
    #[serde(rename = "host.reload")]
    Reload,
    #[serde(rename = "host.applications")]
    Applications,
    /// After the palette hides, into whatever window has the focus.
    #[serde(rename = "host.paste")]
    Paste,
    #[serde(rename = "host.selection")]
    Selection,
    #[serde(rename = "host.files.refresh")]
    FilesRefresh,
    #[serde(rename = "host.ai")]
    Ai,
    /// Tells the user something in passing, the palette being gone.
    #[serde(rename = "host.hud")]
    Hud { text: String },
    /// Opens a file, folder or URL, with `application` when given.
    #[serde(rename = "host.open")]
    Open {
        target: String,
        #[ts(optional)]
        application: Option<String>,
    },
    #[serde(rename = "host.trash")]
    Trash { paths: Vec<String> },
    /// The id of the application that opens `target` by default, or null.
    #[serde(rename = "host.defaultApplication")]
    DefaultApplication { target: String },
    /// Whether copies are recorded, whether that is paused, and how far
    /// finding by meaning has read the history (`{ recording, paused, meaning? }`).
    #[serde(rename = "host.clipboardHistory.status")]
    ClipboardHistoryStatus,
    /// Notes that searching for `query` led to using the entry at `rank`,
    /// which is what the search is measured by.
    #[serde(rename = "host.clipboardHistory.chose")]
    ClipboardHistoryChose {
        #[ts(type = "number")]
        id: u64,
        query: String,
        rank: u32,
    },
    #[serde(rename = "host.clipboardHistory.pin")]
    ClipboardHistoryPin {
        #[ts(type = "number")]
        id: u64,
        pinned: bool,
    },
    #[serde(rename = "host.clipboardHistory.delete")]
    ClipboardHistoryDelete {
        #[ts(type = "number")]
        id: u64,
    },
    #[serde(rename = "host.clipboardHistory.clear")]
    ClipboardHistoryClear,
    /// Stops recording copies, or resumes it.
    #[serde(rename = "host.clipboardHistory.pause")]
    ClipboardHistoryPause { paused: bool },
    /// `launchCommand` for a command that runs in the other runtime process:
    /// in the menu bar or the background when `background`, else in the palette.
    #[serde(rename = "host.launchCommand")]
    LaunchCommand {
        /// The extension's directory.
        extension: String,
        command: String,
        background: bool,
        #[serde(default)]
        #[ts(optional, type = "Record<string, string>")]
        arguments: Option<Value>,
        #[serde(default)]
        #[ts(optional, type = "unknown")]
        context: Option<Value>,
        #[serde(default, rename = "fallbackText")]
        #[ts(optional)]
        fallback_text: Option<String>,
    },
    /// The user's processes, the busiest first (`ProcessInfo[]`).
    #[serde(rename = "host.processes.list")]
    Processes,
    /// Asks a process to quit; with `force`, ends it without asking.
    #[serde(rename = "host.processes.end")]
    ProcessEnd { pid: u32, force: bool },
}

impl HostRequest {
    /// Whether answering hides or shows the palette, which only the command
    /// in the palette may ask for.
    pub fn moves_palette(&self) -> bool {
        matches!(self, HostRequest::Hide | HostRequest::Show)
    }
}

/// What the host tells the extension runtime.
#[derive(Debug, Serialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case", rename_all_fields = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum HostMessage {
    /// Starts a command in the palette.
    Launch {
        extension: String,
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        fallback_text: Option<String>,
        /// What `launchCommand` gave, when another command started this one.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "Record<string, string>")]
        arguments: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "unknown")]
        launch_context: Option<Value>,
    },
    /// A sign-in an extension started came back from the browser.
    OauthCallback { url: String },
    /// Runs a menu bar or background command.
    BackgroundRun {
        #[ts(type = "number")]
        id: u64,
        extension: String,
        command: String,
        #[ts(type = "\"userInitiated\" | \"background\"")]
        launch_type: &'static str,
        /// The path of the menu item chosen, when one was.
        action: Option<Vec<usize>>,
        /// What `launchCommand` gave, when another command started this one.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "Record<string, string>")]
        arguments: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "unknown")]
        launch_context: Option<Value>,
    },
    /// `heyo dev` built the extension in this directory anew (or took it
    /// out): a command of it that is on screen starts again from its files.
    DevReload { extension: String },
    /// The interface went light or dark (`environment.appearance`).
    Appearance { dark: bool },
    /// Something happened that a command subscribed to with an `ext.*`
    /// request (a sentence was recognized, say).
    Event {
        #[ts(type = "number")]
        subscription: u64,
        #[ts(type = "unknown")]
        data: Value,
    },
}

impl HostMessage {
    pub fn launch(extension: &str, command: &str, fallback_text: Option<String>) -> Value {
        HostMessage::Launch { extension: extension.into(), command: command.into(), fallback_text, arguments: None, launch_context: None }.json()
    }

    pub fn json(&self) -> Value {
        serde_json::to_value(self).expect("a host message is plain data")
    }
}

/// What the webview may tell the runtime: input for the command on screen.
/// Starting a command is not among them; that goes through the `launch`
/// command, which only runs commands from the index.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum WebviewMessage {
    /// Calls a function prop of a node, such as `onAction`.
    Dispatch {
        session: u32,
        node: u32,
        prop: String,
        #[ts(type = "unknown[]")]
        args: Vec<Value>,
    },
    /// Leaves the view on top.
    Pop { session: u32 },
    /// Ends a command; without a session, whichever is running.
    Close {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        session: Option<u32>,
    },
    /// Answers a request the runtime made of the webview.
    Reply {
        id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "unknown")]
        result: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        error: Option<String>,
    },
    ToastAction {
        id: u32,
        #[ts(type = "\"primary\" | \"secondary\"")]
        action: String,
    },
    /// A link in rendered content was clicked.
    Open { target: String },
}

// --- What extensions ask (`@heyo/api`) ---------------------------------------
//
// Apart from `host.*`, which is the runtime's own and may change freely:
// these are what a command written against `@heyo/api` asks for, Heyo's own
// commands among them (docs/extension-api.md). Each request names the
// extension it comes from
// (`ExtCaller`), is checked against what that extension may do
// (`permissions.rs`) and answered in `ext.rs`; what goes wrong comes back as
// an `ExtError` rather than as a sentence.

/// Why an `ext.*` request failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum ErrorCode {
    /// This session cannot do it.
    Unsupported,
    /// The user has not allowed it.
    Denied,
    /// The user called it off.
    Cancelled,
    /// Something it needs is not installed.
    Unavailable,
    /// The request itself was wrong.
    Invalid,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct ExtError {
    pub code: ErrorCode,
    pub message: String,
}

impl ExtError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> ExtError {
        ExtError { code, message: message.into() }
    }

    pub fn invalid(message: impl Into<String>) -> ExtError {
        ExtError::new(ErrorCode::Invalid, message)
    }

    pub fn unsupported(message: impl Into<String>) -> ExtError {
        ExtError::new(ErrorCode::Unsupported, message)
    }
}

/// What the desktop says went wrong is passed on as it is.
impl From<String> for ExtError {
    fn from(message: String) -> ExtError {
        ExtError::new(ErrorCode::Failed, message)
    }
}

impl From<&str> for ExtError {
    fn from(message: &str) -> ExtError {
        ExtError::new(ErrorCode::Failed, message)
    }
}

/// How a request reached the host: through `@heyo/api`, or through one of
/// the functions Raycast's API has for the same thing (`WindowManagement`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum Route {
    #[default]
    Heyo,
    Raycast,
}

/// Who asks: beside the method and its parameters, every `ext.*` request
/// carries these. The runtime writes them from the context of the command
/// that is running, never from anything the extension passed.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct ExtCaller {
    /// The `name` of the extension's manifest; `@heyo` for Heyo's own commands.
    pub extension: String,
    /// Where the extension is installed, for its manifest to be read. Heyo's
    /// own commands are installed nowhere: theirs is empty.
    #[serde(default)]
    pub dir: String,
    #[serde(default)]
    #[ts(optional)]
    pub via: Option<Route>,
}

/// A part of the API, to ask whether this session supports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum Module {
    Windows,
    Screen,
    Dictation,
    Memory,
    ClipboardHistory,
    Files,
    System,
    Ai,
    Activity,
}

/// Where to put a window: one of the named layouts, or a part of the work
/// area in twelfths (x, y, width, height).
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(untagged)]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum WindowLayout {
    Named(
        #[ts(
            type = "\"left-half\" | \"right-half\" | \"top-half\" | \"bottom-half\" | \"top-left\" | \"top-right\" | \"bottom-left\" | \"bottom-right\" | \"first-third\" | \"center-third\" | \"last-third\" | \"first-two-thirds\" | \"last-two-thirds\" | \"almost-maximize\" | \"maximize\" | \"center\" | \"next-display\""
        )]
        String,
    ),
    Fraction { fraction: [i32; 4] },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum CaptureWhole {
    /// An area the user selects.
    Selection,
    /// The monitor the pointer is on.
    Screen,
}

/// What of the screen to take a picture of.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(untagged)]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum CaptureTarget {
    Whole(CaptureWhole),
    Window { window: u32 },
    Rect { rect: crate::platform::windows::Rect },
}

/// A window of another application, as extensions are told of it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct ExtWindow {
    pub id: u32,
    pub title: String,
    /// The application's window class, e.g. "firefox".
    pub class: String,
    /// Whether it is the window the user is working in.
    pub active: bool,
    /// Physical pixels; absent where the desktop does not say.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bounds: Option<crate::platform::windows::Rect>,
    /// Which of `monitors()` it is on.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub monitor: Option<u32>,
}

/// A picture of the screen: a PNG file in the extension's cache directory.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Capture {
    pub path: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct RecognizedText {
    /// All of it, in reading order.
    pub text: String,
}

/// Something Heyo remembers. Today that is what was copied.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct MemoryItem {
    /// The source and the entry's number there, e.g. `clipboard:412`.
    pub id: String,
    #[ts(type = "\"clipboard\"")]
    pub source: &'static str,
    pub kind: crate::clipboard::Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
    /// The PNG file of an image.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub image: Option<String>,
    /// The size of an image, in pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub height: Option<u32>,
    /// The window class of the application it came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub app: Option<String>,
    /// That application's name (the window class when nothing installed has
    /// it) and the file of its icon.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub app_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub app_icon: Option<String>,
    /// When it was last copied: milliseconds since the Unix epoch.
    #[ts(type = "number")]
    pub at: u64,
    /// When it was first copied, and how many times it has been: the
    /// clipboard history counts; another source may not.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub first_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub times_copied: Option<u32>,
    pub pinned: bool,
    /// It was found by what it means: the words asked for are not in it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub by_meaning: Option<bool>,
}

/// How a query was read, to show what is being searched for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Understood {
    pub terms: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub app: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<crate::clipboard::Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub from: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub to: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct MemoryResult {
    pub items: Vec<MemoryItem>,
    pub understood: Understood,
    /// What was read out of the query matched nothing, and the query was
    /// taken as plain words instead.
    pub relaxed: bool,
}

/// Which model answers `AI.ask`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct AiProvider {
    #[ts(type = "\"claude\" | \"local\"")]
    pub kind: &'static str,
    /// The local model's name as set, empty for the first the server lists.
    /// (Which Claude model is the runtime's to say.)
    pub model: String,
    /// Whether what is asked stays on this computer.
    pub local: bool,
}

/// The answer to a request that subscribes: events with this number follow
/// (`HostMessage::Event`) until `ext.unsubscribe`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Subscription {
    #[ts(type = "number")]
    pub subscription: u64,
}

/// What an extension asks of the host.
#[derive(Debug, PartialEq, Deserialize, TS)]
#[serde(tag = "method", content = "params", rename_all_fields = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum ExtRequest {
    /// Ends a subscription; what it was for stops (a recording is discarded).
    #[serde(rename = "ext.unsubscribe")]
    Unsubscribe {
        #[ts(type = "number")]
        subscription: u64,
    },
    /// How well this session supports a part of the API (`Support`).
    #[serde(rename = "ext.supported")]
    Supported { module: Module },

    /// The windows of other applications, the most recently used first.
    #[serde(rename = "ext.windows.list")]
    WindowsList,
    /// The window the user is working in, or null.
    #[serde(rename = "ext.windows.active")]
    WindowsActive,
    #[serde(rename = "ext.windows.activate")]
    WindowsActivate { id: u32 },
    #[serde(rename = "ext.windows.close")]
    WindowsClose { id: u32 },
    #[serde(rename = "ext.windows.arrange")]
    WindowsArrange { id: u32, layout: WindowLayout },
    /// Puts a window's frame exactly there (physical pixels).
    #[serde(rename = "ext.windows.setBounds")]
    WindowsSetBounds { id: u32, bounds: crate::platform::windows::Rect },
    #[serde(rename = "ext.windows.monitors")]
    WindowsMonitors,
    /// The application of the window the user is working in, or null.
    #[serde(rename = "ext.frontmostApplication")]
    FrontmostApplication,

    #[serde(rename = "ext.screen.capture")]
    ScreenCapture { target: CaptureTarget },
    /// The text in an image file.
    #[serde(rename = "ext.screen.recognizeText")]
    ScreenRecognizeText { path: String },

    /// Whether the speech engine has been downloaded.
    #[serde(rename = "ext.dictation.installed")]
    DictationInstalled,
    /// Starts recording; subscribes to `{ kind: "segment" | "result", text }`
    /// and `{ kind: "error", code, message }`. Nothing is pasted.
    #[serde(rename = "ext.dictation.listen")]
    DictationListen {
        #[serde(default)]
        #[ts(optional)]
        language: Option<String>,
        /// Whether Heyo's dictation window shows; it does unless false.
        #[serde(default)]
        #[ts(optional)]
        indicator: Option<bool>,
    },
    /// Ends the recording; its text comes as the `result` event.
    #[serde(rename = "ext.dictation.stop")]
    DictationStop {
        #[ts(type = "number")]
        subscription: u64,
    },

    #[serde(rename = "ext.memory.search")]
    MemorySearch {
        #[serde(default)]
        #[ts(optional)]
        text: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        sources: Option<Vec<String>>,
        #[serde(default)]
        #[ts(optional)]
        kinds: Option<Vec<crate::clipboard::Kind>>,
        /// Milliseconds since the Unix epoch.
        #[serde(default)]
        #[ts(optional, type = "number")]
        since: Option<u64>,
        #[serde(default)]
        #[ts(optional, type = "number")]
        until: Option<u64>,
        #[serde(default)]
        #[ts(optional)]
        limit: Option<u32>,
        /// Also what the text means, where the user has that turned on. The
        /// answer waits for it, a second or two the first time.
        #[serde(default)]
        #[ts(optional)]
        by_meaning: Option<bool>,
    },
    /// One item by its id, or null.
    #[serde(rename = "ext.memory.get")]
    MemoryGet { id: String },
    /// Puts an entry back on the clipboard.
    #[serde(rename = "ext.clipboardHistory.copy")]
    ClipboardHistoryCopy { id: String },

    /// Paths only; nothing is read. With how many files there are to search.
    #[serde(rename = "ext.files.search")]
    FilesSearch {
        query: String,
        #[serde(default)]
        #[ts(optional)]
        limit: Option<u32>,
    },

    /// Answers with what to tell the user, or null.
    #[serde(rename = "ext.system.run")]
    SystemRun { action: crate::platform::system::Action },
    /// `"light"` or `"dark"`.
    #[serde(rename = "ext.system.appearance")]
    SystemAppearance,
    #[serde(rename = "ext.system.notify")]
    SystemNotify {
        title: String,
        #[serde(default)]
        #[ts(optional)]
        body: Option<String>,
    },

    /// The speakers and microphones (`AudioDevice[]`).
    #[serde(rename = "ext.system.audioDevices")]
    SystemAudioDevices,
    /// Makes a device the one sound goes through.
    #[serde(rename = "ext.system.useAudioDevice")]
    SystemUseAudioDevice { kind: crate::platform::system::AudioKind, name: String },

    /// `Clipboard.paste` that puts back what was on the clipboard before.
    #[serde(rename = "ext.clipboard.paste")]
    ClipboardPaste {
        text: String,
        #[serde(default)]
        #[ts(optional)]
        html: Option<String>,
    },

    #[serde(rename = "ext.ai.provider")]
    AiProvider,

    /// Starts a live activity, or updates the one of that id
    /// (docs/live-activities.md, 6.1). `launchType` is that of the command
    /// asking, which the runtime writes: only a command the user ran may
    /// start one (6.7). `seq` numbers the content (6.2).
    #[serde(rename = "ext.activity.start")]
    ActivityStart {
        attributes: crate::activities::Attributes,
        content: crate::activities::Content,
        #[serde(default)]
        #[ts(optional)]
        announce: Option<bool>,
        #[serde(default)]
        #[ts(optional, type = "\"userInitiated\" | \"background\"")]
        launch_type: Option<String>,
        #[ts(type = "number")]
        seq: u64,
    },
    #[serde(rename = "ext.activity.update")]
    ActivityUpdate {
        id: String,
        content: crate::activities::Content,
        #[serde(default)]
        #[ts(optional)]
        alert: Option<crate::activities::Alert>,
        #[ts(type = "number")]
        seq: u64,
    },
    #[serde(rename = "ext.activity.end")]
    ActivityEnd {
        id: String,
        #[serde(default, rename = "final")]
        #[ts(optional, rename = "final")]
        final_content: Option<crate::activities::Content>,
        #[serde(default)]
        #[ts(optional, type = "\"none\" | \"brief\"")]
        keep: Option<String>,
    },
    /// The ids of the extension's activities in progress.
    #[serde(rename = "ext.activity.list")]
    ActivityList,
    /// Whether that one is in progress.
    #[serde(rename = "ext.activity.get")]
    ActivityGet { id: String },
    /// How activities show on this desktop (`Presentation`).
    #[serde(rename = "ext.activity.presentation")]
    ActivityPresentation,
    /// Subscribes to the events of the extension's activities:
    /// `{ kind: "action" | "dismissed" | "expired" | "elapsed", id, … }`.
    #[serde(rename = "ext.activity.onEvent")]
    ActivityOnEvent,
}

/// How activities are shown on this desktop, for an extension to fit its
/// content to (6.1 `presentation()`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Presentation {
    /// Of the status bar. The palette's "In Progress" section is always there.
    pub support: crate::platform::Support,
    pub compact_characters: u32,
    pub expanded: bool,
    pub buttons: bool,
    pub version: u32,
}

impl ExtRequest {
    /// Reads a request out of the runtime's message. A request that takes
    /// no parameters may come with none or with an empty set of them.
    pub fn read(mut message: Value) -> Result<ExtRequest, serde_json::Error> {
        serde_json::from_value(message.clone()).or_else(|error| {
            if !message["params"].as_object().is_some_and(|params| params.is_empty()) {
                return Err(error);
            }
            message.as_object_mut().map(|message| message.remove("params"));
            serde_json::from_value(message).map_err(|_| error)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(value: Value) -> Result<HostRequest, serde_json::Error> {
        serde_json::from_value(value)
    }

    #[test]
    fn a_request_is_read_by_its_method() {
        assert_eq!(request(json!({ "method": "host.hide" })).unwrap(), HostRequest::Hide);
        assert_eq!(request(json!({ "method": "host.hide", "params": null })).unwrap(), HostRequest::Hide);
        assert_eq!(
            request(json!({ "type": "request", "id": 3, "method": "host.clipboardHistory.delete", "params": { "id": 7 } })).unwrap(),
            HostRequest::ClipboardHistoryDelete { id: 7 }
        );
        assert_eq!(
            request(json!({ "method": "host.clipboard.write", "params": { "text": "a" } })).unwrap(),
            HostRequest::ClipboardWrite { text: "a".into(), html: None, concealed: None }
        );
    }

    #[test]
    fn only_the_command_in_the_palette_moves_it() {
        assert!(HostRequest::Hide.moves_palette());
        assert!(HostRequest::Show.moves_palette());
        assert!(!HostRequest::Hud { text: "done".into() }.moves_palette());
        assert!(!HostRequest::ClipboardRead.moves_palette());
    }

    #[test]
    fn a_command_asks_for_another_to_be_launched() {
        assert_eq!(
            request(json!({ "method": "host.launchCommand", "params": { "extension": "/x/weather", "command": "now", "background": true } })).unwrap(),
            HostRequest::LaunchCommand { extension: "/x/weather".into(), command: "now".into(), background: true, arguments: None, context: None, fallback_text: None }
        );
        let run = HostMessage::BackgroundRun {
            id: 1,
            extension: "/x/weather".into(),
            command: "now".into(),
            launch_type: "background",
            action: None,
            arguments: Some(json!({ "city": "Oslo" })),
            launch_context: None,
        };
        assert_eq!(run.json()["arguments"]["city"], "Oslo");
        assert!(run.json().get("launchContext").is_none());
    }

    #[test]
    fn a_request_the_host_does_not_know_is_refused() {
        assert!(request(json!({ "method": "host.format-disk" })).is_err());
        assert!(request(json!({ "method": "host.clipboardHistory.delete", "params": {} })).is_err());
    }

    #[test]
    fn what_an_extension_could_ask_for_is_not_the_runtimes_own() {
        // These went to `ext.*` when Heyo's own commands took to `@heyo/api`.
        for gone in [
            "host.windows.list",
            "host.windows.activate",
            "host.windows.close",
            "host.windows.active",
            "host.windows.setBounds",
            "host.windows.maximize",
            "host.windows.monitors",
            "host.frontmostApplication",
            "host.files.search",
            "host.system",
            "host.audio.devices",
            "host.audio.use",
            "host.clipboardHistory.search",
            "host.clipboardHistory.searchByMeaning",
            "host.clipboardHistory.copy",
        ] {
            let with = |params: Value| request(json!({ "method": gone, "params": params }));
            for params in [json!(null), json!({}), json!({ "id": 1 }), json!({ "query": "a" }), json!({ "action": "sleep" })] {
                assert!(with(params).is_err(), "{gone}");
            }
        }
        // What is left of the clipboard history is the keeping of it.
        assert_eq!(request(json!({ "method": "host.clipboardHistory.status" })).unwrap(), HostRequest::ClipboardHistoryStatus);
        assert_eq!(
            request(json!({ "method": "host.clipboardHistory.chose", "params": { "id": 4, "query": "invoice", "rank": 2 } })).unwrap(),
            HostRequest::ClipboardHistoryChose { id: 4, query: "invoice".into(), rank: 2 }
        );
    }

    #[test]
    fn the_webview_cannot_start_a_command() {
        let launch = json!({ "type": "launch", "extension": "x", "command": "y" });
        assert!(serde_json::from_value::<WebviewMessage>(launch).is_err());
    }

    #[test]
    fn a_relayed_message_keeps_its_shape() {
        for message in [
            json!({ "type": "dispatch", "session": 1, "node": 2, "prop": "onAction", "args": [{ "$date": 5 }] }),
            json!({ "type": "close" }),
            json!({ "type": "reply", "id": 4, "result": false }),
            json!({ "type": "toast-action", "id": 1, "action": "primary" }),
        ] {
            let read: WebviewMessage = serde_json::from_value(message.clone()).unwrap();
            assert_eq!(serde_json::to_value(read).unwrap(), message);
        }
    }

    #[test]
    fn a_launch_names_its_command() {
        assert_eq!(
            HostMessage::launch("@heyo", "extensions", None),
            json!({ "type": "launch", "extension": "@heyo", "command": "extensions" })
        );
        assert_eq!(HostMessage::launch("a", "b", Some("q".into()))["fallbackText"], "q");
        let arguments = Some(json!({ "city": "Oslo" }));
        let launch = HostMessage::Launch { extension: "a".into(), command: "b".into(), fallback_text: None, arguments, launch_context: None };
        assert_eq!(launch.json()["arguments"], json!({ "city": "Oslo" }));
    }

    fn ext(value: Value) -> Result<ExtRequest, serde_json::Error> {
        ExtRequest::read(value)
    }

    #[test]
    fn every_ext_request_is_read_by_its_method() {
        use crate::clipboard::Kind;
        use crate::platform::system::Action;
        use crate::platform::windows::Rect;
        let rect = Rect { x: 1, y: 2, width: 30, height: 40 };
        let cases = [
            (json!({ "method": "ext.unsubscribe", "params": { "subscription": 7 } }), ExtRequest::Unsubscribe { subscription: 7 }),
            (json!({ "method": "ext.supported", "params": { "module": "clipboardHistory" } }), ExtRequest::Supported { module: Module::ClipboardHistory }),
            (json!({ "method": "ext.windows.list", "params": {} }), ExtRequest::WindowsList),
            (json!({ "method": "ext.windows.active" }), ExtRequest::WindowsActive),
            (json!({ "method": "ext.windows.active", "params": null }), ExtRequest::WindowsActive),
            (json!({ "method": "ext.windows.activate", "params": { "id": 3 } }), ExtRequest::WindowsActivate { id: 3 }),
            (json!({ "method": "ext.windows.close", "params": { "id": 3 } }), ExtRequest::WindowsClose { id: 3 }),
            (
                json!({ "method": "ext.windows.arrange", "params": { "id": 3, "layout": "left-half" } }),
                ExtRequest::WindowsArrange { id: 3, layout: WindowLayout::Named("left-half".into()) },
            ),
            (
                json!({ "method": "ext.windows.arrange", "params": { "id": 3, "layout": { "fraction": [0, 0, 4, 12] } } }),
                ExtRequest::WindowsArrange { id: 3, layout: WindowLayout::Fraction { fraction: [0, 0, 4, 12] } },
            ),
            (
                json!({ "method": "ext.windows.setBounds", "params": { "id": 3, "bounds": { "x": 1, "y": 2, "width": 30, "height": 40 } } }),
                ExtRequest::WindowsSetBounds { id: 3, bounds: rect },
            ),
            (json!({ "method": "ext.windows.monitors" }), ExtRequest::WindowsMonitors),
            (json!({ "method": "ext.frontmostApplication" }), ExtRequest::FrontmostApplication),
            (
                json!({ "method": "ext.screen.capture", "params": { "target": "selection" } }),
                ExtRequest::ScreenCapture { target: CaptureTarget::Whole(CaptureWhole::Selection) },
            ),
            (
                json!({ "method": "ext.screen.capture", "params": { "target": "screen" } }),
                ExtRequest::ScreenCapture { target: CaptureTarget::Whole(CaptureWhole::Screen) },
            ),
            (
                json!({ "method": "ext.screen.capture", "params": { "target": { "window": 9 } } }),
                ExtRequest::ScreenCapture { target: CaptureTarget::Window { window: 9 } },
            ),
            (
                json!({ "method": "ext.screen.capture", "params": { "target": { "rect": { "x": 1, "y": 2, "width": 30, "height": 40 } } } }),
                ExtRequest::ScreenCapture { target: CaptureTarget::Rect { rect } },
            ),
            (json!({ "method": "ext.screen.recognizeText", "params": { "path": "/tmp/a.png" } }), ExtRequest::ScreenRecognizeText { path: "/tmp/a.png".into() }),
            (json!({ "method": "ext.dictation.installed" }), ExtRequest::DictationInstalled),
            (json!({ "method": "ext.dictation.listen", "params": {} }), ExtRequest::DictationListen { language: None, indicator: None }),
            (
                json!({ "method": "ext.dictation.listen", "params": { "language": "zh", "indicator": false } }),
                ExtRequest::DictationListen { language: Some("zh".into()), indicator: Some(false) },
            ),
            (json!({ "method": "ext.dictation.stop", "params": { "subscription": 2 } }), ExtRequest::DictationStop { subscription: 2 }),
            (
                json!({ "method": "ext.memory.search", "params": {} }),
                ExtRequest::MemorySearch { text: None, sources: None, kinds: None, since: None, until: None, limit: None, by_meaning: None },
            ),
            (
                json!({ "method": "ext.memory.search", "params": { "text": "invoice", "sources": ["clipboard"], "kinds": ["link"], "since": 5, "until": 9, "limit": 3, "byMeaning": true } }),
                ExtRequest::MemorySearch {
                    text: Some("invoice".into()),
                    sources: Some(vec!["clipboard".into()]),
                    kinds: Some(vec![Kind::Link]),
                    since: Some(5),
                    until: Some(9),
                    limit: Some(3),
                    by_meaning: Some(true),
                },
            ),
            (json!({ "method": "ext.memory.get", "params": { "id": "clipboard:4" } }), ExtRequest::MemoryGet { id: "clipboard:4".into() }),
            (json!({ "method": "ext.clipboardHistory.copy", "params": { "id": "clipboard:4" } }), ExtRequest::ClipboardHistoryCopy { id: "clipboard:4".into() }),
            (json!({ "method": "ext.files.search", "params": { "query": "notes", "limit": 5 } }), ExtRequest::FilesSearch { query: "notes".into(), limit: Some(5) }),
            (json!({ "method": "ext.system.run", "params": { "action": "lock-screen" } }), ExtRequest::SystemRun { action: Action::LockScreen }),
            (json!({ "method": "ext.system.appearance" }), ExtRequest::SystemAppearance),
            (json!({ "method": "ext.system.notify", "params": { "title": "Done" } }), ExtRequest::SystemNotify { title: "Done".into(), body: None }),
            (json!({ "method": "ext.system.audioDevices" }), ExtRequest::SystemAudioDevices),
            (
                json!({ "method": "ext.system.useAudioDevice", "params": { "kind": "output", "name": "alsa_output.usb" } }),
                ExtRequest::SystemUseAudioDevice { kind: crate::platform::system::AudioKind::Output, name: "alsa_output.usb".into() },
            ),
            (json!({ "method": "ext.clipboard.paste", "params": { "text": "a" } }), ExtRequest::ClipboardPaste { text: "a".into(), html: None }),
            (json!({ "method": "ext.ai.provider" }), ExtRequest::AiProvider),
            (json!({ "method": "ext.activity.list" }), ExtRequest::ActivityList),
            (json!({ "method": "ext.activity.get", "params": { "id": "t" } }), ExtRequest::ActivityGet { id: "t".into() }),
            (json!({ "method": "ext.activity.presentation" }), ExtRequest::ActivityPresentation),
            (json!({ "method": "ext.activity.onEvent" }), ExtRequest::ActivityOnEvent),
            (
                json!({ "method": "ext.activity.end", "params": { "id": "t", "final": { "title": "Done" }, "keep": "none" } }),
                ExtRequest::ActivityEnd { id: "t".into(), final_content: Some(crate::activities::Content { title: Some("Done".into()), ..Default::default() }), keep: Some("none".into()) },
            ),
            (
                json!({ "method": "ext.activity.update", "params": { "id": "t", "content": { "subtitle": "half" }, "alert": { "expand": true }, "seq": 9 } }),
                ExtRequest::ActivityUpdate {
                    id: "t".into(),
                    content: crate::activities::Content { subtitle: Some("half".into()), ..Default::default() },
                    alert: Some(crate::activities::Alert { expand: Some(true) }),
                    seq: 9,
                },
            ),
        ];
        for (message, expected) in cases {
            assert_eq!(ext(message.clone()).unwrap_or_else(|e| panic!("{message}: {e}")), expected);
        }
    }

    #[test]
    fn an_activity_is_started_with_its_parts_and_who_runs_the_command() {
        use crate::activities::{Action, Body, Timer, Value as Compact};
        let message = json!({ "method": "ext.activity.start", "params": {
            "attributes": { "id": "t", "name": "Timer", "icon": "clock-16", "action": { "launch": { "command": "timer" } } },
            "content": { "title": "25 minutes", "compact": { "timer": { "start": 1, "end": 2 } }, "body": { "timer": { "start": 1, "end": 2, "pausedAt": 1 } },
                         "actions": [{ "title": "Pause", "action": { "event": "pause" } }] },
            "announce": false, "launchType": "userInitiated", "seq": 3,
        } });
        let ExtRequest::ActivityStart { attributes, content, announce, launch_type, seq } = ext(message).unwrap() else { panic!("a start") };
        assert_eq!((attributes.id.as_str(), attributes.name.as_str(), announce, launch_type.as_deref(), seq), ("t", "Timer", Some(false), Some("userInitiated"), 3));
        assert!(matches!(attributes.action, Some(Action::Launch { launch }) if launch.command == "timer"));
        let timer = Timer { start: 1, end: Some(2), paused_at: None, counts_down: None, style: None };
        assert_eq!(content.compact, Some(Compact::Timer { timer: timer.clone() }));
        assert_eq!(content.body, Some(Body::Timer { timer: Timer { paused_at: Some(1), ..timer } }));
        assert_eq!(content.actions.unwrap()[0].action, Action::Event { event: "pause".into(), data: None });
        // Without a sequence number, or with an action that is none, it is not read.
        assert!(ext(json!({ "method": "ext.activity.start", "params": { "attributes": { "id": "t", "name": "T", "icon": "x" }, "content": { "title": "T" } } })).is_err());
        assert!(ext(json!({ "method": "ext.activity.update", "params": { "id": "t", "content": { "actions": [{ "title": "X", "action": { "fly": true } }] }, "seq": 1 } })).is_err());
        let shown = serde_json::to_value(Presentation { support: crate::platform::Support::None, compact_characters: 10, expanded: false, buttons: false, version: 1 }).unwrap();
        assert_eq!(shown, json!({ "support": "none", "compactCharacters": 10, "expanded": false, "buttons": false, "version": 1 }));
    }

    #[test]
    fn an_ext_request_the_host_does_not_know_is_refused() {
        // Neither an unknown method, nor one of Heyo's own under the other name.
        assert!(ext(json!({ "method": "ext.format-disk" })).is_err());
        assert!(ext(json!({ "method": "host.hide" })).is_err());
        assert!(request(json!({ "method": "ext.windows.list" })).is_err());
        // Nor one whose parameters are missing or of the wrong kind.
        assert!(ext(json!({ "method": "ext.windows.close", "params": {} })).is_err());
        assert!(ext(json!({ "method": "ext.windows.close", "params": { "id": "3" } })).is_err());
        assert!(ext(json!({ "method": "ext.screen.capture", "params": { "target": "everything" } })).is_err());
        assert!(ext(json!({ "method": "ext.system.run", "params": { "action": "format-disk" } })).is_err());
        assert!(ext(json!({ "method": "ext.supported", "params": { "module": "kernel" } })).is_err());
        assert!(ext(json!({ "method": "ext.system.useAudioDevice", "params": { "kind": "sideways", "name": "a" } })).is_err());
    }

    #[test]
    fn a_request_says_who_asks_beside_what() {
        let message = json!({ "type": "request", "id": 41, "method": "ext.windows.list", "extension": "window-sessions", "dir": "/x/window-sessions", "params": {} });
        let caller: ExtCaller = serde_json::from_value(message.clone()).unwrap();
        assert_eq!(caller, ExtCaller { extension: "window-sessions".into(), dir: "/x/window-sessions".into(), via: None });
        assert_eq!(ext(message).unwrap(), ExtRequest::WindowsList);
        let through: ExtCaller = serde_json::from_value(json!({ "extension": "tiles", "dir": "/x/tiles", "via": "raycast" })).unwrap();
        assert_eq!(through.via, Some(Route::Raycast));
        // One that does not say who asks is nobody's.
        assert!(serde_json::from_value::<ExtCaller>(json!({ "method": "ext.windows.list" })).is_err());
    }

    #[test]
    fn an_error_and_an_event_keep_their_shapes() {
        let error = ExtError::new(ErrorCode::Denied, "Window access was not allowed.");
        assert_eq!(serde_json::to_value(error).unwrap(), json!({ "code": "denied", "message": "Window access was not allowed." }));
        assert_eq!(ExtError::from("the window is gone".to_owned()).code, ErrorCode::Failed);
        let event = HostMessage::Event { subscription: 7, data: json!({ "kind": "segment", "text": "Hello." }) };
        assert_eq!(event.json(), json!({ "type": "event", "subscription": 7, "data": { "kind": "segment", "text": "Hello." } }));
        assert_eq!(serde_json::to_value(Subscription { subscription: 7 }).unwrap(), json!({ "subscription": 7 }));
    }

    /// Writes the event names for the webview, next to the generated types.
    #[test]
    fn export_event_names() {
        let mut out = String::from("// Generated from src-tauri/src/wire.rs by `cargo test`. Do not edit.\n\n");
        out.push_str("/** The events the host emits to its windows. */\nexport const event = {\n");
        for (name, value) in EVENTS {
            let mut camel = String::new();
            let mut upper = false;
            for c in name.chars() {
                match c {
                    '_' => upper = true,
                    c if upper => {
                        camel.push(c);
                        upper = false;
                    }
                    c => camel.push(c.to_ascii_lowercase()),
                }
            }
            out.push_str(&format!("  {camel}: \"{value}\",\n"));
        }
        out.push_str("} as const;\n");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../shared/bindings/events.ts");
        if std::fs::read_to_string(path).ok().as_deref() != Some(&out) {
            std::fs::write(path, out).unwrap();
        }
    }
}
