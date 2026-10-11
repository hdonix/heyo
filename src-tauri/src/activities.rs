//! Live activities: things in progress that an extension or Heyo itself
//! started, kept here in the host and shown wherever this desktop shows
//! such things (docs/live-activities.md). Today that is the "In Progress"
//! section of the palette (5.0) and, for what must reach the user with the
//! palette away, one notification per activity (5.1, the fallback).
//!
//! The table ([`Table`]) is plain data with the time passed in, so that the
//! limits (6.6), the sequence guard (6.2), the ordering (5.6), the two-step
//! fading of a quiet activity and the text of a timer the host runs itself
//! (6.1) are all tested without a window. What a change calls for outside
//! the table (a notification, an event for the extension, something to
//! open) comes back as [`Effect`]s, which the Tauri side at the bottom of
//! this file carries out.
//!
//! A tick once a second re-reads the table only while it holds something:
//! with nothing in progress, nothing runs.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use ts_rs::TS;

use crate::wire::{ErrorCode, ExtError};

/// Milliseconds since the Unix epoch, as the runtime writes a `Date`.
pub type Millis = u64;

// --- Limits (6.6) --------------------------------------------------------------

/// How many activities one extension may have at once.
pub const PER_EXTENSION: usize = 3;
/// An activity lives this long at most.
pub const LIFETIME: Millis = 8 * 60 * 60 * 1000;
/// Without an update for this long it is shown minimal...
pub const DEMOTE_AFTER: Millis = 2 * 60 * 60 * 1000;
/// ...and after this long it ends.
pub const END_AFTER: Millis = 4 * 60 * 60 * 1000;
/// Attributes and content together.
pub const MAX_BYTES: usize = 4096;
/// Updates closer together than this are merged into one.
pub const UPDATE_GAP: Millis = 1000;
/// Alerts per activity per minute; the rest are plain updates.
pub const ALERTS_PER_MINUTE: usize = 2;
/// How long the first appearance and an alert hold attention.
pub const ATTENTION: Millis = 4000;
/// How long an ended activity stays (`keep: "brief"`).
pub const KEEP_BRIEF: Millis = 4000;
/// How long after the user touched an activity it counts as theirs (5.6).
pub const INTERACTED: Millis = 60 * 1000;
/// Characters of the compact value before it is cut.
pub const COMPACT_CHARACTERS: usize = 10;

// --- What extensions give (6.1, 6.5) ---------------------------------------------

/// A span of time the host counts along, with where it was paused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ActivityTimer", export, export_to = "../../shared/bindings/")]
pub struct Timer {
    #[ts(type = "number")]
    pub start: Millis,
    /// Left out: counting up, with no end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub end: Option<Millis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub paused_at: Option<Millis>,
    /// With an end, counting down unless false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub counts_down: Option<bool>,
    /// `clock` ("12:05", the default) or `relative` ("12 min").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "\"clock\" | \"relative\"")]
    pub style: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ActivityProgress", export, export_to = "../../shared/bindings/")]
pub struct Progress {
    /// 0–1; left out with no interval, the progress is indeterminate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub value: Option<f64>,
    /// Moves along by itself with the time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub interval: Option<Timer>,
}

/// A small piece of content that changes by itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(rename = "ActivityValue", export, export_to = "../../shared/bindings/")]
pub enum Value {
    Text(String),
    Timer {
        timer: Timer,
    },
    Progress {
        progress: Progress,
    },
    Icon {
        #[ts(type = "unknown")]
        icon: Json,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(rename = "ActivityBody", export, export_to = "../../shared/bindings/")]
pub enum Body {
    Progress { progress: Progress },
    Timer { timer: Timer },
    Text { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ActivityLaunch", export, export_to = "../../shared/bindings/")]
pub struct Launch {
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "Record<string, string>")]
    pub arguments: Option<Json>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub context: Option<Json>,
}

/// What a button, or the activity itself, does. The first three the host
/// does by itself; `event` goes back to the extension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(rename = "ActivityAction", export, export_to = "../../shared/bindings/")]
pub enum Action {
    Open {
        open: String,
    },
    Copy {
        copy: String,
    },
    Launch {
        launch: Launch,
    },
    Event {
        event: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "unknown")]
        data: Option<Json>,
    },
    End {
        end: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ActivityButton", export, export_to = "../../shared/bindings/")]
pub struct Button {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub icon: Option<Json>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "\"regular\" | \"destructive\"")]
    pub style: Option<String>,
    pub action: Action,
}

/// The part that does not change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ActivityAttributes", export, export_to = "../../shared/bindings/")]
pub struct Attributes {
    /// Unique within the extension.
    pub id: String,
    /// What it is: "Build", "Timer".
    pub name: String,
    #[ts(type = "unknown")]
    pub icon: Json,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub tint: Option<Json>,
    /// What choosing the activity itself does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub action: Option<Action>,
    /// Whether the user may end it. True unless said otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dismissible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "\"notify\" | \"drop\"")]
    pub fallback: Option<String>,
}

/// The part that changes. Every field is optional so that the same shape
/// serves as the patch an update brings; a start must give a title.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ActivityContent", export, export_to = "../../shared/bindings/")]
pub struct Content {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub compact: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub body: Option<Body>,
    /// Three at most.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub actions: Option<Vec<Button>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub needs_attention: Option<bool>,
    /// After this the content may be wrong: it is shown dimmed, no more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub stale_at: Option<Millis>,
    /// Among this extension's own activities, 0–100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub relevance: Option<f64>,
}

impl Content {
    /// Takes what `patch` gives and keeps the rest.
    fn apply(&mut self, patch: Content) {
        let Content { compact, title, subtitle, body, actions, needs_attention, stale_at, relevance } = patch;
        self.compact = compact.or(self.compact.take());
        self.title = title.or(self.title.take());
        self.subtitle = subtitle.or(self.subtitle.take());
        self.body = body.or(self.body.take());
        self.actions = actions.or(self.actions.take());
        self.needs_attention = needs_attention.or(self.needs_attention);
        self.stale_at = stale_at.or(self.stale_at);
        self.relevance = relevance.or(self.relevance);
    }

    /// Whether the text changes with the time: a running timer, or
    /// progress along a running interval.
    fn changes_by_itself(&self) -> bool {
        let compact = match &self.compact {
            Some(Value::Timer { timer }) => timer.running(),
            Some(Value::Progress { progress }) => progress.running(),
            _ => false,
        };
        let body = match &self.body {
            Some(Body::Timer { timer }) => timer.running(),
            Some(Body::Progress { progress }) => progress.running(),
            _ => false,
        };
        compact || body
    }

    fn timers(&self) -> impl Iterator<Item = &Timer> {
        let compact = match &self.compact {
            Some(Value::Timer { timer }) => Some(timer),
            Some(Value::Progress { progress }) => progress.interval.as_ref(),
            _ => None,
        };
        let body = match &self.body {
            Some(Body::Timer { timer }) => Some(timer),
            Some(Body::Progress { progress }) => progress.interval.as_ref(),
            _ => None,
        };
        compact.into_iter().chain(body)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, TS)]
#[ts(rename = "ActivityAlert", export, export_to = "../../shared/bindings/")]
pub struct Alert {
    /// Expand briefly to show this update.
    #[serde(default)]
    #[ts(optional)]
    pub expand: Option<bool>,
}

// --- Timers and progress, rendered (6.1) ----------------------------------------

/// A length of time as digits: "0:07", "25:00", "1:10:00".
pub fn clock(seconds: u64) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// A length of time in words: "45 s", "12 min", "1 h 10 min".
pub fn relative(seconds: u64) -> String {
    let (h, m) = (seconds / 3600, seconds / 60 % 60);
    match (h, m) {
        (0, 0) => format!("{seconds} s"),
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

impl Timer {
    fn counts_down(&self) -> bool {
        self.counts_down.unwrap_or(self.end.is_some())
    }

    /// The moment the timer shows: where it was paused, or now.
    fn at(&self, now: Millis) -> Millis {
        self.paused_at.unwrap_or(now)
    }

    /// What is left, or what has passed, in seconds. A countdown rounds
    /// up, so it starts at the full length and reaches zero at its end.
    pub fn seconds(&self, now: Millis) -> u64 {
        let at = self.at(now);
        match (self.counts_down(), self.end) {
            (true, Some(end)) => end.saturating_sub(at).div_ceil(1000),
            _ => at.saturating_sub(self.start) / 1000,
        }
    }

    /// The text for this moment.
    pub fn render(&self, now: Millis) -> String {
        let seconds = self.seconds(now);
        if self.style.as_deref() == Some("relative") {
            relative(seconds)
        } else {
            clock(seconds)
        }
    }

    /// Whether the text changes with the time.
    fn running(&self) -> bool {
        self.paused_at.is_none()
    }

    /// A countdown that has reached its end.
    fn elapsed(&self, now: Millis) -> Option<Millis> {
        match (self.counts_down(), self.end, self.paused_at) {
            (true, Some(end), None) if end <= now => Some(end),
            _ => None,
        }
    }
}

impl Progress {
    /// 0–1, or none for indeterminate progress.
    pub fn fraction(&self, now: Millis) -> Option<f64> {
        if let Some(value) = self.value {
            return Some(value.clamp(0.0, 1.0));
        }
        let interval = self.interval.as_ref()?;
        let end = interval.end?;
        let length = end.saturating_sub(interval.start);
        if length == 0 {
            return Some(1.0);
        }
        let passed = interval.at(now).saturating_sub(interval.start).min(length);
        Some(passed as f64 / length as f64)
    }

    pub fn render(&self, now: Millis) -> String {
        match self.fraction(now) {
            Some(fraction) => format!("{}%", (fraction * 100.0).round() as u32),
            None => "…".into(),
        }
    }

    fn running(&self) -> bool {
        self.value.is_none() && self.interval.as_ref().is_some_and(|interval| interval.end.is_some() && interval.running())
    }
}

/// The first `characters` of a text, with an ellipsis where it was cut.
pub fn truncate(text: &str, characters: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= characters {
        return text.to_owned();
    }
    let kept: String = text.chars().take(characters.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// The compact text of an activity: its compact value rendered for this
/// moment, or its title cut short.
pub fn compact_text(content: &Content, now: Millis) -> String {
    match &content.compact {
        Some(Value::Text(text)) => truncate(text, COMPACT_CHARACTERS),
        Some(Value::Timer { timer }) => timer.render(now),
        Some(Value::Progress { progress }) => progress.render(now),
        Some(Value::Icon { .. }) => String::new(),
        None => truncate(content.title.as_deref().unwrap_or_default(), COMPACT_CHARACTERS),
    }
}

/// The body as one line of text.
pub fn body_text(content: &Content, now: Millis) -> Option<String> {
    match &content.body {
        Some(Body::Text { text }) => Some(text.clone()),
        Some(Body::Timer { timer }) => Some(timer.render(now)),
        Some(Body::Progress { progress }) => Some(progress.render(now)),
        None => None,
    }
}

/// An icon as the palette's rows can show it: a file or a symbol name.
fn icon_name(icon: &Json) -> Option<String> {
    match icon {
        Json::String(name) => Some(name.clone()),
        Json::Object(object) => match object.get("source") {
            Some(Json::String(name)) => Some(name.clone()),
            Some(Json::Object(themed)) => themed.get("dark").or(themed.get("light")).and_then(Json::as_str).map(str::to_owned),
            _ => None,
        },
        _ => None,
    }
}

// --- The table -----------------------------------------------------------------

/// Where an activity comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// The manifest's `name`; `@heyo` for Heyo's own.
    pub extension: String,
    pub title: String,
    /// What `launchCommand` and the index call the extension: its
    /// directory, or `@heyo`.
    pub location: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    pub source: Source,
    pub attributes: Attributes,
    pub content: Content,
    started_at: Millis,
    /// When the content last changed by an update: what fading goes by.
    updated_at: Millis,
    /// When the user last chose an action of it.
    interacted_at: Option<Millis>,
    /// The alerts of the last minute.
    alerts: Vec<Millis>,
    /// Until when it holds attention: its first appearance, or an alert.
    attention_until: Millis,
    /// The sequence number of the last update applied (6.2).
    seq: u64,
    /// An update held back for the one-a-second limit.
    pending: Option<(Content, Option<Alert>)>,
    /// When the content last changed, for that limit.
    changed_at: Millis,
    /// Ended, and to be removed at this time.
    ended_at: Option<Millis>,
    /// Heyo's own dictation: always first (5.6).
    microphone: bool,
    /// The end of the countdown already announced, so it is announced once.
    elapsed_at: Option<Millis>,
}

impl Activity {
    pub fn key(&self) -> String {
        key(&self.source.extension, &self.attributes.id)
    }

    fn live(&self) -> bool {
        self.ended_at.is_none()
    }

    /// Needs the user, or was just alerted or started.
    fn alerting(&self, now: Millis) -> bool {
        self.content.needs_attention.unwrap_or(false) || self.attention_until > now
    }

    fn interacted(&self, now: Millis) -> bool {
        self.interacted_at.is_some_and(|at| now < at + INTERACTED)
    }

    /// What ordering goes by first (5.6): the microphone, then what the user
    /// touched, then what asks for attention, then the rest.
    fn class(&self, now: Millis) -> u8 {
        match () {
            _ if self.microphone => 0,
            _ if self.interacted(now) => 1,
            _ if self.alerting(now) => 2,
            _ => 3,
        }
    }

    fn recency(&self) -> Millis {
        self.updated_at.max(self.interacted_at.unwrap_or(0)).max(self.attention_until)
    }

    fn stale(&self, now: Millis) -> bool {
        self.content.stale_at.is_some_and(|at| at <= now)
    }

    fn dismissible(&self) -> bool {
        self.attributes.dismissible.unwrap_or(true)
    }
}

/// How an activity is known to the palette: `<extension>/<id>`.
pub fn key(extension: &str, id: &str) -> String {
    format!("{extension}/{id}")
}

fn split_key(key: &str) -> Option<(&str, &str)> {
    key.split_once('/').filter(|(extension, id)| !extension.is_empty() && !id.is_empty())
}

/// What a change to the table calls for outside it.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Tell the user, the palette being away. One per activity, replaced.
    Notify { key: String, title: String, body: String },
    /// Tell the extension: a listening command, or its handler command.
    Event {
        source: Source,
        event: Json,
        /// The user caused it, so what runs for it may start activities.
        user: bool,
    },
    Open(String),
    Copy(String),
    Launch { location: String, launch: Launch },
}

/// What an extension is told (6.5).
pub fn event(kind: &str, id: &str, rest: Json) -> Json {
    let mut event = json!({ "kind": kind, "id": id });
    if let Some(rest) = rest.as_object() {
        event.as_object_mut().unwrap().extend(rest.clone());
    }
    event
}

/// How an activity is shown: compact for the first, minimal for the rest
/// and for one that has gone quiet.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "ActivitySize", export, export_to = "../../shared/bindings/")]
pub enum Size {
    Compact,
    Minimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ActivityButtonView", export, export_to = "../../shared/bindings/")]
pub struct ButtonView {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub icon: Option<String>,
    pub destructive: bool,
}

/// An activity as the palette shows it, with its timers rendered for the
/// moment it was asked for.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct ActivityView {
    pub key: String,
    pub extension: String,
    pub extension_title: String,
    pub id: String,
    pub name: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub icon: Option<String>,
    pub compact: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub body: Option<String>,
    pub buttons: Vec<ButtonView>,
    /// Whether choosing the activity itself does something.
    pub has_action: bool,
    pub dismissible: bool,
    pub needs_attention: bool,
    pub stale: bool,
    pub size: Size,
    /// The text changes with the time: a timer or timed progress is running.
    pub live: bool,
    pub ended: bool,
}

#[derive(Debug, Default)]
pub struct Table {
    activities: Vec<Activity>,
    /// Commands listening for events (`onEvent`), by extension.
    listeners: HashMap<String, Vec<u64>>,
}

fn invalid(message: impl Into<String>) -> ExtError {
    ExtError::invalid(message)
}

/// Checks what every activity's content must satisfy.
fn check(attributes: &Attributes, content: &Content) -> Result<(), ExtError> {
    if content.actions.as_ref().is_some_and(|actions| actions.len() > 3) {
        return Err(invalid("An activity has three buttons at most."));
    }
    let size = serde_json::to_vec(attributes).map(|a| a.len()).unwrap_or(0) + serde_json::to_vec(content).map(|c| c.len()).unwrap_or(0);
    if size > MAX_BYTES {
        return Err(invalid(format!("An activity's attributes and content are {MAX_BYTES} bytes at most; these are {size}.")));
    }
    Ok(())
}

impl Table {
    pub fn is_empty(&self) -> bool {
        self.activities.is_empty()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.activities.len()
    }

    fn position(&self, extension: &str, id: &str) -> Option<usize> {
        self.activities.iter().position(|a| a.source.extension == extension && a.attributes.id == id && a.live())
    }

    fn find(&mut self, extension: &str, id: &str) -> Result<&mut Activity, ExtError> {
        let at = self.position(extension, id).ok_or_else(|| invalid(format!("There is no activity {id:?} in progress.")))?;
        Ok(&mut self.activities[at])
    }

    pub fn get(&self, extension: &str, id: &str) -> Option<&Activity> {
        self.position(extension, id).map(|at| &self.activities[at])
    }

    #[cfg(test)]
    fn by_key(&self, key: &str) -> Option<&Activity> {
        let (extension, id) = split_key(key)?;
        self.get(extension, id)
    }

    /// The ids of an extension's activities in progress.
    pub fn ids(&self, extension: &str) -> Vec<String> {
        self.activities.iter().filter(|a| a.source.extension == extension && a.live()).map(|a| a.attributes.id.clone()).collect()
    }

    /// Starts an activity, or updates the one of that id (6.1). `user` says
    /// the request comes from a command the user ran or from an action on
    /// an activity: nothing else may start one (6.7).
    pub fn start(&mut self, source: Source, attributes: Attributes, content: Content, announce: bool, user: bool, seq: u64, now: Millis) -> Result<Vec<Effect>, ExtError> {
        if attributes.id.is_empty() {
            return Err(invalid("An activity needs an id."));
        }
        if let Some(at) = self.position(&source.extension, &attributes.id) {
            check(&attributes, &content)?;
            self.activities[at].attributes = attributes;
            let id = self.activities[at].attributes.id.clone();
            return self.update(&source.extension, &id, content, None, seq, now);
        }
        if !user {
            return Err(ExtError::new(ErrorCode::Denied, "Only a command the user ran, or an action on an activity, can start one; a command run on a schedule can only update its own."));
        }
        if content.title.as_deref().unwrap_or_default().trim().is_empty() {
            return Err(invalid("An activity needs a title."));
        }
        check(&attributes, &content)?;
        if self.ids(&source.extension).len() >= PER_EXTENSION {
            return Err(invalid(format!("{} has {PER_EXTENSION} activities in progress already, which is the most it may have.", source.title)));
        }
        let activity = Activity {
            source,
            attributes,
            content,
            started_at: now,
            updated_at: now,
            interacted_at: None,
            alerts: Vec::new(),
            attention_until: if announce { now + ATTENTION } else { 0 },
            seq,
            pending: None,
            changed_at: now,
            ended_at: None,
            microphone: false,
            elapsed_at: None,
        };
        let mut effects = Vec::new();
        if announce {
            effects.push(notice(&activity, now));
        }
        self.activities.push(activity);
        Ok(effects)
    }

    /// Marks an activity as Heyo's dictation, which is always shown first.
    pub fn set_microphone(&mut self, extension: &str, id: &str, microphone: bool) {
        if let Ok(activity) = self.find(extension, id) {
            activity.microphone = microphone;
        }
    }

    /// Changes an activity's content. An update numbered no higher than the
    /// last applied is stale and dropped (6.2); one within a second of the
    /// last change waits for the next tick, merged with any others (6.6).
    pub fn update(&mut self, extension: &str, id: &str, patch: Content, alert: Option<Alert>, seq: u64, now: Millis) -> Result<Vec<Effect>, ExtError> {
        let activity = self.find(extension, id)?;
        if seq <= activity.seq {
            return Ok(Vec::new());
        }
        let mut merged = activity.content.clone();
        merged.apply(patch.clone());
        check(&activity.attributes, &merged)?;
        activity.seq = seq;
        if now < activity.changed_at + UPDATE_GAP {
            let (mut held, held_alert) = activity.pending.take().unwrap_or_default();
            held.apply(patch);
            activity.pending = Some((held, alert.or(held_alert)));
            return Ok(Vec::new());
        }
        Ok(apply(activity, patch, alert, now))
    }

    /// Ends an activity with its final content; it stays briefly unless
    /// `keep` is `none` (6.4). An end always applies, whatever its number.
    pub fn end(&mut self, extension: &str, id: &str, final_content: Option<Content>, keep: Option<&str>, now: Millis) -> Result<Vec<Effect>, ExtError> {
        let activity = self.find(extension, id)?;
        if let Some((held, _)) = activity.pending.take() {
            activity.content.apply(held);
        }
        if let Some(final_content) = final_content {
            activity.content.apply(final_content);
        }
        activity.updated_at = now;
        activity.ended_at = Some(if keep == Some("none") { now } else { now + KEEP_BRIEF });
        let effects = vec![notice(activity, now)];
        self.remove_ended(now);
        Ok(effects)
    }

    /// The user ended it from the palette.
    pub fn dismiss(&mut self, key: &str, now: Millis) -> Result<Vec<Effect>, ExtError> {
        let (extension, id) = split_key(key).ok_or_else(|| invalid(format!("{key:?} is not an activity.")))?;
        let activity = self.find(extension, id)?;
        if !activity.dismissible() {
            return Err(invalid(format!("{} cannot be ended from here.", activity.attributes.name)));
        }
        let effect = Effect::Event { source: activity.source.clone(), event: event("dismissed", id, Json::Null), user: true };
        activity.ended_at = Some(now);
        self.remove_ended(now);
        Ok(vec![effect])
    }

    /// The user chose a button (by its index) or the activity itself.
    pub fn act(&mut self, key: &str, button: Option<usize>, now: Millis) -> Result<Vec<Effect>, ExtError> {
        let (extension, id) = split_key(key).ok_or_else(|| invalid(format!("{key:?} is not an activity.")))?;
        let activity = self.find(extension, id)?;
        let action = match button {
            None => activity.attributes.action.clone(),
            Some(index) => activity.content.actions.as_ref().and_then(|actions| actions.get(index)).map(|button| button.action.clone()),
        };
        let Some(action) = action else { return Ok(Vec::new()) };
        activity.interacted_at = Some(now);
        let source = activity.source.clone();
        let id = id.to_owned();
        match action {
            Action::Open { open } => Ok(vec![Effect::Open(open)]),
            Action::Copy { copy } => Ok(vec![Effect::Copy(copy)]),
            Action::Launch { launch } => Ok(vec![Effect::Launch { location: source.location, launch }]),
            Action::Event { event: name, data } => Ok(vec![Effect::Event { source, event: event("action", &id, json!({ "event": name, "data": data })), user: true }]),
            Action::End { end } => {
                if end {
                    self.end(extension, &id, None, None, now)
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }

    /// A command wants the events of its extension's activities.
    pub fn listen(&mut self, extension: &str, subscription: u64) {
        self.listeners.entry(extension.to_owned()).or_default().push(subscription);
    }

    pub fn unlisten(&mut self, subscription: u64) {
        self.listeners.retain(|_, subscriptions| {
            subscriptions.retain(|s| *s != subscription);
            !subscriptions.is_empty()
        });
    }

    pub fn listeners_of(&self, extension: &str) -> Vec<u64> {
        self.listeners.get(extension).cloned().unwrap_or_default()
    }

    fn remove_ended(&mut self, now: Millis) {
        self.activities.retain(|a| a.ended_at.is_none_or(|at| at > now));
    }

    /// A second has passed: held updates apply, the quiet fade, the old
    /// end, countdowns that ran out are announced.
    pub fn tick(&mut self, now: Millis) -> Vec<Effect> {
        let mut effects = Vec::new();
        for activity in &mut self.activities {
            if !activity.live() {
                continue;
            }
            if activity.pending.is_some() && now >= activity.changed_at + UPDATE_GAP {
                let (held, alert) = activity.pending.take().unwrap();
                effects.extend(apply(activity, held, alert, now));
            }
            if now >= activity.started_at + LIFETIME || now >= activity.updated_at + END_AFTER {
                effects.push(Effect::Event { source: activity.source.clone(), event: event("expired", &activity.attributes.id, Json::Null), user: false });
                activity.ended_at = Some(now);
                continue;
            }
            let elapsed = activity.content.timers().find_map(|timer| timer.elapsed(now));
            if let Some(end) = elapsed.filter(|end| activity.elapsed_at != Some(*end)) {
                activity.elapsed_at = Some(end);
                effects.push(Effect::Event { source: activity.source.clone(), event: event("elapsed", &activity.attributes.id, json!({ "at": end })), user: false });
            }
        }
        self.remove_ended(now);
        effects
    }

    /// The activities in the order they are shown (5.6). `relevance` only
    /// orders an extension's own activities among the places they hold.
    pub fn ordered(&self, now: Millis) -> Vec<&Activity> {
        let mut order: Vec<&Activity> = self.activities.iter().collect();
        order.sort_by(|a, b| a.class(now).cmp(&b.class(now)).then(b.recency().cmp(&a.recency())));
        let mut extensions: Vec<&str> = order.iter().map(|a| a.source.extension.as_str()).collect();
        extensions.dedup();
        for extension in extensions {
            let places: Vec<usize> = order.iter().enumerate().filter(|(_, a)| a.source.extension == extension).map(|(i, _)| i).collect();
            let mut own: Vec<&Activity> = places.iter().map(|&i| order[i]).collect();
            own.sort_by(|a, b| {
                let relevance = |x: &Activity| x.content.relevance.unwrap_or(0.0);
                a.class(now).cmp(&b.class(now)).then(relevance(b).partial_cmp(&relevance(a)).unwrap_or(std::cmp::Ordering::Equal))
            });
            for (place, activity) in places.into_iter().zip(own) {
                order[place] = activity;
            }
        }
        order
    }

    /// What to show now: the first compact, the rest minimal, all rendered
    /// for this moment.
    pub fn present(&self, now: Millis) -> Vec<ActivityView> {
        self.ordered(now)
            .into_iter()
            .enumerate()
            .map(|(index, a)| {
                let quiet = now >= a.updated_at + DEMOTE_AFTER;
                ActivityView {
                    key: a.key(),
                    extension: a.source.extension.clone(),
                    extension_title: a.source.title.clone(),
                    id: a.attributes.id.clone(),
                    name: a.attributes.name.clone(),
                    title: a.content.title.clone().unwrap_or_default(),
                    subtitle: a.content.subtitle.clone(),
                    icon: icon_name(&a.attributes.icon),
                    compact: compact_text(&a.content, now),
                    body: body_text(&a.content, now),
                    buttons: a
                        .content
                        .actions
                        .iter()
                        .flatten()
                        .map(|b| ButtonView { title: b.title.clone(), icon: b.icon.as_ref().and_then(icon_name), destructive: b.style.as_deref() == Some("destructive") })
                        .collect(),
                    has_action: a.attributes.action.is_some(),
                    dismissible: a.dismissible(),
                    needs_attention: a.content.needs_attention.unwrap_or(false),
                    stale: a.stale(now),
                    size: if index == 0 && !quiet { Size::Compact } else { Size::Minimal },
                    live: a.live() && a.content.changes_by_itself(),
                    ended: !a.live(),
                }
            })
            .collect()
    }
}

/// Puts an update into effect: the content changes, and an alert (two a
/// minute at most) holds attention and reaches the user.
fn apply(activity: &mut Activity, patch: Content, alert: Option<Alert>, now: Millis) -> Vec<Effect> {
    activity.content.apply(patch);
    activity.updated_at = now;
    activity.changed_at = now;
    let mut effects = Vec::new();
    if alert.is_some() {
        activity.alerts.retain(|at| now < *at + 60_000);
        if activity.alerts.len() < ALERTS_PER_MINUTE {
            activity.alerts.push(now);
            activity.attention_until = now + ATTENTION;
            effects.push(notice(activity, now));
        }
    }
    effects
}

/// The notification for an activity's state: its title, with what it is
/// and its compact value.
fn notice(activity: &Activity, now: Millis) -> Effect {
    let title = activity.content.title.clone().unwrap_or_else(|| activity.attributes.name.clone());
    let compact = compact_text(&activity.content, now);
    let body = match (&activity.content.subtitle, compact.as_str()) {
        (Some(subtitle), "") => subtitle.clone(),
        (Some(subtitle), compact) if compact != title => format!("{subtitle} · {compact}"),
        (Some(subtitle), _) => subtitle.clone(),
        (None, compact) if compact != title && !compact.is_empty() => format!("{} · {compact}", activity.attributes.name),
        (None, _) => activity.attributes.name.clone(),
    };
    Effect::Notify { key: activity.key(), title, body }
}

// --- In Heyo -------------------------------------------------------------------

/// The table in Tauri's state, with whether the tick is running and the
/// notification each activity last put up (to replace it).
#[derive(Default)]
pub struct Activities {
    table: Mutex<Table>,
    ticking: AtomicBool,
    notifications: Mutex<HashMap<String, u32>>,
}

pub fn now() -> Millis {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Whether activities are on at all, and for this extension.
pub(crate) fn enabled(settings: &crate::settings::Settings, extension: &str) -> bool {
    settings.live_activities && !settings.activities_off.iter().any(|off| off == extension)
}

impl Activities {
    /// The table, for one who changes it to carry out what the change
    /// calls for (`carry_out`).
    pub(crate) fn table(&self) -> &Mutex<Table> {
        &self.table
    }
}

/// Changes the table and carries out what the change calls for.
pub(crate) fn change<T>(app: &tauri::AppHandle, f: impl FnOnce(&mut Table, Millis) -> Result<(T, Vec<Effect>), ExtError>) -> Result<T, ExtError> {
    use tauri::Manager;
    let state = app.state::<Activities>();
    let (result, effects) = {
        let mut table = state.table.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut table, now())?
    };
    carry_out(app, effects);
    Ok(result)
}

/// What a change to the table calls for: its effects, the palette told,
/// the tick started if it was not.
pub(crate) fn carry_out(app: &tauri::AppHandle, effects: Vec<Effect>) {
    perform(app, effects);
    changed(app);
}

/// Reads the table.
pub(crate) fn read<T>(app: &tauri::AppHandle, f: impl FnOnce(&Table, Millis) -> T) -> T {
    use tauri::Manager;
    let state = app.state::<Activities>();
    let table = state.table.lock().unwrap_or_else(|e| e.into_inner());
    f(&table, now())
}

/// Tells the palette, and keeps the tick going while there is something.
fn changed(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    let state = app.state::<Activities>();
    let (view, empty) = {
        let table = state.table.lock().unwrap_or_else(|e| e.into_inner());
        (table.present(now()), table.is_empty())
    };
    let _ = app.emit(crate::wire::event::ACTIVITIES, &view);
    if !empty && !state.ticking.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        std::thread::spawn(move || tick(app));
    }
}

/// Once a second while there are activities; stops with the last of them.
fn tick(app: tauri::AppHandle) {
    use tauri::Manager;
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let state = app.state::<Activities>();
        let (effects, empty) = {
            let mut table = state.table.lock().unwrap_or_else(|e| e.into_inner());
            let effects = table.tick(now());
            (effects, table.is_empty())
        };
        let any = !effects.is_empty();
        perform(&app, effects);
        if empty {
            state.ticking.store(false, Ordering::SeqCst);
            // The last one went: the palette shows none.
            changed(&app);
            return;
        }
        if any {
            changed(&app);
        }
    }
}

fn perform(app: &tauri::AppHandle, effects: Vec<Effect>) {
    for effect in effects {
        if let Err(e) = perform_one(app, effect) {
            eprintln!("heyo: activities: {e}");
        }
    }
}

fn perform_one(app: &tauri::AppHandle, effect: Effect) -> Result<(), String> {
    use tauri::Manager;
    match effect {
        Effect::Notify { key, title, body } => {
            let state = app.state::<Activities>();
            let previous = state.notifications.lock().unwrap().get(&key).copied();
            if let Some(id) = crate::platform::common::notify_replacing(previous, &title, &body) {
                state.notifications.lock().unwrap().insert(key, id);
            }
        }
        Effect::Event { source, event, user } => deliver(app, &source, event, user),
        Effect::Open(target) => crate::platform::common::open(&target)?,
        Effect::Copy(text) => crate::platform::clipboard_watch::write_text(&text)?,
        // As the root search would open it: in the palette, which comes up
        // for it, unless it lives in the menu bar.
        Effect::Launch { location, launch } => crate::menubar::launch_command(app, &location, &launch.command, false, launch.arguments, launch.context, None)?,
    }
    Ok(())
}

/// An event reaches the commands listening for it, or else the handler
/// command the extension declared, run in the background with the event
/// as its launch context (6.5).
fn deliver(app: &tauri::AppHandle, source: &Source, event: Json, user: bool) {
    use tauri::Manager;
    let listeners = read(app, |table, _| table.listeners_of(&source.extension));
    if !listeners.is_empty() {
        for subscription in listeners {
            crate::ext::emit(app, subscription, event.clone());
        }
        return;
    }
    let handler = {
        let index = app.state::<crate::IndexState>();
        let index = index.0.read().unwrap();
        index.commands().iter().find(|c| c.extension == source.location && c.handles_activities).cloned()
    };
    match handler {
        Some(command) => crate::menubar::run_for_event(app, &command, user, json!({ "activity": event })),
        None => eprintln!("heyo: {} has no command listening for its activities, and none that handles them", source.title),
    }
}

/// Heyo's own activities (section 8), under the `@heyo` name. Nothing
/// starts one yet: dictation would, as it begins to listen (`dictation.rs`,
/// with `microphone`), the engine downloads as they pass two seconds
/// (`dictation::install`, `ocr::install`, `clipboard::meaning`); the AI
/// commands, being runtime code, would go through `LiveActivity` instead.
#[allow(dead_code)]
pub(crate) fn start_builtin(app: &tauri::AppHandle, attributes: Attributes, content: Content, microphone: bool) -> Result<(), ExtError> {
    use tauri::Manager;
    if !enabled(&app.state::<crate::SettingsState>().0.read().unwrap(), crate::extensions::BUILTIN) {
        return Ok(());
    }
    let source = Source { extension: crate::extensions::BUILTIN.into(), title: "Heyo".into(), location: crate::extensions::BUILTIN.into() };
    change(app, |table, now| {
        let id = attributes.id.clone();
        let effects = table.start(source, attributes, content, true, true, now, now)?;
        table.set_microphone(crate::extensions::BUILTIN, &id, microphone);
        Ok(((), effects))
    })
}

#[allow(dead_code)]
pub(crate) fn update_builtin(app: &tauri::AppHandle, id: &str, patch: Content) -> Result<(), ExtError> {
    change(app, |table, now| table.update(crate::extensions::BUILTIN, id, patch, None, now, now).map(|effects| ((), effects)))
}

#[allow(dead_code)]
pub(crate) fn end_builtin(app: &tauri::AppHandle, id: &str, final_content: Option<Content>) -> Result<(), ExtError> {
    change(app, |table, now| table.end(crate::extensions::BUILTIN, id, final_content, None, now).map(|effects| ((), effects)))
}

/// The activities as the palette shows them, now.
#[tauri::command]
pub(crate) fn activities(app: tauri::AppHandle) -> Vec<ActivityView> {
    read(&app, |table, now| table.present(now))
}

/// The user chose a button of an activity (by its index), or the activity
/// itself (no button).
#[tauri::command(async)]
pub(crate) fn activity_action(key: String, button: Option<usize>, app: tauri::AppHandle) -> Result<(), String> {
    change(&app, |table, now| table.act(&key, button, now).map(|effects| ((), effects))).map_err(|e| e.message)
}

/// The user ended an activity.
#[tauri::command(async)]
pub(crate) fn activity_dismiss(key: String, app: tauri::AppHandle) -> Result<(), String> {
    change(&app, |table, now| table.dismiss(&key, now).map(|effects| ((), effects))).map_err(|e| e.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Millis = 60_000;
    const HOUR: Millis = 60 * MINUTE;
    const T0: Millis = 1_700_000_000_000;

    fn source(extension: &str) -> Source {
        Source { extension: extension.into(), title: extension.to_uppercase(), location: format!("/x/{extension}") }
    }

    fn attributes(id: &str) -> Attributes {
        Attributes { id: id.into(), name: "Build".into(), icon: json!("hammer-16"), tint: None, action: None, dismissible: None, fallback: None }
    }

    fn content(title: &str) -> Content {
        Content { title: Some(title.into()), ..Content::default() }
    }

    fn countdown(start: Millis, end: Millis) -> Timer {
        Timer { start, end: Some(end), paused_at: None, counts_down: None, style: None }
    }

    /// Starts as a user would; `seq` is the time, as the runtime numbers them.
    fn start(table: &mut Table, extension: &str, id: &str, title: &str, now: Millis) -> Vec<Effect> {
        table.start(source(extension), attributes(id), content(title), true, true, now, now).unwrap()
    }

    fn titles(table: &Table, now: Millis) -> Vec<String> {
        table.ordered(now).iter().map(|a| a.content.title.clone().unwrap()).collect()
    }

    #[test]
    fn a_timer_is_rendered_for_the_moment_asked() {
        let timer = countdown(T0, T0 + 25 * MINUTE);
        assert_eq!(timer.render(T0), "25:00");
        assert_eq!(timer.render(T0 + 1), "25:00");
        assert_eq!(timer.render(T0 + 1000), "24:59");
        assert_eq!(timer.render(T0 + 24 * MINUTE + 53_000), "0:07");
        assert_eq!(timer.render(T0 + 25 * MINUTE), "0:00");
        // Past its end a countdown stays at zero.
        assert_eq!(timer.render(T0 + 30 * MINUTE), "0:00");
        assert_eq!(countdown(T0, T0 + 70 * MINUTE).render(T0), "1:10:00");
        // Paused: the moment it was paused at, however long ago.
        let paused = Timer { paused_at: Some(T0 + 5 * MINUTE), ..countdown(T0, T0 + 25 * MINUTE) };
        assert_eq!(paused.render(T0 + HOUR), "20:00");
        assert!(!paused.running() && timer.running());
        // Counting up: no end, or asked not to count down.
        let up = Timer { start: T0, end: None, paused_at: None, counts_down: None, style: None };
        assert_eq!(up.render(T0 + 61_500), "1:01");
        assert_eq!(Timer { counts_down: Some(false), ..countdown(T0, T0 + HOUR) }.render(T0 + 2 * HOUR), "2:00:00");
        // In words.
        let words = |seconds: Millis| Timer { style: Some("relative".into()), ..countdown(T0, T0 + seconds * 1000) }.render(T0);
        assert_eq!((words(45), words(12 * 60), words(3600), words(4200)), ("45 s".to_owned(), "12 min".to_owned(), "1 h".to_owned(), "1 h 10 min".to_owned()));
        assert_eq!(clock(0), "0:00");
        assert_eq!(relative(0), "0 s");
    }

    #[test]
    fn progress_is_a_value_or_moves_with_the_time() {
        let fixed = Progress { value: Some(0.423), interval: None };
        assert_eq!((fixed.render(T0), fixed.fraction(T0)), ("42%".into(), Some(0.423)));
        assert_eq!(Progress { value: Some(7.0), interval: None }.render(T0), "100%");
        let timed = Progress { value: None, interval: Some(countdown(T0, T0 + 10_000)) };
        assert_eq!((timed.render(T0), timed.render(T0 + 2500), timed.render(T0 + 20_000)), ("0%".into(), "25%".into(), "100%".into()));
        assert!(timed.running());
        let paused = Progress { value: None, interval: Some(Timer { paused_at: Some(T0 + 5000), ..countdown(T0, T0 + 10_000) }) };
        assert_eq!(paused.render(T0 + 9000), "50%");
        assert!(!paused.running());
        let unknown = Progress { value: None, interval: None };
        assert_eq!((unknown.render(T0), unknown.fraction(T0)), ("…".into(), None));
        assert_eq!(Progress { value: None, interval: Some(countdown(T0, T0)) }.render(T0), "100%");
    }

    #[test]
    fn the_compact_text_is_short_and_falls_back_to_the_title() {
        assert_eq!(truncate("Building the thing", 10), "Building…");
        assert_eq!(truncate("Short", 10), "Short");
        assert_eq!(truncate("  padded  ", 10), "padded");
        assert_eq!(truncate("十个字以内的中文正好十个", 10), "十个字以内的中文正…");
        let mut c = content("Compiling the kernel");
        assert_eq!(compact_text(&c, T0), "Compiling…");
        c.compact = Some(Value::Text("42/100".into()));
        assert_eq!(compact_text(&c, T0), "42/100");
        c.compact = Some(Value::Timer { timer: countdown(T0, T0 + MINUTE) });
        assert_eq!(compact_text(&c, T0 + 30_000), "0:30");
        c.compact = Some(Value::Progress { progress: Progress { value: Some(0.5), interval: None } });
        assert_eq!(compact_text(&c, T0), "50%");
        c.compact = Some(Value::Icon { icon: json!("mic-16") });
        assert_eq!(compact_text(&c, T0), "");
        c.body = Some(Body::Text { text: "All of it".into() });
        assert_eq!(body_text(&c, T0).as_deref(), Some("All of it"));
        c.body = Some(Body::Timer { timer: countdown(T0, T0 + HOUR) });
        assert_eq!(body_text(&c, T0).as_deref(), Some("1:00:00"));
        assert_eq!(icon_name(&json!("clock-16")).as_deref(), Some("clock-16"));
        assert_eq!(icon_name(&json!({ "source": "a.png" })).as_deref(), Some("a.png"));
        assert_eq!(icon_name(&json!({ "source": { "light": "l.png", "dark": "d.png" } })).as_deref(), Some("d.png"));
        assert_eq!(icon_name(&json!(3)), None);
    }

    #[test]
    fn the_content_is_read_as_extensions_write_it() {
        let content: Content = serde_json::from_value(json!({
            "compact": { "timer": { "start": 1, "end": 2 } },
            "title": "Focus",
            "body": { "progress": { "value": 0.5 } },
            "actions": [
                { "title": "Open", "action": { "open": "https://x" } },
                { "title": "Pause", "action": { "event": "pause", "data": { "end": 2 } }, "style": "destructive" },
                { "title": "Stop", "action": { "end": true } },
            ],
            "staleAt": 9,
        }))
        .unwrap();
        assert_eq!(content.compact, Some(Value::Timer { timer: Timer { start: 1, end: Some(2), paused_at: None, counts_down: None, style: None } }));
        assert_eq!(content.body, Some(Body::Progress { progress: Progress { value: Some(0.5), interval: None } }));
        let actions = content.actions.as_ref().unwrap();
        assert_eq!(actions[0].action, Action::Open { open: "https://x".into() });
        assert_eq!(actions[1].action, Action::Event { event: "pause".into(), data: Some(json!({ "end": 2 })) });
        assert_eq!(actions[2].action, Action::End { end: true });
        assert_eq!(content.stale_at, Some(9));
        let copy: Action = serde_json::from_value(json!({ "copy": "text" })).unwrap();
        assert_eq!(copy, Action::Copy { copy: "text".into() });
        let launch: Action = serde_json::from_value(json!({ "launch": { "command": "show", "arguments": { "id": "4" } } })).unwrap();
        assert!(matches!(launch, Action::Launch { launch } if launch.command == "show"));
        assert!(serde_json::from_value::<Action>(json!({ "fly": true })).is_err());
        assert_eq!(serde_json::from_value::<Value>(json!("plain")).unwrap(), Value::Text("plain".into()));
        // What a patch leaves out stays.
        let mut c = content.clone();
        c.apply(Content { title: Some("Rest".into()), actions: Some(Vec::new()), ..Content::default() });
        assert_eq!((c.title.as_deref(), c.actions.as_ref().map(Vec::len), c.stale_at), (Some("Rest"), Some(0), Some(9)));
        assert!(c.compact.is_some());
    }

    #[test]
    fn an_activity_starts_and_is_shown_until_it_ends() {
        let mut table = Table::default();
        let effects = start(&mut table, "ci", "build-1", "Building", T0);
        assert_eq!(effects, [Effect::Notify { key: "ci/build-1".into(), title: "Building".into(), body: "Build".into() }]);
        assert_eq!(table.ids("ci"), ["build-1"]);
        assert_eq!(table.by_key("ci/build-1").unwrap().key(), "ci/build-1");
        assert!(table.by_key("ci").is_none() && table.by_key("/x").is_none());

        // Quiet starts tell nobody.
        assert!(table.start(source("ci"), attributes("build-2"), content("Testing"), false, true, T0, T0).unwrap().is_empty());
        let shown = table.present(T0);
        assert_eq!(shown.iter().map(|v| v.title.as_str()).collect::<Vec<_>>(), ["Building", "Testing"]);
        assert_eq!((shown[0].size, shown[1].size), (Size::Compact, Size::Minimal));
        assert_eq!((shown[0].compact.as_str(), shown[0].icon.as_deref(), shown[0].dismissible, shown[0].ended), ("Building", Some("hammer-16"), true, false));

        // Ended, it stays briefly with its final content, then goes.
        let effects = table.end("ci", "build-1", Some(content("Built")), None, T0 + 10_000).unwrap();
        assert_eq!(effects, [Effect::Notify { key: "ci/build-1".into(), title: "Built".into(), body: "Build".into() }]);
        assert_eq!(table.ids("ci"), ["build-2"]);
        assert!(table.present(T0 + 10_000).iter().any(|v| v.title == "Built" && v.ended));
        table.tick(T0 + 10_000 + KEEP_BRIEF);
        assert_eq!(table.len(), 1);
        // Or at once.
        table.end("ci", "build-2", None, Some("none"), T0 + 20_000).unwrap();
        assert!(table.is_empty());
        // Nothing to end twice.
        assert_eq!(table.end("ci", "build-2", None, None, T0 + 20_000).unwrap_err().code, ErrorCode::Invalid);
    }

    #[test]
    fn what_must_be_said_of_a_start() {
        let mut table = Table::default();
        let fails = |table: &mut Table, attributes: Attributes, content: Content| table.start(source("ci"), attributes, content, true, true, T0, T0).unwrap_err().code;
        assert_eq!(fails(&mut table, attributes(""), content("x")), ErrorCode::Invalid);
        assert_eq!(fails(&mut table, attributes("a"), Content::default()), ErrorCode::Invalid);
        assert_eq!(fails(&mut table, attributes("a"), content("  ")), ErrorCode::Invalid);
        let four = (0..4).map(|i| Button { title: format!("{i}"), icon: None, style: None, action: Action::End { end: true } }).collect();
        assert_eq!(fails(&mut table, attributes("a"), Content { actions: Some(four), ..content("x") }), ErrorCode::Invalid);
        assert_eq!(fails(&mut table, attributes("a"), content(&"x".repeat(MAX_BYTES))), ErrorCode::Invalid);
        assert!(table.is_empty());
    }

    #[test]
    fn only_the_user_starts_an_activity_and_an_extension_has_three() {
        let mut table = Table::default();
        // A scheduled run may not start one...
        let refused = table.start(source("ci"), attributes("a"), content("A"), true, false, T0, T0).unwrap_err();
        assert_eq!(refused.code, ErrorCode::Denied);
        // ...but may update its own, by starting it again.
        start(&mut table, "ci", "a", "A", T0);
        assert!(table.start(source("ci"), attributes("a"), content("A2"), true, false, T0 + 2000, T0 + 2000).is_ok());
        assert_eq!(table.get("ci", "a").unwrap().content.title.as_deref(), Some("A2"));
        assert_eq!(table.ids("ci"), ["a"]);

        start(&mut table, "ci", "b", "B", T0);
        start(&mut table, "ci", "c", "C", T0);
        assert_eq!(table.start(source("ci"), attributes("d"), content("D"), true, true, T0, T0).unwrap_err().code, ErrorCode::Invalid);
        // Another extension has its own three.
        start(&mut table, "dl", "a", "Download", T0);
        assert_eq!(table.len(), 4);
        // Room again once one ends.
        table.end("ci", "a", None, Some("none"), T0).unwrap();
        assert!(table.start(source("ci"), attributes("d"), content("D"), true, true, T0, T0).is_ok());
    }

    #[test]
    fn a_late_old_update_is_dropped_and_fast_ones_are_merged() {
        let mut table = Table::default();
        start(&mut table, "ci", "a", "Step 1", T0);
        let step = |n: u32| content(&format!("Step {n}"));
        // Numbered after the start, and a second after it: applied.
        assert!(table.update("ci", "a", step(2), None, T0 + 1000, T0 + 1000).unwrap().is_empty());
        assert_eq!(table.get("ci", "a").unwrap().content.title.as_deref(), Some("Step 2"));
        // The command that started it sends an old update late: dropped.
        table.update("ci", "a", step(1), None, T0 + 500, T0 + 1100).unwrap();
        assert_eq!(table.get("ci", "a").unwrap().content.title.as_deref(), Some("Step 2"));
        // Two within the same second as the last change: held, as one.
        table.update("ci", "a", Content { subtitle: Some("x".into()), ..Content::default() }, None, T0 + 1200, T0 + 1200).unwrap();
        table.update("ci", "a", step(3), None, T0 + 1300, T0 + 1300).unwrap();
        assert_eq!(table.get("ci", "a").unwrap().content.title.as_deref(), Some("Step 2"));
        assert!(table.tick(T0 + 1500).is_empty());
        assert_eq!(table.get("ci", "a").unwrap().content.title.as_deref(), Some("Step 2"));
        table.tick(T0 + 2000);
        let now = table.get("ci", "a").unwrap();
        assert_eq!((now.content.title.as_deref(), now.content.subtitle.as_deref()), (Some("Step 3"), Some("x")));
        // Ending applies what was held, then the final content.
        table.update("ci", "a", step(4), None, T0 + 2100, T0 + 2100).unwrap();
        table.end("ci", "a", Some(Content { subtitle: Some("done".into()), ..Content::default() }), None, T0 + 2200).unwrap();
        let ended = table.activities.iter().find(|a| a.attributes.id == "a").unwrap();
        assert_eq!((ended.content.title.as_deref(), ended.content.subtitle.as_deref()), (Some("Step 4"), Some("done")));
        // No update to what is not there.
        assert_eq!(table.update("ci", "zz", step(1), None, T0 + 9000, T0 + 9000).unwrap_err().code, ErrorCode::Invalid);
    }

    #[test]
    fn an_alert_reaches_the_user_twice_a_minute_at_most() {
        let mut table = Table::default();
        start(&mut table, "ci", "a", "Build", T0);
        let alert = |table: &mut Table, n: u64| table.update("ci", "a", content(&format!("Alert {n}")), Some(Alert { expand: Some(true) }), T0 + n * 2000, T0 + n * 2000).unwrap();
        assert_eq!(alert(&mut table, 1).len(), 1);
        assert_eq!(alert(&mut table, 2).len(), 1);
        // The third within the minute is a plain update: applied, unannounced.
        assert!(alert(&mut table, 3).is_empty());
        assert_eq!(table.get("ci", "a").unwrap().content.title.as_deref(), Some("Alert 3"));
        // A minute on, there is room again.
        assert_eq!(alert(&mut table, 31).len(), 1);
        // Without an alert, an update tells nobody (no progress by notification).
        assert!(table.update("ci", "a", content("Quiet"), None, T0 + 80_000, T0 + 80_000).unwrap().is_empty());
    }

    #[test]
    fn a_quiet_activity_fades_and_an_old_one_ends() {
        let mut table = Table::default();
        start(&mut table, "ci", "a", "Build", T0);
        assert_eq!(table.present(T0 + DEMOTE_AFTER - 1)[0].size, Size::Compact);
        assert_eq!(table.present(T0 + DEMOTE_AFTER)[0].size, Size::Minimal);
        // An update makes it current again.
        table.update("ci", "a", content("Still building"), None, T0 + DEMOTE_AFTER, T0 + DEMOTE_AFTER).unwrap();
        assert_eq!(table.present(T0 + DEMOTE_AFTER + 1)[0].size, Size::Compact);
        // Four quiet hours end it, and the extension is told.
        assert!(table.tick(T0 + DEMOTE_AFTER + END_AFTER - 1).is_empty());
        let effects = table.tick(T0 + DEMOTE_AFTER + END_AFTER);
        assert_eq!(effects, [Effect::Event { source: source("ci"), event: json!({ "kind": "expired", "id": "a" }), user: false }]);
        assert!(table.is_empty());

        // Eight hours end it however often it is updated.
        start(&mut table, "ci", "b", "Long", T0);
        for hour in 1..8 {
            table.update("ci", "b", content("Long"), None, T0 + hour * HOUR, T0 + hour * HOUR).unwrap();
        }
        assert!(table.tick(T0 + LIFETIME - 1).is_empty());
        assert_eq!(table.tick(T0 + LIFETIME).len(), 1);
        assert!(table.is_empty());

        // Stale only dims.
        table.start(source("ci"), attributes("c"), Content { stale_at: Some(T0 + MINUTE), ..content("Maybe") }, false, true, T0, T0).unwrap();
        assert!(!table.present(T0 + MINUTE - 1)[0].stale);
        assert!(table.present(T0 + MINUTE)[0].stale);
        assert!(table.tick(T0 + HOUR).is_empty() && table.len() == 1);
    }

    #[test]
    fn a_countdown_that_runs_out_is_announced_once() {
        let mut table = Table::default();
        let timer = countdown(T0, T0 + 10_000);
        table.start(source("@heyo"), attributes("t"), Content { compact: Some(Value::Timer { timer: timer.clone() }), body: Some(Body::Timer { timer }), ..content("Timer") }, false, true, T0, T0).unwrap();
        assert!(table.present(T0)[0].live);
        assert!(table.tick(T0 + 9999).is_empty());
        let effects = table.tick(T0 + 10_000);
        assert_eq!(effects, [Effect::Event { source: source("@heyo"), event: json!({ "kind": "elapsed", "id": "t", "at": T0 + 10_000 }), user: false }]);
        assert!(table.tick(T0 + 11_000).is_empty());
        // Given a minute more, it runs out again, once more.
        let longer = countdown(T0, T0 + 70_000);
        table.update("@heyo", "t", Content { compact: Some(Value::Timer { timer: longer.clone() }), body: Some(Body::Timer { timer: longer }), ..Content::default() }, None, T0 + 12_000, T0 + 12_000).unwrap();
        assert!(table.tick(T0 + 60_000).is_empty());
        assert_eq!(table.tick(T0 + 70_000).len(), 1);
        // A paused one never runs out.
        let paused = Timer { paused_at: Some(T0 + 71_000), ..countdown(T0, T0 + 80_000) };
        let content = Content { compact: Some(Value::Timer { timer: paused.clone() }), body: Some(Body::Timer { timer: paused }), ..Content::default() };
        table.update("@heyo", "t", content, None, T0 + 71_000, T0 + 71_000).unwrap();
        assert!(!table.present(T0 + 71_000)[0].live);
        assert!(table.tick(T0 + HOUR).is_empty());
    }

    #[test]
    fn the_user_acts_on_an_activity() {
        let mut table = Table::default();
        let buttons = vec![
            Button { title: "Open".into(), icon: Some(json!("globe-16")), style: None, action: Action::Open { open: "https://ci.example/1".into() } },
            Button { title: "Copy".into(), icon: None, style: None, action: Action::Copy { copy: "log".into() } },
            Button { title: "Pause".into(), icon: None, style: Some("destructive".into()), action: Action::Event { event: "pause".into(), data: Some(json!({ "end": 5 })) } },
        ];
        let attrs = Attributes {
            action: Some(Action::Launch { launch: Launch { command: "show".into(), arguments: Some(json!({ "id": "1" })), context: None } }),
            ..attributes("a")
        };
        table.start(source("ci"), attrs, Content { actions: Some(buttons), ..content("Build") }, false, true, T0, T0).unwrap();
        let shown = &table.present(T0)[0];
        assert_eq!(shown.buttons.iter().map(|b| (b.title.as_str(), b.icon.as_deref(), b.destructive)).collect::<Vec<_>>(), [("Open", Some("globe-16"), false), ("Copy", None, false), ("Pause", None, true)]);
        assert!(shown.has_action);

        assert_eq!(table.act("ci/a", None, T0 + 1).unwrap(), [Effect::Launch { location: "/x/ci".into(), launch: Launch { command: "show".into(), arguments: Some(json!({ "id": "1" })), context: None } }]);
        assert_eq!(table.act("ci/a", Some(0), T0 + 2).unwrap(), [Effect::Open("https://ci.example/1".into())]);
        assert_eq!(table.act("ci/a", Some(1), T0 + 3).unwrap(), [Effect::Copy("log".into())]);
        assert_eq!(
            table.act("ci/a", Some(2), T0 + 4).unwrap(),
            [Effect::Event { source: source("ci"), event: json!({ "kind": "action", "id": "a", "event": "pause", "data": { "end": 5 } }), user: true }]
        );
        assert!(table.act("ci/a", Some(7), T0 + 5).unwrap().is_empty());
        assert_eq!(table.act("ci/zz", None, T0).unwrap_err().code, ErrorCode::Invalid);
        assert_eq!(table.act("nonsense", None, T0).unwrap_err().code, ErrorCode::Invalid);

        // An "end" button ends it, like the user dismissing it.
        table.start(source("ci"), attributes("b"), Content { actions: Some(vec![Button { title: "Stop".into(), icon: None, style: None, action: Action::End { end: true } }]), ..content("Test") }, false, true, T0, T0).unwrap();
        assert!(matches!(table.act("ci/b", Some(0), T0 + 6).unwrap()[..], [Effect::Notify { .. }]));
        assert_eq!(table.ids("ci"), ["a"]);
        assert_eq!(table.dismiss("ci/a", T0 + 7).unwrap(), [Effect::Event { source: source("ci"), event: json!({ "kind": "dismissed", "id": "a" }), user: true }]);
        assert!(table.ids("ci").is_empty());
        // The dismissed one is gone at once; the stopped one stays its moment.
        assert_eq!(table.len(), 1);
        table.tick(T0 + 6 + KEEP_BRIEF);
        assert!(table.is_empty());

        // One that may not be dismissed.
        table.start(source("ci"), Attributes { dismissible: Some(false), ..attributes("c") }, content("Fixed"), false, true, T0, T0).unwrap();
        assert!(!table.present(T0)[0].dismissible);
        assert_eq!(table.dismiss("ci/c", T0).unwrap_err().code, ErrorCode::Invalid);
    }

    #[test]
    fn activities_are_ordered_by_what_matters_to_the_user() {
        let mut table = Table::default();
        start(&mut table, "ci", "old", "Old", T0);
        start(&mut table, "ci", "new", "New", T0 + 1000);
        start(&mut table, "dl", "file", "File", T0 + 2000);
        let later = T0 + ATTENTION + 10_000;
        // Nothing special: the most recently updated first.
        assert_eq!(titles(&table, later), ["File", "New", "Old"]);
        // Alerting comes before that.
        table.update("ci", "old", Content { needs_attention: Some(true), ..Content::default() }, None, later, later).unwrap();
        assert_eq!(titles(&table, later + 1), ["Old", "File", "New"]);
        // What the user touched comes before alerting, for a while.
        table.act("ci/new", Some(0), later + 2).unwrap();
        table.start(source("ci"), Attributes { action: Some(Action::Copy { copy: "x".into() }), ..attributes("new") }, Content::default(), false, true, later + 2, later + 2).unwrap();
        table.act("ci/new", None, later + 3).unwrap();
        assert_eq!(titles(&table, later + 4), ["New", "Old", "File"]);
        // Then alerting goes first again, and the touched one is the more recent of the rest.
        assert_eq!(titles(&table, later + INTERACTED + 4), ["Old", "New", "File"]);
        // The microphone before everything.
        start(&mut table, "@heyo", "dictation", "Listening", T0);
        table.set_microphone("@heyo", "dictation", true);
        assert_eq!(titles(&table, later + 5), ["Listening", "New", "Old", "File"]);
    }

    #[test]
    fn relevance_orders_an_extensions_own_activities_only() {
        let mut table = Table::default();
        let with = |table: &mut Table, ext: &str, id: &str, relevance: f64, at: Millis| {
            table.start(source(ext), attributes(id), Content { relevance: Some(relevance), ..content(id) }, false, true, at, at).unwrap();
        };
        with(&mut table, "ci", "ci-low", 10.0, T0);
        with(&mut table, "dl", "dl-mid", 50.0, T0 + 1000);
        with(&mut table, "ci", "ci-high", 90.0, T0 + 2000);
        with(&mut table, "dl", "dl-top", 100.0, T0 + 3000);
        // By recency the order would be dl-top, ci-high, dl-mid, ci-low;
        // each extension's places are its own, and relevance fills them.
        assert_eq!(titles(&table, T0 + 4000), ["dl-top", "ci-high", "dl-mid", "ci-low"]);
        table.update("ci", "ci-low", Content { relevance: Some(99.0), ..Content::default() }, None, T0 + 4000, T0 + 4000).unwrap();
        table.update("dl", "dl-mid", Content { relevance: Some(100.0), ..Content::default() }, None, T0 + 4001, T0 + 4001).unwrap();
        // Updating moved both up by recency; relevance then decides between each pair.
        let order = titles(&table, T0 + 5000);
        assert_eq!(order, ["dl-mid", "ci-low", "dl-top", "ci-high"]);
        // Filling in 100 everywhere gains an extension nothing over another.
        let shown = table.present(T0 + 5000);
        assert_eq!(shown[0].size, Size::Compact);
        assert!(shown[1..].iter().all(|v| v.size == Size::Minimal));
    }

    #[test]
    fn listeners_are_kept_by_extension() {
        let mut table = Table::default();
        table.listen("ci", 4);
        table.listen("ci", 5);
        table.listen("dl", 6);
        assert_eq!(table.listeners_of("ci"), [4, 5]);
        table.unlisten(4);
        assert_eq!((table.listeners_of("ci"), table.listeners_of("dl"), table.listeners_of("x")), (vec![5], vec![6], vec![]));
        table.unlisten(5);
        assert!(table.listeners.get("ci").is_none());
    }

    #[test]
    fn a_notice_says_what_and_how_far() {
        let mut table = Table::default();
        let effects = table
            .start(source("ci"), attributes("a"), Content { subtitle: Some("main".into()), compact: Some(Value::Text("3/9".into())), ..content("Building") }, true, true, T0, T0)
            .unwrap();
        assert_eq!(effects, [Effect::Notify { key: "ci/a".into(), title: "Building".into(), body: "main · 3/9".into() }]);
        let effects = table.end("ci", "a", Some(Content { subtitle: None, compact: Some(Value::Text("Building".into())), ..Content::default() }), None, T0 + 2000).unwrap();
        assert_eq!(effects, [Effect::Notify { key: "ci/a".into(), title: "Building".into(), body: "main".into() }]);
        assert_eq!(event("action", "a", json!({ "event": "x", "data": null })), json!({ "kind": "action", "id": "a", "event": "x", "data": null }));
        assert_eq!(key("ci", "a"), "ci/a");
        assert_eq!(split_key("ci/a/b"), Some(("ci", "a/b")));
        assert_eq!(split_key("ci/"), None);
    }

    #[test]
    fn the_view_is_plain_data_for_the_palette() {
        let mut table = Table::default();
        start(&mut table, "ci", "a", "Build", T0);
        let view = serde_json::to_value(&table.present(T0)[0]).unwrap();
        assert_eq!(view["key"], "ci/a");
        assert_eq!(view["size"], "compact");
        assert_eq!(view["extensionTitle"], "CI");
        assert_eq!(view["buttons"], json!([]));
        assert!(view.get("subtitle").is_none() && view.get("body").is_none());
    }
}
