//! Where the connections keep what they sync: mail, events, messages,
//! documents and tasks, each an `Item` of an account. One SQLite file
//! (`connections.db` in Heyo's data directory), readable by the user alone,
//! with the two full-text indexes of `text` over each item's title, people,
//! place, attachments and text, and vectors of its title and the beginning
//! of its text for finding it by meaning.
//!
//! A connector puts what it fetched (`put`, which never waits for the
//! model) and takes away what is gone (`remove`, `prune`, `drop_account`).
//! The vectors are made afterwards in the background, as the clipboard's
//! are, once `start_meaning` has been given a model.
//!
//! A search reads a time, a person and a kind out of the query
//! (`query.rs`), finds the items whose words match, nearest to now first,
//! and, when the query's vector is there, puts what its meaning finds in
//! the same order (`fusion`).

pub mod query;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use self::query::Query;
use super::embed::Embed;
use super::fusion::{self, Tuning};
use super::meaning::{self, Queries, Vector, Vectored, Waiting};
use super::text;
use super::vectors::{blob, floats, language, Index};

/// Longer text is cut to this many characters before it is stored: enough
/// to find a mail by, and a bound on the file.
const MAX_TEXT: usize = 8000;
/// How much of the text is read for its meaning, with the title, in characters.
const READ: usize = 512;
/// How many items are given their vectors in one go.
const BATCH: usize = 8;
/// While finding by meaning is on, how long the vectors wait when nothing
/// wakes them (a `put` does). It grows after a failure.
const NAP: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum ItemKind {
    Email,
    Event,
    Message,
    Document,
    Task,
}

impl ItemKind {
    fn name(self) -> &'static str {
        match self {
            ItemKind::Email => "email",
            ItemKind::Event => "event",
            ItemKind::Message => "message",
            ItemKind::Document => "document",
            ItemKind::Task => "task",
        }
    }

    fn named(name: &str) -> ItemKind {
        match name {
            "event" => ItemKind::Event,
            "message" => ItemKind::Message,
            "document" => ItemKind::Document,
            "task" => ItemKind::Task,
            _ => ItemKind::Email,
        }
    }
}

/// The part someone has in an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum PersonRole {
    From,
    To,
    Cc,
    Organizer,
    Attendee,
}

impl PersonRole {
    fn name(self) -> &'static str {
        match self {
            PersonRole::From => "from",
            PersonRole::To => "to",
            PersonRole::Cc => "cc",
            PersonRole::Organizer => "organizer",
            PersonRole::Attendee => "attendee",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ItemPerson")]
pub struct Person {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub address: Option<String>,
    pub role: PersonRole,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ItemAttachment")]
pub struct Attachment {
    pub name: String,
    /// In bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, as = "Option<f64>")]
    pub size: Option<u64>,
    /// The media type, "application/pdf".
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime: Option<String>,
}

/// One thing a connector synced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectionItem")]
pub struct Item {
    /// The connector's own id for it, unique within the account.
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub people: Option<Vec<Person>>,
    /// ISO 8601: when it was sent, or when it begins. With an offset, or
    /// without one for local time; a date alone for a whole day.
    pub time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end: Option<String>,
    /// Cut to 8000 characters by the store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub attachments: Option<Vec<Attachment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub flags: Option<Vec<String>>,
}

/// An item a search found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectionFound")]
pub struct Found {
    pub account: String,
    pub item: Item,
    /// Higher is better: the item's places in what the words and the
    /// meaning found, by reciprocal rank (`fusion::scored`).
    pub score: f32,
}

/// Finding by meaning, once it has been started.
struct Meaning {
    model: Arc<dyn Embed>,
    enabled: Box<dyn Fn() -> bool + Send + Sync>,
}

pub(crate) struct Items {
    db: Mutex<Connection>,
    /// The vectors, in memory, read when they are first searched.
    vectors: Mutex<Option<Index>>,
    /// Items were deleted or changed since the vectors were read.
    stale: AtomicBool,
    queries: Queries,
    meaning: OnceLock<Meaning>,
    /// Whether there may be items without vectors.
    work: Mutex<bool>,
    work_signal: Condvar,
}

const COLUMNS: &str = "rowid, account, item_id, kind, title, people_json, time, \"end\", text, url, location, attachments_json, folder, flags";

fn json<T: Serialize>(value: &Option<Vec<T>>) -> Option<String> {
    value.as_ref().filter(|list| !list.is_empty()).and_then(|list| serde_json::to_string(list).ok())
}

fn parsed<T: for<'a> Deserialize<'a>>(text: Option<String>) -> Option<T> {
    text.and_then(|text| serde_json::from_str(&text).ok())
}

/// A row as (rowid, account, item).
fn row(row: &Row) -> rusqlite::Result<(i64, String, Item)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        Item {
            id: row.get(2)?,
            kind: ItemKind::named(&row.get::<_, String>(3)?),
            title: row.get(4)?,
            people: parsed(row.get(5)?),
            time: row.get(6)?,
            end: row.get(7)?,
            text: row.get(8)?,
            url: row.get(9)?,
            location: row.get(10)?,
            attachments: parsed(row.get(11)?),
            folder: row.get(12)?,
            flags: parsed(row.get(13)?),
        },
    ))
}

/// Milliseconds since the epoch of an ISO 8601 time: with an offset, or
/// local without one, or the local midnight a date begins at.
fn instant(text: &str) -> Option<i64> {
    let text = text.trim();
    if let Ok(time) = DateTime::parse_from_rfc3339(text) {
        return Some(time.timestamp_millis());
    }
    let local = ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M"]
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
        .or_else(|| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok().map(|date| date.and_time(chrono::NaiveTime::MIN)))?;
    Local.from_local_datetime(&local).earliest().map(|time| time.timestamp_millis())
}

fn cut(text: &str, chars: usize) -> &str {
    match text.char_indices().nth(chars) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

/// What of an item the words are found in.
fn searched(item: &Item, text: Option<&str>) -> String {
    let mut parts: Vec<&str> = vec![&item.title];
    for person in item.people.iter().flatten() {
        parts.extend(person.name.as_deref());
        parts.extend(person.address.as_deref());
    }
    parts.extend(item.location.as_deref());
    parts.extend(item.attachments.iter().flatten().map(|attachment| attachment.name.as_str()));
    parts.extend(text);
    parts.join("\n")
}

/// What of an item is read for its meaning, if anything is worth it.
fn worth(title: &str, text: Option<&str>) -> Option<String> {
    let read = format!("{}\n{}", title.trim(), cut(text.unwrap_or_default(), READ).trim());
    let read = read.trim();
    (read.chars().count() >= 3 && read.chars().any(char::is_alphabetic)).then(|| read.to_owned())
}

fn invalid(what: String) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(what.into())
}

/// The conditions an item must meet whatever its words: the time, person,
/// kind and attachments read out of the query.
fn filters(query: &Query) -> (Vec<String>, Vec<Value>) {
    let mut conditions = Vec::new();
    let mut values = Vec::new();
    if let Some(time) = &query.time {
        // Any part of it in the span: a meeting that runs into it too.
        conditions.push("at < ? AND coalesce(until, at) >= ?".to_owned());
        values.extend([Value::Integer(time.end), Value::Integer(time.start)]);
    }
    if let Some(someone) = &query.someone {
        let roles = someone.roles.iter().map(|role| format!("'{}'", role.name())).collect::<Vec<_>>().join(", ");
        conditions.push(format!("rowid IN (SELECT item FROM people WHERE role IN ({roles}) AND (instr(name, ?) > 0 OR substr(address, 1, length(?)) = ?))"));
        values.extend(std::iter::repeat_n(Value::Text(someone.text.clone()), 3));
    }
    if let Some(kind) = query.kind {
        conditions.push("kind = ?".to_owned());
        values.push(Value::Text(kind.name().into()));
    }
    if query.attachments {
        conditions.push("attachments_json IS NOT NULL".to_owned());
    }
    (conditions, values)
}

/// Whether some item is from or to someone whose name has `text` in it or
/// whose address begins with it.
fn known(db: &Connection, text: &str) -> bool {
    db.prepare_cached("SELECT EXISTS (SELECT 1 FROM people WHERE instr(name, ?1) > 0 OR substr(address, 1, length(?1)) = ?1)")
        .and_then(|mut statement| statement.query_row([text], |row| row.get(0)))
        .unwrap_or(false)
}

/// The items the words of a query find, with its time, person and kind,
/// nearest to `now` first: the latest mail, the next meeting.
fn run(db: &Connection, query: &Query, limit: usize, now: i64) -> rusqlite::Result<Vec<(i64, String, Item)>> {
    let (mut conditions, mut values) = filters(query);
    text::matching(&query.terms, "rowid", "(title || ' ' || coalesce(text, ''))", &mut conditions, &mut values);
    let filter = if conditions.is_empty() { String::new() } else { format!("WHERE {}", conditions.join(" AND ")) };
    values.push(Value::Integer(now));
    let mut statement = db.prepare_cached(&format!("SELECT {COLUMNS} FROM items {filter} ORDER BY abs(at - ?), rowid DESC LIMIT {limit}"))?;
    let rows = statement.query_map(params_from_iter(values), row)?;
    rows.collect()
}

/// The items that pass the query's time, person and kind whatever their
/// words; `None` when it asks for none of those.
fn passing(db: &Connection, query: &Query) -> rusqlite::Result<Option<HashSet<u64>>> {
    let (conditions, values) = filters(query);
    if conditions.is_empty() {
        return Ok(None);
    }
    let mut statement = db.prepare_cached(&format!("SELECT rowid FROM items WHERE {}", conditions.join(" AND ")))?;
    let rows = statement.query_map(params_from_iter(values), |row| row.get::<_, i64>(0).map(|id| id as u64))?;
    rows.collect::<rusqlite::Result<_>>().map(Some)
}

impl Items {
    /// Opens the items at `path`, creating the file readable by the user alone.
    pub(crate) fn open(path: &Path) -> rusqlite::Result<Items> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Readable by the user alone before anything is in it.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let _ = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(path);
        }
        let items = Items::with(Connection::open(path)?)?;
        for suffix in ["", "-wal", "-shm"] {
            let mut name = path.as_os_str().to_owned();
            name.push(suffix);
            super::private(Path::new(&name), 0o600);
        }
        Ok(items)
    }

    /// Where the connections' items are kept.
    pub(crate) fn path() -> PathBuf {
        crate::paths::data().join("connections.db")
    }

    /// Items that are forgotten when Heyo exits.
    pub(crate) fn in_memory() -> Items {
        Items::with(Connection::open_in_memory().expect("an in-memory database")).expect("the schema")
    }

    fn with(db: Connection) -> rusqlite::Result<Items> {
        // Deleted means gone: overwrite what is deleted rather than leave it in free pages.
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA secure_delete = ON;
             CREATE TABLE IF NOT EXISTS items (
                 rowid INTEGER PRIMARY KEY,
                 account TEXT NOT NULL,
                 item_id TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 title TEXT NOT NULL,
                 people_json TEXT,
                 time TEXT NOT NULL,
                 \"end\" TEXT,
                 text TEXT,
                 url TEXT,
                 location TEXT,
                 attachments_json TEXT,
                 folder TEXT,
                 flags TEXT,
                 updated INTEGER NOT NULL,
                 -- The time and the end in milliseconds since the epoch: what is searched by.
                 at INTEGER NOT NULL,
                 until INTEGER,
                 UNIQUE (account, item_id)
             );
             CREATE INDEX IF NOT EXISTS items_at ON items (at);
             -- Who an item is from and to, in lower case: what a person is found by.
             CREATE TABLE IF NOT EXISTS people (item INTEGER NOT NULL, role TEXT NOT NULL, name TEXT, address TEXT);
             CREATE INDEX IF NOT EXISTS people_item ON people (item);
             -- What an item means, as the model named saw it (as the clipboard's).
             CREATE TABLE IF NOT EXISTS vectors (item INTEGER PRIMARY KEY, model TEXT NOT NULL, vector BLOB NOT NULL);",
        )?;
        db.execute_batch(text::SCHEMA)?;
        Ok(Items {
            db: Mutex::new(db),
            vectors: Mutex::default(),
            stale: AtomicBool::new(false),
            queries: Queries::default(),
            meaning: OnceLock::new(),
            work: Mutex::new(false),
            work_signal: Condvar::new(),
        })
    }

    /// Adds the items of an account, or replaces the ones it has already,
    /// in one go. An item with a time that is not ISO 8601 stops it, and
    /// none are put.
    pub(crate) fn put(&self, account: &str, items: &[Item]) -> rusqlite::Result<()> {
        let mut db = self.db.lock().unwrap();
        let transaction = db.transaction()?;
        let updated = Utc::now().timestamp_millis();
        let mut changed = false;
        for item in items {
            let at = instant(&item.time).ok_or_else(|| invalid(format!("item {}: {:?} is not a time", item.id, item.time)))?;
            let until = match &item.end {
                Some(end) => Some(instant(end).ok_or_else(|| invalid(format!("item {}: {end:?} is not a time", item.id)))?),
                None => None,
            };
            let text = item.text.as_deref().map(|text| cut(text, MAX_TEXT));
            let before: Option<(i64, String, Option<String>)> = transaction
                .query_row(&format!("SELECT rowid, title, substr(text, 1, {READ}) FROM items WHERE account = ?1 AND item_id = ?2"), params![account, item.id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .optional()?;
            let rowid: i64 = transaction.query_row(
                "INSERT INTO items (account, item_id, kind, title, people_json, time, \"end\", text, url, location, attachments_json, folder, flags, updated, at, until)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                 ON CONFLICT (account, item_id) DO UPDATE SET
                     kind = excluded.kind, title = excluded.title, people_json = excluded.people_json, time = excluded.time,
                     \"end\" = excluded.\"end\", text = excluded.text, url = excluded.url, location = excluded.location,
                     attachments_json = excluded.attachments_json, folder = excluded.folder, flags = excluded.flags,
                     updated = excluded.updated, at = excluded.at, until = excluded.until
                 RETURNING rowid",
                params![
                    account,
                    item.id,
                    item.kind.name(),
                    item.title,
                    json(&item.people),
                    item.time,
                    item.end,
                    text,
                    item.url,
                    item.location,
                    json(&item.attachments),
                    item.folder,
                    json(&item.flags),
                    updated,
                    at,
                    until
                ],
                |row| row.get(0),
            )?;
            if let Some((_, title, read)) = &before {
                text::remove(&transaction, rowid)?;
                transaction.execute("DELETE FROM people WHERE item = ?1", [rowid])?;
                // What it means may have changed with its words: it is read again.
                if *title != item.title || read.as_deref() != text.map(|text| cut(text, READ)) {
                    transaction.execute("DELETE FROM vectors WHERE item = ?1", [rowid])?;
                    changed = true;
                }
            }
            text::add(&transaction, rowid, &searched(item, text))?;
            for person in item.people.iter().flatten() {
                transaction.execute(
                    "INSERT INTO people (item, role, name, address) VALUES (?1, ?2, ?3, ?4)",
                    params![rowid, person.role.name(), person.name.as_deref().map(str::to_lowercase), person.address.as_deref().map(str::to_lowercase)],
                )?;
            }
        }
        transaction.commit()?;
        if changed {
            self.stale.store(true, Ordering::Relaxed);
        }
        drop(db);
        self.wake();
        Ok(())
    }

    /// Deletes the items the condition selects, with all that was made of
    /// them. Returns how many.
    fn delete_where(&self, condition: &str, values: Vec<Value>) -> rusqlite::Result<usize> {
        let mut db = self.db.lock().unwrap();
        let transaction = db.transaction()?;
        let doomed: Vec<i64> = {
            let mut statement = transaction.prepare(&format!("SELECT rowid FROM items WHERE {condition}"))?;
            let rows = statement.query_map(params_from_iter(values), |row| row.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for rowid in &doomed {
            transaction.execute("DELETE FROM items WHERE rowid = ?1", [rowid])?;
            text::remove(&transaction, *rowid)?;
            transaction.execute("DELETE FROM people WHERE item = ?1", [rowid])?;
            transaction.execute("DELETE FROM vectors WHERE item = ?1", [rowid])?;
        }
        transaction.commit()?;
        if !doomed.is_empty() {
            self.stale.store(true, Ordering::Relaxed);
        }
        Ok(doomed.len())
    }

    /// Deletes the items of an account with these ids.
    pub(crate) fn remove(&self, account: &str, ids: &[String]) -> rusqlite::Result<()> {
        for ids in ids.chunks(500) {
            let marks = vec!["?"; ids.len()].join(", ");
            let values = std::iter::once(account).chain(ids.iter().map(String::as_str)).map(|text| Value::Text(text.to_owned())).collect();
            self.delete_where(&format!("account = ? AND item_id IN ({marks})"), values)?;
        }
        Ok(())
    }

    /// Deletes everything of an account.
    pub(crate) fn drop_account(&self, account: &str) -> rusqlite::Result<()> {
        self.delete_where("account = ?", vec![Value::Text(account.to_owned())]).map(|_| ())
    }

    /// Deletes the items of an account that were last written before
    /// `since`: what a sync that brings everything did not bring. Returns
    /// how many.
    pub(crate) fn drop_unwritten(&self, account: &str, since: DateTime<Utc>) -> rusqlite::Result<usize> {
        self.delete_where("account = ? AND updated < ?", vec![Value::Text(account.to_owned()), Value::Integer(since.timestamp_millis())])
    }

    /// Deletes the items of an account that were over (or sent) before
    /// `older_than`. Returns how many.
    pub(crate) fn prune(&self, account: &str, older_than: DateTime<Utc>) -> rusqlite::Result<usize> {
        self.delete_where("account = ? AND coalesce(until, at) < ?", vec![Value::Text(account.to_owned()), Value::Integer(older_than.timestamp_millis())])
    }

    pub(crate) fn count(&self, account: &str) -> rusqlite::Result<usize> {
        self.db.lock().unwrap().query_row("SELECT count(*) FROM items WHERE account = ?1", [account], |row| row.get::<_, i64>(0)).map(|n| n as usize)
    }

    /// How a query is read at `now`.
    pub(crate) fn reading(&self, text: &str, now: DateTime<Local>) -> Query {
        let db = self.db.lock().unwrap();
        query::parse(text, now, &|name| known(&db, name))
    }

    /// Finds items, the best first. A time, person or kind read out of the
    /// query that matches nothing is taken back, and the query searched for
    /// as words. With finding by meaning on and the query's vector made,
    /// what it means counts too; the vector is asked for otherwise, and a
    /// search a moment later has it (`search_waiting`).
    pub(crate) fn search(&self, query: &str, limit: usize, now: DateTime<Local>) -> rusqlite::Result<Vec<Found>> {
        let vector = match self.query_vector(query, now) {
            Some(Vector::Ready(vector)) => Some(vector),
            _ => None,
        };
        self.search_with(query, limit, now, vector.as_deref())
    }

    /// `search`, waiting up to `patience` for the query's vector to be made.
    pub(crate) fn search_waiting(&self, query: &str, limit: usize, now: DateTime<Local>, patience: Duration) -> rusqlite::Result<Vec<Found>> {
        let asked = Instant::now();
        loop {
            match self.query_vector(query, now) {
                Some(Vector::Ready(vector)) => return self.search_with(query, limit, now, Some(&vector)),
                Some(Vector::Coming) if asked.elapsed() < patience => std::thread::sleep(Duration::from_millis(40)),
                _ => return self.search_with(query, limit, now, None),
            }
        }
    }

    /// The vector of what of a query is put to the model, when finding by
    /// meaning is on and there is anything to put.
    fn query_vector(&self, query: &str, now: DateTime<Local>) -> Option<Vector> {
        self.meaning.get().filter(|meaning| (meaning.enabled)())?;
        let asked = fusion::asked(&self.reading(query, now).terms, &Tuning::default())?;
        Some(self.queries.vector(&asked))
    }

    /// The search, with `vector` the model's vector for what of the query
    /// is put to it (`fusion::asked`), when there is one.
    fn search_with(&self, text: &str, limit: usize, now: DateTime<Local>, vector: Option<&[f32]>) -> rusqlite::Result<Vec<Found>> {
        let tuning = Tuning::default();
        let db = self.db.lock().unwrap();
        let at = now.timestamp_millis();
        let mut query = query::parse(text, now, &|name| known(&db, name));
        let mut vectors = self.vectors.lock().unwrap();
        let mut meant = |query: &Query| -> rusqlite::Result<Vec<u64>> {
            let (Some(vector), Some(_)) = (vector, fusion::weight(&query.terms, &tuning)) else { return Ok(Vec::new()) };
            let Some(model) = self.meaning.get().map(|meaning| meaning.model.model().to_owned()) else { return Ok(Vec::new()) };
            if vectors.is_none() || self.stale.swap(false, Ordering::Relaxed) {
                *vectors = Some(Items::read_vectors(&db, &model)?);
            }
            let index = vectors.as_mut().unwrap();
            Ok(fusion::near(index, vector, language(&query.terms.join(" ")), passing(&db, query)?.as_ref(), &tuning))
        };
        let mut words = run(&db, &query, limit, at)?;
        let mut meaning = meant(&query)?;
        if words.is_empty() && meaning.is_empty() && query.filtered() {
            query = Query::literal(text);
            words = run(&db, &query, limit, at)?;
            meaning = meant(&query)?;
        }
        let weight = fusion::weight(&query.terms, &tuning).unwrap_or(1.0);
        let order = fusion::scored(&words.iter().map(|(rowid, _, _)| *rowid as u64).collect::<Vec<_>>(), &meaning, weight);
        let mut by_rowid: HashMap<u64, (String, Item)> = words.into_iter().map(|(rowid, account, item)| (rowid as u64, (account, item))).collect();
        let mut found = Vec::with_capacity(order.len().min(limit));
        for (rowid, score, _) in order {
            let item = match by_rowid.remove(&rowid) {
                Some(item) => Some(item),
                // One deleted since its vector was read is not there to be found.
                None => db.query_row(&format!("SELECT {COLUMNS} FROM items WHERE rowid = ?1"), [rowid as i64], row).optional()?.map(|(_, account, item)| (account, item)),
            };
            let Some((account, item)) = item else { continue };
            found.push(Found { account, item, score });
            if found.len() == limit {
                break;
            }
        }
        Ok(found)
    }

    fn read_vectors(db: &Connection, model: &str) -> rusqlite::Result<Index> {
        let mut index = Index::default();
        let mut statement = db.prepare(&format!(
            "SELECT item, vector, title || char(10) || substr(coalesce(text, ''), 1, {READ}) FROM vectors JOIN items ON items.rowid = vectors.item
             WHERE model = ?1 AND length(vector) > 0"
        ))?;
        let mut rows = statement.query([model])?;
        while let Some(row) = rows.next()? {
            let text: String = row.get(2)?;
            index.push(row.get::<_, i64>(0)? as u64, language(&text), &floats(row.get_ref(1)?.as_blob()?));
        }
        Ok(index)
    }

    /// How many items `model` has made vectors of, and how many it has yet
    /// to look at.
    pub(crate) fn vector_counts(&self, model: &str) -> rusqlite::Result<(usize, usize)> {
        let db = self.db.lock().unwrap();
        let count = |sql: &str| db.query_row(sql, [model], |row| row.get::<_, i64>(0)).map(|n| n as usize);
        Ok((
            count("SELECT count(*) FROM vectors WHERE model = ?1 AND length(vector) > 0")?,
            count("SELECT count(*) FROM items WHERE NOT EXISTS (SELECT 1 FROM vectors WHERE item = items.rowid AND model = ?1)")?,
        ))
    }

    /// Tells the background work there may be items without vectors.
    fn wake(&self) {
        *self.work.lock().unwrap() = true;
        self.work_signal.notify_all();
    }

    /// Starts finding by meaning with `model`, while `enabled` says so: two
    /// threads, one giving items their vectors, one making the vectors of
    /// queries. Once; later calls do nothing.
    pub(crate) fn start_meaning(self: &Arc<Self>, model: Arc<dyn Embed>, enabled: impl Fn() -> bool + Send + Sync + 'static) {
        if self.meaning.set(Meaning { model, enabled: Box::new(enabled) }).is_err() {
            return;
        }
        let items = self.clone();
        std::thread::spawn(move || items.fill_vectors());
        let items = self.clone();
        std::thread::spawn(move || {
            let meaning = items.meaning.get().unwrap();
            items.queries.answer(|| (meaning.enabled)().then_some(meaning.model.as_ref()));
        });
        self.wake();
    }

    /// Gives items their vectors whenever there are some without, for as
    /// long as Heyo runs, as the clipboard's entries get theirs.
    fn fill_vectors(&self) {
        let Some(meaning) = self.meaning.get() else { return };
        let mut wait = NAP;
        loop {
            {
                let mut work = self.work.lock().unwrap();
                if !*work {
                    work = self.work_signal.wait_timeout(work, wait).unwrap().0;
                }
                *work = false;
            }
            if !(meaning.enabled)() {
                // What takes memory goes while it is off.
                *self.vectors.lock().unwrap() = None;
                self.queries.clear();
                continue;
            }
            loop {
                if !(meaning.enabled)() {
                    break;
                }
                if self.queries.typing() {
                    std::thread::sleep(Duration::from_millis(250));
                    continue;
                }
                let started = Instant::now();
                let done = meaning::step(self, meaning.model.as_ref(), BATCH, |id, language, vector| {
                    if let Some(index) = &mut *self.vectors.lock().unwrap() {
                        index.push(id, language, vector);
                    }
                });
                match done {
                    Ok(0) => {
                        wait = NAP;
                        break;
                    }
                    // As long again doing nothing: a mailbox being read is
                    // not to be what the computer is busy with.
                    Ok(_) => std::thread::sleep(started.elapsed().max(Duration::from_millis(20))),
                    Err(e) => {
                        eprintln!("heyo: connections: could not read items for their meaning: {e}");
                        wait = (wait * 2).min(Duration::from_secs(3600));
                        break;
                    }
                }
            }
        }
    }
}

impl Vectored for Items {
    /// The items nearest to now first: those are the ones most likely to be looked for.
    fn waiting(&self, model: &str, count: usize) -> Result<Vec<Waiting>, String> {
        let db = self.db.lock().unwrap();
        let read = || -> rusqlite::Result<Vec<Waiting>> {
            let mut statement = db.prepare_cached(&format!(
                "SELECT rowid, title, substr(text, 1, {READ}) FROM items
                 WHERE NOT EXISTS (SELECT 1 FROM vectors WHERE item = items.rowid AND model = ?1)
                 ORDER BY abs(at - ?2), rowid DESC LIMIT ?3"
            ))?;
            let rows = statement.query_map(params![model, Utc::now().timestamp_millis(), count as i64], |row| {
                let (title, text): (String, Option<String>) = (row.get(1)?, row.get(2)?);
                let read = worth(&title, text.as_deref());
                Ok(Waiting { id: row.get::<_, i64>(0)? as u64, language: language(&format!("{title}\n{}", text.as_deref().unwrap_or_default())), read })
            })?;
            rows.collect()
        };
        read().map_err(|e| e.to_string())
    }

    fn note(&self, model: &str, made: &[(u64, Option<&[f32]>)]) -> Result<Vec<bool>, String> {
        let db = self.db.lock().unwrap();
        made.iter()
            .map(|(id, vector)| {
                db.execute(
                    "INSERT OR REPLACE INTO vectors (item, model, vector) SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM items WHERE rowid = ?1)",
                    params![*id as i64, model, blob(vector.unwrap_or_default())],
                )
                .map(|noted| noted == 1)
                .map_err(|e| e.to_string())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
