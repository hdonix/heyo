//! The window model, through what the window does with it: built from the
//! host's own types, so that a setting renamed there fails here when it is
//! compiled.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{json, Value};

use super::*;
use crate::clipboard::{meaning, sources::Source};
use crate::commands::media::OcrStatus;
use crate::commands::settings::{HotkeyName, Hotkeys, ProviderState, SettingsView};
use crate::extensions::ExtensionInfo;
use crate::models::cli::Found;
use crate::models::kinds::Checked;
use crate::models::registry::{ModelRef, Provider};
use crate::permissions::{Permission, Row};
use crate::platform::{Capabilities, HoldKey, Support, Words};
use crate::settings::Appearance;
use crate::slint_windows::answer::{Application, DictationStatus, Microphone, SpeechModel, SpeechProgress};
use crate::slint_windows::ids::{self, Act, Setting};
use crate::slint_windows::pages::extensions::{APPLICATIONS, BUILTIN};

/// A model told these settings.
fn told(edit: impl FnOnce(&mut SettingsView)) -> Model {
    let mut model = Model::new();
    edit(&mut model.known.settings);
    model
}

/// A session that can do everything, on X11.
fn capabilities() -> Capabilities {
    let full = Support::Full;
    Capabilities { session: "x11".into(), windows: full, paste: full, selection: full, clipboard_watch: full, hotkeys: full, hold_key: full, capture: full, overlay: full, snippet_expansion: full, dictation_words: Vec::new(), hold_keys: Vec::new(), extension: "none".into() }
}

fn set(setting: Setting) -> String {
    Id::Set(setting).to_string()
}

fn settings(view: SettingsView) -> Result<Answer, String> {
    Ok(Answer::Settings(Box::new(view)))
}

fn row<'a>(lines: &'a [Line], label: &str) -> &'a Line {
    lines.iter().find(|line| line.label == label).unwrap_or_else(|| panic!("no line {label:?} among {:?}", lines.iter().map(|line| &line.label).collect::<Vec<_>>()))
}

fn has(lines: &[Line], label: &str) -> bool {
    lines.iter().any(|line| line.label == label)
}

fn titles(line: &Line) -> Vec<&str> {
    line.choices.iter().map(|(_, title)| title.as_str()).collect()
}

fn names(asks: &[Ask]) -> Vec<&'static str> {
    asks.iter().map(|ask| ask.command.name()).collect()
}

/// The one thing asked of the host.
fn asked(asks: &[Ask]) -> Command {
    assert_eq!(asks.len(), 1, "{asks:?}");
    asks[0].command.clone()
}

/// The settings the one thing asked of the host changes, and what to, as
/// the host is sent them.
fn patch(asks: &[Ask]) -> Value {
    match asked(asks) {
        Command::ChangeSettings(changes) => Value::Object(changes),
        other => panic!("{other:?} changes no settings"),
    }
}

/// The preferences the one thing asked of the host saves, and whose.
fn saved(asks: &[Ask]) -> (String, Value) {
    match asked(asks) {
        Command::SetExtensionPreferences { extension, values } => (extension, serde_json::to_value(values).unwrap()),
        other => panic!("{other:?} saves no preferences"),
    }
}

/// A page of rows, by their ids.
fn page(ids: &[&str]) -> Vec<Line> {
    ids.iter().map(|id| Line::row(*id, *id)).collect()
}

/// A card of a page: its lines, from its heading to the next.
fn card<'a>(lines: &'a [Line], title: &str) -> &'a [Line] {
    let at = lines.iter().position(|line| line.kind == "heading" && line.label == title).unwrap_or_else(|| panic!("no card {title}"));
    let end = lines[at + 2..].iter().position(|line| line.kind == "heading" || line.kind == "end" || line.first).map_or(lines.len(), |end| at + 2 + end);
    &lines[at + 1..end]
}

fn head(lines: &[Line]) -> &Line {
    lines.iter().find(|line| line.kind == "head").expect("a head")
}

#[test]
fn a_control_that_was_used_stays_the_line_it_was() {
    // A switch that was flipped: the line is told to be drawn again,
    // not taken out and put back, which would make it another switch.
    let was = page(&["a", "b", "c"]);
    let mut is = was.clone();
    is[1].on = true;
    assert_eq!(changes(&was, &is), Changes { head: 3, out: 0, come: 0, changed: vec![1] });
    // And nothing at all is said when nothing is different.
    assert_eq!(changes(&was, &was), Changes { head: 3, ..Changes::default() });
}

#[test]
fn a_group_opened_further_down_leaves_the_lines_round_it() {
    // The first line stays, so the list stays where it was scrolled to.
    let (was, is) = (page(&["a", "group", "z"]), page(&["a", "group", "one", "two", "z"]));
    assert_eq!(changes(&was, &is), Changes { head: 2, out: 0, come: 2, changed: vec![] });
    // Closed again; and the group's own line says it is closed.
    let mut closed = was.clone();
    closed[1].open = true;
    assert_eq!(changes(&is, &closed), Changes { head: 2, out: 2, come: 0, changed: vec![1] });
    // What changed after what was put in is told by its place among the new lines.
    let mut last = is.clone();
    last[4].value = "now".into();
    assert_eq!(changes(&was, &last), Changes { head: 2, out: 0, come: 2, changed: vec![4] });
}

#[test]
fn another_page_takes_the_first_line_out() {
    // So that it is not left scrolled to where the last one was.
    let (was, is) = (page(&["a", "b", "c"]), page(&["x", "y"]));
    assert_eq!(changes(&was, &is), Changes { head: 0, out: 3, come: 2, changed: vec![] });
    // Even where the two end alike: the end stays, the beginning goes.
    let (was, is) = (page(&["a", "b", "end"]), page(&["x", "end"]));
    assert_eq!(changes(&was, &is), Changes { head: 0, out: 2, come: 1, changed: vec![] });
    // From nothing, and to nothing.
    assert_eq!(changes(&[], &is), Changes { head: 0, out: 0, come: 2, changed: vec![] });
    assert_eq!(changes(&was, &[]), Changes { head: 0, out: 3, come: 0, changed: vec![] });
}

#[test]
fn a_line_is_counted_once_where_the_pages_begin_and_end_alike() {
    // One line taken out of a run of lines that are alike: no line is
    // both of the beginning that stays and of the end that stays.
    let same = |n: usize| vec![Line::row("x", "x"); n];
    assert_eq!(changes(&same(3), &same(2)), Changes { head: 2, out: 1, come: 0, changed: vec![] });
    assert_eq!(changes(&same(2), &same(3)), Changes { head: 2, out: 0, come: 1, changed: vec![] });
}

#[test]
fn general_says_what_the_settings_are() {
    let mut model = told(|view| {
        view.launch_at_login = true;
        let settings = &mut view.settings;
        (settings.appearance, settings.language) = (Appearance::Dark, "system".into());
        (settings.compact_mode, settings.hide_on_blur, settings.keep_command, settings.live_activities) = (false, true, false, true);
    });
    model.known.capabilities = Some(capabilities());
    model.known.hotkeys = Hotkeys { supported: true, toggle: Some("<Super>space".into()), ..Hotkeys::default() };
    let lines = model.lines();
    assert!(row(&lines, "Open at Login").on && row(&lines, "Hide When Focus Is Lost").on && row(&lines, "Show What Is in Progress").on);
    assert!(!row(&lines, "Compact Mode").on && !row(&lines, "Keep the Running Command").on);
    assert_eq!((row(&lines, "Theme").value.as_str(), titles(row(&lines, "Theme"))), ("dark", vec!["System", "Light", "Dark"]));
    assert_eq!((row(&lines, "Heyo Hotkey").hotkey.as_str(), row(&lines, "Heyo Hotkey").keys.clone()), ("@toggle", vec!["Super".to_string(), "Space".to_string()]));
    assert!(!has(&lines, "GNOME Shell Extension"));
    // The cards: the first row of each is its first, the last its last.
    assert!(row(&lines, "Open at Login").first && row(&lines, "Heyo Hotkey").last);
    assert_eq!(lines.iter().filter(|line| line.kind == "heading").map(|line| line.label.as_str()).collect::<Vec<_>>(), ["Appearance", "Window", "Live Activities"]);
    // The language: the system's, the sources', and every translation
    // by its own name for itself.
    assert_eq!((row(&lines, "Language").value.as_str(), titles(row(&lines, "Language"))), ("system", vec!["System", "English", "简体中文"]));
    assert!(row(&lines, "Theme").first && !row(&lines, "Theme").last && row(&lines, "Language").last);
}

#[test]
fn a_switch_is_shown_at_once_and_asked_of_the_host() {
    let mut model = told(|view| view.settings.compact_mode = false);
    // The host is sent the setting that changed, by its own name for it.
    assert_eq!(patch(&model.acted(&set(Setting::CompactMode), "true")), json!({ "compactMode": true }));
    assert!(row(&model.lines(), "Compact Mode").on);
    assert_eq!(patch(&model.acted(&set(Setting::Appearance), "light")), json!({ "appearance": "light" }));
    assert_eq!(model.known.settings.settings.appearance, Appearance::Light);
    assert_eq!(patch(&model.acted(&set(Setting::LaunchAtLogin), "true")), json!({ "launchAtLogin": true }));
    // The host's answer is what is then so; its refusal is said, and
    // the settings are read again.
    let mut answer = SettingsView::default();
    (answer.settings.compact_mode, answer.settings.appearance) = (false, Appearance::Dark);
    assert!(model.answered(Then::Settings, settings(answer)).is_empty());
    assert!(!row(&model.lines(), "Compact Mode").on);
    assert_eq!(asked(&model.answered(Then::Settings, Err("no".into()))).name(), "get_settings");
    assert_eq!(model.error, "no");
}

#[test]
fn general_offers_the_shell_extension_only_where_it_is_wanted() {
    let of = |state: &'static str| {
        let mut model = told(|_| {});
        model.known.capabilities = Some(Capabilities { extension: state.into(), ..capabilities() });
        model.lines().iter().find(|line| line.label == "GNOME Shell Extension").map(|line| (line.control, line.value.clone()))
    };
    assert_eq!(of("none"), None);
    assert_eq!(of("active"), None);
    assert_eq!(of("missing"), Some(("button", "Install".to_string())));
    assert_eq!(of("outdated"), Some(("button", "Update".to_string())));
    assert_eq!(of("pending"), Some(("", String::new())));
}

#[test]
fn a_session_without_hotkeys_has_no_recorder() {
    let mut model = told(|_| {});
    model.known.hotkeys = Hotkeys { supported: false, ..Hotkeys::default() };
    let lines = model.lines();
    assert_eq!((row(&lines, "Heyo Hotkey").hotkey.as_str(), row(&lines, "Heyo Hotkey").description.as_str()), ("", "Bind a desktop keyboard shortcut to heyo."));
}

#[test]
fn a_hotkey_is_bound_as_one_of_heyos_own_or_as_an_items() {
    let mut model = told(|_| {});
    model.known.applications = vec![Application { id: "app:files.desktop".into(), title: "Files".into(), ..Application::default() }];
    assert_eq!(asked(&model.bind("@toggle", "<Super>space")), Command::SetHotkey { name: HotkeyName::Toggle, accelerator: "<Super>space".into() });
    assert_eq!(asked(&model.bind("app:files.desktop", "<Super>e")), Command::SetItemHotkey { id: "app:files.desktop".into(), title: "Files".into(), accelerator: "<Super>e".into() });
    assert_eq!(asked(&model.bind("heyo:screenshot", "")), Command::SetItemHotkey { id: "heyo:screenshot".into(), title: "Capture Screenshot".into(), accelerator: String::new() });
    assert!(model.bind("app:gone.desktop", "<Super>g").is_empty());
    model.answered(Then::Bound("app:files.desktop".into(), "<Super>e".into()), Ok(Answer::Done));
    assert_eq!(model.known.item_hotkeys["app:files.desktop"], "<Super>e");
    model.answered(Then::Bound("app:files.desktop".into(), String::new()), Ok(Answer::Done));
    assert!(model.known.item_hotkeys.is_empty());
}

fn provider(id: &str, kind: &str, name: &str, config: &[(&str, &str)]) -> Provider {
    Provider { id: id.into(), kind: kind.into(), name: name.into(), config: config.iter().map(|(field, value)| (field.to_string(), value.to_string())).collect() }
}

/// The providers as the host tells them: Claude with its key taken out,
/// a server on this computer and one elsewhere, Claude Code found and
/// Codex not, and the speech models of this computer.
fn providers(view: &mut SettingsView) {
    view.settings.providers = vec![
        provider("local-speech", "local-speech", "Speech Models on This Computer", &[]),
        provider("claude", "anthropic", "Claude", &[("key", "")]),
        provider("ollama", "openai-compatible", "Ollama", &[("url", "http://127.0.0.1:11434/v1")]),
        provider("lan", "openai-compatible", "Workstation", &[("url", "http://10.0.0.5:8000/v1")]),
        provider("claude-code", "claude-code", "Claude Code", &[]),
        provider("codex", "codex", "Codex", &[("path", "/opt/codex")]),
    ];
    let state = |secrets: &[&str], cli_found| ProviderState { secrets_set: secrets.iter().map(|name| name.to_string()).collect(), cli_found };
    view.provider_states = BTreeMap::from([
        ("claude".into(), state(&["key"], None)),
        ("claude-code".into(), state(&[], Some(Ok(Found { path: "/home/ada/.local/bin/claude".into(), version: "2.1.302 (Claude Code)".into() })))),
        ("codex".into(), state(&[], Some(Err("There is no executable at /opt/codex.".into())))),
    ]);
    let settings = &mut view.settings;
    settings.ai_model = ModelRef::to("ollama", "qwen3");
    settings.dictation_model = ModelRef::to("local-speech", "x-asr-2026-06-05");
    settings.polish_model = ModelRef::to("claude", "claude-opus-5-5");
    settings.cases_model = ModelRef::default();
}

/// The speech models of this computer as the host says them: one in
/// use, one downloading, one not downloaded.
fn speech() -> Vec<SpeechModel> {
    let entry = |id: &str, name: &str, installed: bool, in_use: bool, downloading: Option<SpeechProgress>| SpeechModel { id: id.into(), name: name.into(), languages: "Chinese, English".into(), note: "Fast.".into(), megabytes: 240, installed, in_use, takes_language: true, downloading };
    vec![
        entry("sense-voice-2024-07-17", "SenseVoice Small", true, false, None),
        entry("x-asr-2026-06-05", "X-ASR Streaming", false, true, Some(SpeechProgress { step: "speech-model".into(), received: 67, total: 134 })),
        entry("whisper-turbo", "Whisper Turbo", false, false, None),
    ]
}

#[test]
fn dictation_says_what_this_session_can_do() {
    let mut model = told(|view| {
        providers(view);
        let settings = &mut view.settings;
        (settings.dictation_hold_key, settings.dictation_live, settings.dictation_source, settings.dictation_language) = (String::new(), true, String::new(), "zh".into());
        (settings.dictation_vocabulary, settings.dictation_polish) = (vec!["TypeScript".into(), "杰森 -> JSON".into()], "clean".into());
    });
    model.known.capabilities = Some(Capabilities { hold_keys: vec![HoldKey { value: "Control_R".into(), title: "Right Ctrl".into() }], dictation_words: vec![Words::Composed, Words::Rewritten, Words::Pasted], ..capabilities() });
    model.page = PageId::own("dictation");
    model.known.dictation = Some(DictationStatus { installed: false, installing: false, microphones: vec![Microphone { name: "alsa_input.usb".into(), description: "USB Microphone".into() }] });
    model.known.speech = speech();
    let lines = model.lines();
    // Recognition: the speech models that can be used, the one chosen not downloaded yet; no download here.
    let recognition = row(&lines, "Speech Recognition");
    assert_eq!(&titles(recognition)[..3], ["This Computer · Default", "This Computer · SenseVoice Small", "This Computer · SenseVoice (Cantonese)"]);
    assert!(recognition.danger && recognition.description.starts_with("X-ASR Streaming is not downloaded yet. Download it in Models."), "{}", recognition.description);
    assert!(lines.iter().filter(|line| line.control == "button").all(|line| line.id == "act:open-models"));
    assert_eq!(titles(row(&lines, "Hold-to-Talk Key")), ["None", "Right Ctrl"]);
    assert!(row(&lines, "Hold-to-Talk Key").description.starts_with("A single key to hold"));
    let live = row(&lines, "Type as You Speak");
    assert!(live.control == "switch" && live.on && live.description.contains("through the input method") && live.description.contains("corrected where they stand"));
    assert_eq!(titles(row(&lines, "Microphone")), ["System Default", "USB Microphone"]);
    assert!(!row(&lines, "Microphone").danger);
    assert_eq!(row(&lines, "Language").value, "zh");
    assert_eq!(lines.iter().find(|line| line.kind == "area").unwrap().value, "TypeScript\n杰森 -> JSON");
    assert!(row(&lines, "Mode").value == "clean" && row(&lines, "Mode").description.contains("filler words"));
    // Polishing: the text models, of every provider that answers in text.
    let polishing = row(&lines, "Model");
    assert_eq!(polishing.value, ids::choice("claude", "claude-opus-5-5"));
    assert!(titles(polishing).contains(&"Claude · claude-opus-5-5") && titles(polishing).contains(&"Codex · Default") && !titles(polishing).iter().any(|title| title.contains("This Computer")));
    assert!(!polishing.danger);
    assert_eq!(names(&model.entered()), ["dictation_status", "speech_models", "provider_models", "provider_models"]);
    // One that is downloaded: nothing to say.
    model.known.settings.settings.dictation_model = ModelRef::to("local-speech", "sense-voice-2024-07-17");
    assert!(!row(&model.lines(), "Speech Recognition").danger);
    // Its settings, each by its control.
    assert_eq!(patch(&model.acted(&set(Setting::DictationLanguage), "auto")), json!({ "dictationLanguage": "auto" }));
    assert_eq!(patch(&model.acted(&set(Setting::DictationLive), "false")), json!({ "dictationLive": false }));
}

#[test]
fn dictation_explains_what_a_session_cannot_do() {
    let mut model = told(|view| {
        providers(view);
        (view.settings.dictation_hold_key, view.settings.dictation_polish) = ("Control_R".into(), "nonsense".into());
    });
    model.known.capabilities = Some(Capabilities { session: "wayland".into(), hold_key: Support::None, dictation_words: vec![Words::Pasted], ..capabilities() });
    model.page = PageId::own("dictation");
    model.known.dictation = Some(DictationStatus { installed: true, installing: false, microphones: Vec::new() });
    let lines = model.lines();
    assert!(row(&lines, "Microphone").danger);
    assert!(row(&lines, "Hold-to-Talk Key").control.is_empty() && row(&lines, "Hold-to-Talk Key").description.contains("cannot be watched"));
    assert!(row(&lines, "Type as You Speak").control.is_empty() && row(&lines, "Type as You Speak").description.starts_with("Not available on Wayland"));
    // A mode it does not know is shown as none, and then there is no model to choose.
    assert_eq!(row(&lines, "Mode").value, "off");
    assert!(!has(&lines, "Model"));
}

#[test]
fn a_speech_model_is_downloaded_used_and_deleted_on_the_models_page() {
    let mut model = told(providers);
    model.show("provider:local-speech");
    model.answered(Then::Known, Ok(Answer::Speech(speech())));
    let lines = model.lines();
    // Always there: no name to change, nothing to remove.
    assert!(!has(&lines, "Name") && !has(&lines, "Remove Provider"));
    assert!(row(&lines, "What It Can Do").description.contains("stays here"));
    // One row a model: downloaded but not used, downloading, not downloaded.
    let speech = card(&lines, "Models");
    let downloaded = row(speech, "SenseVoice Small");
    assert_eq!((downloaded.control, downloaded.value.as_str(), downloaded.more.as_str()), ("confirm", "Delete…", "Use"));
    assert_eq!(downloaded.description, "Chinese, English · 240 MB. Fast.");
    let downloading = row(speech, "X-ASR Streaming");
    assert_eq!((downloading.control, downloading.value.as_str(), downloading.more.as_str()), ("text", "Speech model 50%", "Cancel"));
    assert_eq!((row(speech, "Whisper Turbo").control, row(speech, "Whisper Turbo").value.as_str()), ("button", "Download"));
    let commands = |asks: Vec<Ask>| asks.iter().map(|ask| ask.command.clone()).collect::<Vec<_>>();
    assert_eq!(commands(model.acted(&row(speech, "Whisper Turbo").id, "")), [Command::DownloadSpeechModel("whisper-turbo".into()), Command::SpeechModels]);
    assert_eq!(commands(model.acted(&downloading.more_id, "")), [Command::CancelSpeechDownload("x-asr-2026-06-05".into()), Command::SpeechModels]);
    assert_eq!(commands(model.acted(&downloaded.id, "")), [Command::DeleteSpeechModel("sense-voice-2024-07-17".into()), Command::SpeechModels]);
    // Used: dictation refers to it.
    let asks = model.acted(&downloaded.more_id, "");
    assert_eq!(asks[0].command, Command::ChangeSettings(json!({ "dictationModel": { "provider": "local-speech", "model": "sense-voice-2024-07-17" } }).as_object().unwrap().clone()));
    assert_eq!(model.known.settings.settings.dictation_model, ModelRef::to("local-speech", "sense-voice-2024-07-17"));
    // A download's progress, and its end, are read again from the host.
    use crate::wire::event;
    assert_eq!(commands(model.heard(event::DICTATION_INSTALL, &json!({ "type": "progress", "step": "speech-model", "received": 1, "total": 2 }))), [Command::SpeechModels]);
    assert_eq!(commands(model.heard(event::DICTATION_INSTALL, &json!({ "type": "error", "message": "no space left" }))), [Command::DictationStatus, Command::SpeechModels]);
    assert_eq!(model.error, "no space left");
    // In use: said, and no delete.
    let mut used = self::speech();
    used[0].in_use = true;
    model.answered(Then::Known, Ok(Answer::Speech(used)));
    let lines = model.lines();
    assert_eq!(row(card(&lines, "Models"), "SenseVoice Small").control, "badge");
}

#[test]
fn the_vocabulary_is_saved_as_its_lines() {
    let mut model = told(|view| view.settings.dictation_vocabulary = vec!["TypeScript".into(), "jason -> JSON".into()]);
    model.page = PageId::own("dictation");
    assert!(model.acted("words", "  TypeScript \n\n jason -> JSON\n").is_empty());
    assert_eq!(patch(&model.acted("words", "TypeScript\n杰森 -> JSON\n\n")), json!({ "dictationVocabulary": ["TypeScript", "杰森 -> JSON"] }));
}

#[test]
fn screenshots_have_their_hotkeys_and_the_engine() {
    let mut model = told(|_| {});
    model.page = PageId::own("screenshots");
    model.known.hotkeys = Hotkeys { supported: true, ..Hotkeys::default() };
    model.known.item_hotkeys = BTreeMap::from([("heyo:screenshot".into(), "<Control><Shift>4".into())]);
    let lines = model.lines();
    assert_eq!((row(&lines, "Capture Screenshot").hotkey.as_str(), row(&lines, "Capture Screenshot").keys.len()), ("heyo:screenshot", 3));
    assert_eq!(row(&lines, "Recognition Engine").control, "button");
    let asks = model.acted("act:install-ocr", "");
    assert_eq!(asked(&asks).name(), "ocr_install");
    assert_eq!(row(&model.lines(), "Recognition Engine").value, "Downloading…");
    // A download that could not be started is said, and offered again.
    model.answered(asks[0].then.clone(), Err("no network".into()));
    assert_eq!((row(&model.lines(), "Recognition Engine").control, model.error.as_str()), ("button", "no network"));
    model.acted("act:install-ocr", "");
    assert_eq!(asked(&model.heard(crate::wire::event::OCR_INSTALL, &json!({ "type": "installed" }))).name(), "ocr_status");
    model.answered(Then::Known, Ok(Answer::Ocr(OcrStatus { installed: true, installing: false })));
    assert_eq!(row(&model.lines(), "Recognition Engine").control, "badge");
}

#[test]
fn the_models_page_lists_each_provider_with_how_it_is_and_what_it_can_do() {
    let mut model = told(providers);
    assert_eq!(names(&model.show("models")), ["dictation_status", "speech_models", "provider_models", "provider_models"]);
    let lines = model.lines();
    let listed = card(&lines, "Providers");
    assert_eq!(listed.iter().map(|line| line.label.as_str()).collect::<Vec<_>>(), ["Speech Models on This Computer", "Claude", "Ollama", "Workstation", "Claude Code", "Codex"]);
    // Each a row to its page: its kind (where its name does not say it), how it is, what it can do.
    let claude = row(listed, "Claude");
    assert_eq!((claude.control, claude.id.as_str(), claude.description.as_str()), ("link", "go:provider:claude", "Claude API · Set up"));
    assert_eq!(claude.chips, ["Text", "Images", "Streaming"]);
    assert_eq!(row(listed, "Ollama").description, "OpenAI-Compatible Server · Checking…");
    assert_eq!(row(listed, "Ollama").chips, ["Text", "Streaming", "Private"]);
    assert_eq!(row(listed, "Claude Code").description, "Set up");
    assert!(row(listed, "Codex").danger && row(listed, "Codex").description == "Not found");
    model.answered(Then::Models("ollama".into()), Ok(Answer::Models(vec!["qwen3".into(), "llama3".into()])));
    model.answered(Then::Models("lan".into()), Err("connection refused".into()));
    model.answered(Then::Known, Ok(Answer::Speech(speech())));
    let lines = model.lines();
    let listed = card(&lines, "Providers");
    assert_eq!(row(listed, "Ollama").description, "OpenAI-Compatible Server · 2 models");
    assert!(row(listed, "Workstation").danger && row(listed, "Workstation").description.ends_with("Not reachable"));
    assert_eq!(row(listed, "Speech Models on This Computer").description, "1 downloaded");
    assert!(model.error.is_empty(), "a server that does not answer is said on its row");
    // No provider's details here: those are on its page.
    assert!(!has(&lines, "API Key") && !has(&lines, "Server"));
    assert_eq!(row(&lines, "Add Provider…").id, "go:provider:add");
    assert!(model.acted("go:provider:ollama", "").iter().any(|ask| ask.command == Command::ProviderModels("ollama".into())));
    assert_eq!((model.page.to_string(), model.section(), model.title()), ("provider:ollama".to_string(), "models".to_string(), "Models".to_string()));
}

#[test]
fn a_provider_is_added_by_its_kind_and_then_its_form() {
    let mut model = told(providers);
    model.show("models");
    model.acted("go:provider:add", "");
    let lines = model.lines();
    assert_eq!((lines[0].kind, lines[0].id.as_str(), lines[0].label.as_str()), ("back", "go:models", "Models"));
    assert_eq!(head(&lines).label, "Add Provider");
    // Every kind there is but the one that is always there.
    let kinds: Vec<&str> = lines.iter().filter(|line| line.control == "link").map(|line| line.label.as_str()).collect();
    assert_eq!(kinds, ["Claude API", "OpenAI-Compatible Server", "Claude Code", "Codex", "Doubao (Volcengine)"]);
    assert!(model.acted(&row(&lines, "OpenAI-Compatible Server").id, "").iter().all(|ask| ask.command != Command::GetSettings));
    assert_eq!(model.page, PageId::NewProvider);
    let lines = model.lines();
    assert_eq!(head(&lines).label, "New OpenAI-Compatible Server");
    let form = card(&lines, "Settings");
    assert_eq!(form.iter().map(|line| line.label.as_str()).collect::<Vec<_>>(), ["Name", "Server", "API Key", "More Models"]);
    // Its fields are kept here until it is added.
    assert!(model.acted(&row(form, "Server").id, " http://127.0.0.1:8080/v1 ").is_empty());
    assert!(model.acted(&row(form, "Name").id, "Lab").is_empty());
    assert!(model.acted(&row(form, "API Key").id, "sk-1").is_empty());
    let lines = model.lines();
    assert_eq!(row(card(&lines, "Settings"), "API Key").placeholder, "Typed — type again to replace it");
    assert_eq!(row(&lines, "Add This Provider").more_id, "go:models");
    let asks = model.acted("act:save-provider", "");
    let config = BTreeMap::from([("url".to_string(), "http://127.0.0.1:8080/v1".to_string()), ("key".to_string(), "sk-1".to_string())]);
    assert_eq!(asks[0].command, Command::AddProvider { kind: "openai-compatible".into(), name: "Lab".into(), config });
    // Added: its own page, where it can be checked.
    let mut added = model.known.settings.clone();
    added.settings.providers.push(provider("openai-compatible", "openai-compatible", "Lab", &[("url", "http://127.0.0.1:8080/v1")]));
    let asks = model.answered(asks[0].then.clone(), settings(added));
    assert_eq!(model.page, PageId::Provider("openai-compatible".into()));
    assert!(asks.iter().any(|ask| ask.command == Command::ProviderModels("openai-compatible".into())));
    assert_eq!(head(&model.lines()).label, "Lab");
    // The form is gone with it: a new one begins with choosing its kind.
    model.show("provider:new");
    assert_eq!(head(&model.lines()).label, "Add Provider");
}

#[test]
fn a_providers_page_edits_checks_and_removes_it() {
    let mut model = told(providers);
    model.show("provider:claude");
    let lines = model.lines();
    assert_eq!((lines[0].id.as_str(), head(&lines).label.as_str(), head(&lines).description.as_str()), ("go:models", "Claude", "Claude API"));
    let status = row(&lines, "Status").clone();
    assert!(status.description.starts_with("Connected with an API key.") && status.value == "Check Connection");
    assert_eq!(row(&lines, "What It Can Do").chips, ["Text", "Images", "Streaming"]);
    assert_eq!(row(&lines, "What It Can Do").description, "What it is given is sent to Anthropic.");
    // The key is never shown, only that it is saved, and it can be forgotten.
    let key = row(card(&lines, "Settings"), "API Key").clone();
    assert_eq!((key.control, key.value.as_str(), key.placeholder.as_str(), key.more.as_str()), ("secret", "", "Saved — type a new key to replace it", "Remove"));
    assert_eq!(asked(&model.acted(&key.id, "sk-ant-2")), Command::SetProviderField { provider: "claude".into(), field: "key".into(), value: "sk-ant-2".into() });
    assert!(model.acted(&key.id, "  ").is_empty());
    assert_eq!(asked(&model.acted(&key.more_id, "")), Command::SetProviderField { provider: "claude".into(), field: "key".into(), value: String::new() });
    assert_eq!(asked(&model.acted(&row(&lines, "Name").id, "Work")), Command::SetProviderField { provider: "claude".into(), field: "name".into(), value: "Work".into() });
    // Checked on a thread of its own; once at a time; what it came to is said.
    let checking = model.acted(&status.id, "");
    assert_eq!(asked(&checking), Command::CheckProvider("claude".into()));
    assert!(Command::CheckProvider("claude".into()).slow());
    assert!(model.acted(&status.id, "").is_empty());
    assert!(row(&model.lines(), "Status").busy);
    model.answered(checking[0].then.clone(), Err("The Claude API key is invalid.".into()));
    let failed = row(&model.lines(), "Connection Failed").clone();
    assert!(failed.danger && failed.description == "The Claude API key is invalid." && model.error.is_empty());
    model.acted(&status.id, "");
    model.answered(checking[0].then.clone(), Ok(Answer::Checked(Checked { said: "Claude's API takes the key.".into(), models: None })));
    assert!(!row(&model.lines(), "Connection Works").danger);
    // The model it has, and what uses it.
    assert_eq!(row(card(&model.lines(), "Models"), "claude-opus-5-5").value, "Used for Polish");
    // Removed: back to the list.
    let remove = row(&lines, "Remove Provider").clone();
    assert_eq!(remove.control, "confirm");
    assert_eq!(asked(&model.acted(&remove.id, "")), Command::RemoveProvider("claude".into()));
    assert_eq!(model.page, PageId::own("models"));
}

#[test]
fn a_server_lists_its_models_and_says_where_what_it_is_given_goes() {
    let mut model = told(providers);
    model.show("provider:ollama");
    assert!(row(&model.lines(), "Status").description.starts_with("Checking… Text stays on this computer."));
    // A check lists its models too.
    let checking = model.acted("check-provider:ollama", "");
    model.answered(checking[0].then.clone(), Ok(Answer::Checked(Checked { said: "The server answers and lists 2 models.".into(), models: Some(vec!["qwen3".into(), "llama3".into()]) })));
    let lines = model.lines();
    assert!(row(&lines, "Status").description.starts_with("Connected: qwen3, llama3. Text stays on this computer.") && !row(&lines, "Status").danger);
    assert_eq!(row(&lines, "Connection Works").description, "The server answers and lists 2 models.");
    let models = card(&lines, "Models");
    assert_eq!(models.iter().map(|line| line.label.as_str()).collect::<Vec<_>>(), ["First Listed", "qwen3", "llama3"]);
    assert_eq!(row(models, "qwen3").value, "Used for AI");
    // Its address changed: asked again, and the check no longer said.
    let url = row(card(&lines, "Settings"), "Server").id.clone();
    assert_eq!(model.acted(&url, "http://127.0.0.1:11434/v1").len(), 0, "the same address is no change");
    let asks = model.acted(&url, "http://127.0.0.1:8080/v1");
    assert_eq!(asks.iter().map(|ask| ask.command.clone()).collect::<Vec<_>>(), [Command::SetProviderField { provider: "ollama".into(), field: "url".into(), value: "http://127.0.0.1:8080/v1".into() }, Command::ProviderModels("ollama".into())]);
    assert!(!has(&model.lines(), "Connection Works"));
    // One elsewhere, over plain http, that does not answer.
    model.show("provider:lan");
    model.answered(Then::Models("lan".into()), Err("connection refused".into()));
    let status = row(&model.lines(), "Status").clone();
    assert!(status.danger && status.description.starts_with("Not reachable: connection refused.") && status.description.contains("sent to 10.0.0.5:8000 over plain http"));
    assert_eq!(row(card(&model.lines(), "Models"), "Not reachable").description, "connection refused");
}

#[test]
fn a_tools_page_says_whether_it_is_found() {
    let mut model = told(providers);
    model.show("provider:claude-code");
    let lines = model.lines();
    let status = row(&lines, "Status");
    assert!(!status.danger && status.description.starts_with("Using 2.1.302 (Claude Code) at /home/ada/.local/bin/claude."), "{}", status.description);
    assert_eq!(row(&lines, "What It Can Do").description, "Runs on this computer, and sends what it is given to Anthropic.");
    assert_eq!(card(&lines, "Models").iter().map(|line| line.label.as_str()).collect::<Vec<_>>(), ["Default", "opus", "sonnet", "haiku"]);
    model.show("provider:codex");
    let lines = model.lines();
    assert!(row(&lines, "Status").danger && row(&lines, "Status").description.starts_with("There is no executable at /opt/codex."));
    assert_eq!(row(card(&lines, "Settings"), "Executable").value, "/opt/codex");
}

#[test]
fn doubao_asks_for_the_key_of_its_console_and_its_resources_are_speech_models() {
    let mut model = told(|view| {
        providers(view);
        view.settings.dictation_model = ModelRef::to("doubao", "");
        view.settings.providers.push(provider("doubao", "doubao", "Doubao", &[]));
    });
    model.page = PageId::own("models");
    assert_eq!(row(&model.lines(), "Doubao").description, "Doubao (Volcengine) · Key missing");
    model.show("provider:doubao");
    let lines = model.lines();
    assert!(row(&lines, "Status").danger && row(&lines, "Status").description.starts_with("The key is missing."));
    let form = card(&lines, "Settings");
    assert_eq!(row(form, "Console").value, "new");
    assert!(has(form, "API Key") && !has(form, "App ID") && row(form, "Remove Filler Words").control == "switch");
    assert!(has(card(&lines, "Models"), "Doubao Streaming ASR 2.0, Hourly"));
    // The old console: an App ID and an Access Token instead.
    let view = &mut model.known.settings;
    view.settings.providers[6].config = BTreeMap::from([("console".into(), "old".into()), ("appId".into(), "id".into())]);
    view.provider_states.insert("doubao".into(), ProviderState { secrets_set: vec!["accessToken".into()], cli_found: None });
    let lines = model.lines();
    let form = card(&lines, "Settings");
    assert!(!row(&lines, "Status").danger && !has(form, "API Key") && row(form, "App ID").value == "id" && row(form, "Access Token").more == "Remove");
    assert_eq!(asked(&model.acted(&row(form, "Remove Filler Words").id, "true")), Command::SetProviderField { provider: "doubao".into(), field: "removeFillers".into(), value: "true".into() });
    // Recognition offers its resources beside the models of this computer.
    model.page = PageId::own("dictation");
    let lines = model.lines();
    let recognition = row(&lines, "Speech Recognition");
    assert!(titles(recognition).contains(&"Doubao · Doubao Streaming ASR 2.0, Hourly"));
    assert_eq!(recognition.value, ids::choice("doubao", ""));
}

#[test]
fn a_picker_offers_what_can_be_used_and_says_what_cannot() {
    let mut model = told(providers);
    model.page = PageId::own("advanced");
    model.answered(Then::Models("ollama".into()), Ok(Answer::Models(vec!["qwen3".into(), "llama3".into()])));
    let lines = model.lines();
    let answers = row(&lines, "Model");
    assert_eq!(answers.value, ids::choice("ollama", "qwen3"));
    assert_eq!(&titles(answers)[..4], ["Claude · claude-opus-5-5", "Ollama · First Listed", "Ollama · qwen3", "Ollama · llama3"]);
    assert!(!answers.danger);
    // Cases: none, or a model that keeps what it is given here; never Claude, a tool, or the server elsewhere.
    let cases = row(&lines, "Clipboard Search Cases");
    assert_eq!(titles(cases), ["None", "Ollama · First Listed", "Ollama · qwen3", "Ollama · llama3"]);
    assert!(cases.value.is_empty() && !cases.danger);
    // Choosing is a change of the reference.
    assert_eq!(patch(&model.acted(&answers.id, &ids::choice("claude-code", "opus"))), json!({ "aiModel": { "provider": "claude-code", "model": "opus" } }));
    assert_eq!(patch(&model.acted(&cases.id, &ids::choice("ollama", "qwen3"))), json!({ "casesModel": { "provider": "ollama", "model": "qwen3" } }));
    assert_eq!(patch(&model.acted(&cases.id, "")), json!({ "casesModel": { "provider": "", "model": "" } }));
    // A tool that is not found: said where it is chosen.
    model.known.settings.settings.ai_model = ModelRef::to("codex", "");
    let answers = row(&model.lines(), "Model").clone();
    assert!(answers.danger && answers.description.starts_with("There is no executable at /opt/codex."));
    // A provider that is gone: still shown, and said.
    model.known.settings.settings.ai_model = ModelRef::to("gone", "big");
    let answers = row(&model.lines(), "Model").clone();
    assert!(answers.danger && answers.value == ids::choice("gone", "big") && titles(&answers).contains(&"gone · big") && answers.description.contains("no longer connected"));
    // Models is a button away.
    assert_eq!(names(&model.acted("act:open-models", "")), ["dictation_status", "speech_models", "provider_models", "provider_models"]);
    assert_eq!(model.page, PageId::own("models"));
}

#[test]
fn about_says_the_version_and_what_updating_is_doing() {
    let mut model = told(|view| (view.version, view.can_update) = ("0.1.0".into(), false));
    model.page = PageId::own("about");
    let about = |model: &Model| model.lines()[0].clone();
    let update = Then::Acted(Id::Act(Act::Update));
    assert_eq!((about(&model).value.as_str(), about(&model).more.as_str()), ("0.1.0", ""));
    model.known.settings.can_update = true;
    assert_eq!(about(&model).more, "Check for Updates");
    assert_eq!(asked(&model.acted("act:update", "")).name(), "check_for_update");
    assert!(about(&model).busy && model.acted("act:update", "").is_empty());
    model.answered(update.clone(), Ok(Answer::Update(None)));
    assert_eq!(about(&model).more, "Heyo Is Up to Date");
    model.acted("act:update", "");
    model.answered(update.clone(), Ok(Answer::Update(Some("0.2.0".into()))));
    assert_eq!(about(&model).more, "Install Heyo 0.2.0 and Restart");
    assert_eq!(asked(&model.acted("act:update", "")).name(), "install_update");
    model.answered(update, Err("no network".into()));
    assert_eq!((about(&model).more.as_str(), model.error.as_str()), ("Install Heyo 0.2.0 and Restart", "no network"));
    model.answered(Then::Acted(Id::Act(Act::CopyDiagnostics)), Ok(Answer::Done));
    assert!(about(&model).on);
}

/// Extensions as the host lists them: one installed, Heyo's own, and one
/// being developed.
fn extensions() -> Vec<ExtensionInfo> {
    serde_json::from_value(json!([
        { "id": "/data/zeta", "name": "zeta", "title": "Zeta Tools", "author": "someone", "icon": "icon.png", "builtin": false, "description": "Tools.",
            "preferences": [{ "name": "token", "type": "password", "title": "Token", "required": true }, { "name": "mode", "type": "dropdown", "title": "Mode", "data": [{ "title": "Fast", "value": "fast" }, { "title": "Careful", "value": "careful" }] }],
            "values": { "extension": { "mode": "careful", "token": "tok_1" }, "commands": {} },
            "commands": [{ "id": "cmd:zeta:one", "name": "one", "title": "First Thing", "description": null, "mode": "view", "icon": "star-16", "preferences": [{ "name": "loud", "type": "checkbox", "title": "Sound", "label": "Play a sound", "default": true }] }, { "id": "cmd:zeta:two", "name": "two", "title": "Second Thing", "description": "Does more", "mode": "view", "icon": null, "preferences": [] }] },
        { "id": BUILTIN, "name": BUILTIN, "title": "Heyo", "description": null, "author": null, "builtin": true, "icon": "store-16", "preferences": [], "values": { "extension": {}, "commands": {} }, "commands": [{ "id": "cmd:@heyo:clipboard-history", "name": "clipboard-history", "title": "Clipboard History", "description": "What was copied", "mode": "view", "icon": null, "preferences": [] }, { "id": "cmd:@heyo:dictate", "name": "dictate", "title": "Dictate", "description": null, "mode": "no-view", "icon": null, "preferences": [] }] },
        { "id": "/src/alpha", "name": "alpha", "title": "alpha Notes", "description": null, "author": null, "icon": null, "builtin": false, "development": "/src/alpha", "preferences": [], "values": { "extension": {}, "commands": {} }, "commands": [{ "id": "cmd:alpha:only", "name": "only", "title": "Only Command", "description": null, "mode": "view", "icon": null, "preferences": [] }] },
    ]))
    .unwrap()
}

fn with_extensions() -> Model {
    let mut model = told(|view| {
        let settings = &mut view.settings;
        settings.aliases = BTreeMap::from([("app:firefox.desktop".into(), "ff".into())]);
        settings.disabled_commands = vec!["cmd:zeta:two".into()];
        (settings.clipboard_history, settings.clipboard_retention_days) = (true, 90);
        settings.clipboard_ignored_apps = vec!["org.keepassxc.KeePassXC.desktop".into(), "gone.desktop".into()];
        (settings.live_activities, settings.activities_off) = (true, Vec::new());
    });
    model.known.capabilities = Some(capabilities());
    model.known.hotkeys = Hotkeys { supported: true, clipboard: Some("<Super>v".into()), ..Hotkeys::default() };
    model.known.extensions = extensions();
    let application = |id: &str, title: &str, subtitle: Option<&str>, icon: &str| Application { id: id.into(), title: title.into(), subtitle: subtitle.map(str::to_string), icon: Some(icon.into()) };
    model.known.applications = vec![application("app:firefox.desktop", "Firefox", Some("Web Browser"), "/usr/share/icons/firefox.png"), application("app:files.desktop", "Files", None, "folder")];
    model.known.item_hotkeys = BTreeMap::from([("app:files.desktop".into(), "<Super>e".into())]);
    let source = |id: &str, name: &str, icon: Option<&str>, seen: bool| Source { id: id.into(), name: name.into(), icon: icon.map(PathBuf::from), seen };
    model.known.sources = vec![source("org.keepassxc.KeePassXC.desktop", "KeePassXC", Some("/icons/keepassxc.png"), false), source("firefox.desktop", "Firefox", None, true), source("files.desktop", "Files", None, false)];
    model
}

#[test]
fn the_sidebar_lists_pages_then_extensions_in_their_order() {
    let mut model = with_extensions();
    let (entries, pages) = model.nav();
    assert_eq!(pages, 8);
    assert_eq!(entries.iter().map(|entry| entry.title.as_str()).collect::<Vec<_>>(), ["General", "Shortcuts", "Dictation", "Screenshots", "Models", "Connections", "Advanced", "About", "Applications", "Heyo Commands", "alpha Notes", "Zeta Tools"]);
    // A page's symbol, a symbol the host named, a file of the extension's own.
    assert_eq!((entries[0].icon.as_deref(), entries[9].icon.as_deref(), entries[11].icon.as_deref()), (Some("cog-16"), Some("store-16"), Some("/data/zeta/assets/icon.png")));
    assert!(entries[11].extension && !entries[0].extension);
    // An extension is found by a command of its own; the applications
    // only by their name.
    model.nav_search = "dict".into();
    let (entries, pages) = model.nav();
    assert_eq!((entries.iter().map(|entry| entry.title.as_str()).collect::<Vec<_>>(), pages), (vec!["Dictation", "Heyo Commands"], 1));
    model.nav_search = "firefox".into();
    assert_eq!(model.nav().0, []);
}

#[test]
fn the_applications_are_a_list_one_can_search() {
    let mut model = with_extensions();
    assert_eq!(model.show(APPLICATIONS), []);
    assert_eq!((model.title().as_str(), model.find_placeholder()), ("Applications", "Search applications…"));
    let lines = model.lines();
    assert_eq!((lines[0].kind, lines[0].label.as_str(), lines[0].icon.as_deref()), ("head", "Applications", Some("app-window-grid-3x3-16")));
    let firefox = row(&lines, "Firefox");
    assert_eq!((firefox.description.as_str(), firefox.icon.as_deref(), firefox.hotkey.as_str(), firefox.control), ("Web Browser", Some("/usr/share/icons/firefox.png"), "app:firefox.desktop", ""));
    assert_eq!(row(&lines, "Files").keys, ["Super", "E"]);
    model.known.find = "FIRE".into();
    assert!(has(&model.lines(), "Firefox") && !has(&model.lines(), "Files"));
    model.known.find = "nothing".into();
    assert!(has(&model.lines(), "No matches"));
    // Another page starts with nothing searched for.
    model.show("general");
    assert!(model.known.find.is_empty());
}

#[test]
fn an_extension_has_its_preferences_commands_permissions_and_removal() {
    let mut model = with_extensions();
    assert_eq!(asked(&model.show("/data/zeta")), Command::ExtensionPermissions("/data/zeta".into()));
    let permission = |permission, allows: &'static str, state: &'static str| Row { permission, allows: allows.into(), state: state.into() };
    model.answered(Then::Permissions("/data/zeta".into()), Ok(Answer::Permissions(vec![permission(Permission::Windows, "see and move windows", "allowed"), permission(Permission::Ai, "ask the model", "asks"), permission(Permission::Activity, "show live activities", "free")])));
    let lines = model.lines();
    assert_eq!((lines[0].label.as_str(), lines[0].description.as_str(), lines[1].kind, lines[1].label.as_str()), ("Zeta Tools", "by someone", "note", "Tools."));
    // Preferences, of each kind the API has; a value stored, else the default.
    assert_eq!((row(&lines, "Token *").control, row(&lines, "Mode").control, row(&lines, "Mode").value.as_str(), titles(row(&lines, "Mode"))), ("password", "select", "careful", vec!["Fast", "Careful"]));
    // A password that is stored stays in its field (as dots): the field
    // is not one that forgets what was typed, or leaving it would save
    // the nothing it then held.
    assert_eq!(row(&lines, "Token *").value, "tok_1");
    let sound = row(&lines, "Sound");
    assert_eq!((sound.control, sound.on, sound.description.as_str()), ("switch", true, "Play a sound"));
    assert!(has(&lines, "First Thing Preferences"));
    // Commands: on unless disabled, each with its hotkey.
    assert!(row(&lines, "First Thing").on && !row(&lines, "Second Thing").on && row(&lines, "First Thing").icon.as_deref() == Some("star-16"));
    assert_eq!(patch(&model.acted("enable:cmd:zeta:two", "true")), json!({ "disabledCommands": [] }));
    // Permissions: what was allowed can be taken back; activities have a switch.
    let lines = model.lines();
    assert_eq!((row(&lines, "See and move windows").control, row(&lines, "See and move windows").description.as_str()), ("button", "Allowed by you"));
    assert_eq!((row(&lines, "Ask the model").control, row(&lines, "Show live activities").control, row(&lines, "Show live activities").on), ("", "switch", true));
    assert_eq!(model.acted(&row(&lines, "See and move windows").id, "")[0], ask(Command::RevokeExtensionPermission { extension: "zeta".into(), permission: Permission::Windows }, Then::Permissions("/data/zeta".into())));
    assert_eq!(patch(&model.acted("activities:zeta", "false")), json!({ "activitiesOff": ["zeta"] }));
    // A preference is saved among the others of the extension, and the
    // extensions are then read again.
    let saving = model.acted(&sound.id, "false");
    assert_eq!(saved(&saving), ("zeta".to_string(), json!({ "extension": { "mode": "careful", "token": "tok_1" }, "commands": { "one": { "loud": false } } })));
    assert_eq!(names(&model.answered(saving[0].then.clone(), Ok(Answer::Done))), ["list_extensions", "list_applications", "item_hotkeys"]);
    assert_eq!(saved(&model.acted(&row(&lines, "Token *").id, "secret")).1["extension"], json!({ "mode": "careful", "token": "secret" }));
    // Removal, after which the window is elsewhere.
    assert_eq!((row(&lines, "Uninstall Extension").control, row(&lines, "Uninstall Extension").placeholder.as_str()), ("confirm", "Click Again to Uninstall"));
    let removing = model.acted("uninstall:/data/zeta", "");
    assert_eq!(asked(&removing), Command::UninstallExtension("/data/zeta".into()));
    assert_eq!(model.answered(removing[0].then.clone(), Ok(Answer::Done)).len(), 3);
    assert_eq!(model.page, PageId::own("general"));
    model.page = PageId::parse("/src/alpha");
    assert!(has(&model.lines(), "Stop Developing") && model.lines()[0].description == "In development from /src/alpha");
}

/// How finding by meaning stands.
fn meaning(installed: bool, indexed: usize, waiting: usize, problem: Option<&str>) -> meaning::Status {
    meaning::Status { enabled: true, installed, installing: false, indexed, waiting, problem: problem.map(str::to_string) }
}

#[test]
fn heyos_commands_have_the_clipboard_and_snippets() {
    let mut model = with_extensions();
    assert_eq!(names(&model.show(BUILTIN)), ["clipboard_sources", "clipboard_meaning_status"]);
    model.known.meaning = Some(meaning(true, 1200, 3400, None));
    model.known.settings.settings.clipboard_meaning = true;
    let lines = model.lines();
    assert_eq!((lines[0].description.as_str(), model.title().as_str()), ("Built into Heyo", "Heyo Commands"));
    // The clipboard history's hotkey is Heyo's own, on a row of its own.
    assert!(row(&lines, "Clipboard History").hotkey.is_empty() && row(&lines, "Dictate").hotkey == "cmd:@heyo:dictate");
    assert_eq!((row(&lines, "Hotkey").hotkey.as_str(), row(&lines, "Hotkey").keys.clone()), ("@clipboard", vec!["Super".to_string(), "V".to_string()]));
    assert_eq!((row(&lines, "Keep History For").value.as_str(), titles(row(&lines, "Keep History For"))[3]), ("90", "3 Months"));
    assert_eq!(patch(&model.acted(&set(Setting::ClipboardRetentionDays), "7")), json!({ "clipboardRetentionDays": 7 }));
    assert_eq!(model.known.settings.settings.clipboard_retention_days, 7);
    assert_eq!(row(&lines, "Clear History").control, "confirm");
    assert!(row(&lines, "Find by Meaning").description.ends_with("Reading the history: 3,400 entries to go; until then some are found by their words only."));
    assert_eq!(model.poll().len(), 1);
    // Left out of the history: by name where the application is known.
    assert_eq!((row(&lines, "KeePassXC").icon.as_deref(), row(&lines, "gone.desktop").description.as_str(), row(&lines, "gone.desktop").icon.as_deref()), (Some("/icons/keepassxc.png"), "Not installed", Some("app-window-16")));
    assert_eq!(titles(row(&lines, "Never Record Copies From")), ["Add Application…", "Firefox", "Files"]);
    assert_eq!(patch(&model.acted("ignore:add", "firefox.desktop")), json!({ "clipboardIgnoredApps": ["org.keepassxc.KeePassXC.desktop", "gone.desktop", "firefox.desktop"] }));
    assert_eq!(patch(&model.acted("ignore:remove:gone.desktop", ""))["clipboardIgnoredApps"], json!(["org.keepassxc.KeePassXC.desktop", "firefox.desktop"]));
    assert!(model.acted("ignore:add", "").is_empty());
    // With recording off, what only matters while it is on is not shown.
    model.known.settings.settings.clipboard_history = false;
    let lines = model.lines();
    assert!(!has(&lines, "Pause Recording") && !has(&lines, "Find by Meaning") && !has(&lines, "Never Record Copies From"));
    // Snippets: the switch, and where it is not on offer, why.
    assert_eq!(row(&lines, "Expand Snippets as You Type").control, "switch");
    model.known.capabilities = Some(Capabilities { snippet_expansion: Support::None, session: "wayland".into(), ..capabilities() });
    let lines = model.lines();
    assert!(row(&lines, "Expand Snippets as You Type").control.is_empty() && row(&lines, "Expand Snippets as You Type").description.starts_with("Not available on Wayland"));
}

#[test]
fn find_by_meaning_offers_its_download_until_it_is_there() {
    let mut model = with_extensions();
    model.page = PageId::Extension(BUILTIN.into());
    model.known.meaning = Some(meaning(false, 0, 0, None));
    let lines = model.lines();
    assert_eq!((row(&lines, "Find by Meaning").control, row(&lines, "Find by Meaning").id.as_str()), ("button", "act:install-meaning"));
    assert_eq!(asked(&model.acted("act:install-meaning", "")).name(), "clipboard_meaning_install");
    assert_eq!(row(&model.lines(), "Find by Meaning").value, "Downloading…");
    model.heard(crate::wire::event::CLIPBOARD_MEANING_INSTALL, &json!({ "type": "installed" }));
    model.known.meaning = Some(meaning(true, 1, 0, Some("the model would not load")));
    model.known.settings.settings.clipboard_meaning = true;
    let lines = model.lines();
    assert!(row(&lines, "Find by Meaning").control == "switch" && row(&lines, "Find by Meaning").description.ends_with("It is not working at present: the model would not load"));
    assert!(model.poll().is_empty());
}

#[test]
fn the_shortcuts_table_opens_narrows_and_changes() {
    let mut model = with_extensions();
    model.show("shortcuts");
    let labels = |model: &Model| model.lines().iter().filter(|line| line.kind == "group" || line.kind == "entry").map(|line| line.label.clone()).collect::<Vec<_>>();
    // Everything starts closed but the extensions; one with a single
    // command is shown as that command.
    assert_eq!(labels(&model), ["Applications", "Extensions", "Zeta Tools", "Heyo Commands", "Only Command"]);
    assert!(model.acted("open:applications", "toggle").is_empty());
    model.acted("open:/data/zeta", "toggle");
    assert_eq!(labels(&model), ["Applications", "Firefox", "Files", "Extensions", "Zeta Tools", "First Thing", "Second Thing", "Heyo Commands", "Only Command"]);
    let lines = model.lines();
    let (firefox, second, zeta) = (row(&lines, "Firefox"), row(&lines, "Second Thing"), row(&lines, "Zeta Tools"));
    assert_eq!((firefox.alias.as_str(), firefox.checkable, firefox.hotkey.as_str()), ("ff", false, "app:firefox.desktop"));
    assert!(second.checkable && !second.on && second.indent);
    // Some of its commands are on: neither ticked nor empty.
    assert!(zeta.checkable && zeta.on && zeta.mixed && zeta.open);
    assert_eq!(patch(&model.acted(&zeta.check_id, "false"))["disabledCommands"], json!(["cmd:zeta:one", "cmd:zeta:two"]));
    assert_eq!(patch(&model.acted("alias:app:files.desktop", " fm ")), json!({ "aliases": { "app:firefox.desktop": "ff", "app:files.desktop": "fm" } }));
    assert!(model.acted("alias:app:files.desktop", "fm").is_empty());
    assert_eq!(patch(&model.acted("alias:app:firefox.desktop", ""))["aliases"], json!({ "app:files.desktop": "fm" }));
    // Searching opens everything it finds something in, by a command's
    // title or its group's.
    model.known.find = "zeta".into();
    assert_eq!(labels(&model), ["Extensions", "Zeta Tools", "First Thing", "Second Thing"]);
    model.known.find.clear();
    // The menu: what the table is narrowed to.
    let (title, ticked, picks) = model.filter().unwrap();
    assert_eq!((title.as_str(), ticked, picks.len()), ("All categories", 0, 9));
    model.pick("mod:hotkey");
    assert_eq!(labels(&model), ["Applications", "Files"]);
    model.pick("mod:hotkey");
    model.pick("mod:disabled");
    assert!(labels(&model).contains(&"First Thing".to_string()) && !labels(&model).contains(&"Firefox".to_string()));
    model.pick("mod:disabled");
    model.pick("cat:applications");
    assert_eq!((labels(&model)[0].as_str(), model.filter().unwrap().0.as_str(), labels(&model).contains(&"Extensions".to_string())), ("Applications", "Applications", false));
    model.pick("cat:all");
    model.known.find = "no such thing".into();
    assert_eq!(model.lines().iter().map(|line| (line.kind, line.label.as_str())).collect::<Vec<_>>(), [("note", "Nothing matches."), ("end", "")]);
    assert!(model.filter().is_some());
    model.show("general");
    assert!(model.filter().is_none());
}

#[test]
fn what_changes_elsewhere_is_read_again() {
    let mut model = with_extensions();
    use crate::wire::event;
    assert_eq!(asked(&model.heard(event::SETTINGS_CHANGED, &Value::Null)).name(), "get_settings");
    assert_eq!(names(&model.heard(event::INDEX_READY, &Value::Null)), ["list_extensions", "list_applications", "item_hotkeys"]);
    assert_eq!(names(&model.opened()), ["get_settings", "get_hotkeys", "get_capabilities", "list_extensions", "list_applications", "item_hotkeys"]);
    model.answered(Then::Known, Ok(Answer::Extensions(Vec::new())));
    assert_eq!(model.nav().0.len(), 9);
}

#[test]
fn no_setting_changes_unless_it_is_different() {
    // What the host is sent is what differs: nothing, when nothing does.
    let mut model = told(|view| view.settings.compact_mode = true);
    assert!(model.acted(&set(Setting::CompactMode), "true").is_empty());
}
