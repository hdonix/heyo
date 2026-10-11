//! Where the clipboard history is kept: one SQLite file, with two full-text
//! indexes over the text of the entries.
//!
//! `words` finds words and, for Chinese, any run of characters (see
//! `query::words`). `grams` finds any substring of three characters or more,
//! which is what a piece of a URL, a path or an identifier needs.
//!
//! `vectors` holds what finding by meaning goes by (`vectors.rs`).

use std::path::Path;

use chrono::Local;
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use serde::Serialize;

use super::query::{self, Lookup, Query};
use super::{Entry, Kind};

/// Unpinned entries beyond this many are dropped, oldest first. How long
/// entries are kept is what bounds the history; this only bounds the file.
const MAX_ENTRIES: i64 = 50_000;

const COLUMNS: &str = "id, kind, text, image, width, height, app, copied_at, first_copied_at, times_copied, pinned, hash";

pub struct Store {
    pub(super) db: Connection,
}

/// What a search found, and how the query was read.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub entries: Vec<Entry>,
    pub query: Query,
    /// The time, source or kind read out of the query matched nothing, and
    /// the query was taken as plain words instead.
    pub relaxed: bool,
    /// The entries that are here for what they mean alone: the words of the
    /// query are not in them.
    #[serde(skip)]
    pub by_meaning: Vec<u64>,
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Text => "text",
        Kind::Link => "link",
        Kind::Color => "color",
        Kind::File => "file",
        Kind::Image => "image",
    }
}

pub(super) fn kind_named(name: &str) -> Kind {
    match name {
        "link" => Kind::Link,
        "color" => Kind::Color,
        "file" => Kind::File,
        "image" => Kind::Image,
        _ => Kind::Text,
    }
}

fn entry(row: &Row) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get::<_, i64>(0)? as u64,
        kind: kind_named(&row.get::<_, String>(1)?),
        text: row.get(2)?,
        image: row.get(3)?,
        width: row.get(4)?,
        height: row.get(5)?,
        app: row.get(6)?,
        copied_at: row.get::<_, i64>(7)? as u64,
        first_copied_at: row.get::<_, i64>(8)? as u64,
        times_copied: row.get(9)?,
        pinned: row.get(10)?,
        hash: row.get::<_, i64>(11)? as u64,
    })
}

impl Store {
    /// Opens the history in `dir`, creating it and taking over the entries
    /// of the JSON file that came before.
    pub fn open(dir: &Path) -> rusqlite::Result<Store> {
        let _ = std::fs::create_dir_all(dir);
        super::private(dir, 0o700);
        let path = dir.join("history.db");
        let store = Store::with(Connection::open(&path)?)?;
        // The history holds whatever was copied: the user's alone.
        for suffix in ["", "-wal", "-shm"] {
            super::private(&dir.join(format!("history.db{suffix}")), 0o600);
        }
        Ok(store)
    }

    /// A history that is forgotten when Heyo exits, for when the file cannot be opened.
    pub fn in_memory() -> Store {
        Store::with(Connection::open_in_memory().expect("an in-memory database")).expect("the schema")
    }

    fn with(db: Connection) -> rusqlite::Result<Store> {
        // Deleted means gone: overwrite what is deleted rather than leave it in free pages.
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA secure_delete = ON;
             CREATE TABLE IF NOT EXISTS entries (
                 id INTEGER PRIMARY KEY,
                 kind TEXT NOT NULL,
                 text TEXT,
                 image TEXT,
                 width INTEGER,
                 height INTEGER,
                 app TEXT,
                 copied_at INTEGER NOT NULL,
                 first_copied_at INTEGER NOT NULL,
                 times_copied INTEGER NOT NULL,
                 pinned INTEGER NOT NULL,
                 hash INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS entries_copied_at ON entries (copied_at);
             CREATE INDEX IF NOT EXISTS entries_hash ON entries (hash);
             CREATE VIRTUAL TABLE IF NOT EXISTS words USING fts5 (tokens, content = '', contentless_delete = 1, tokenize = 'unicode61');
             CREATE VIRTUAL TABLE IF NOT EXISTS grams USING fts5 (text, content = '', contentless_delete = 1, tokenize = 'trigram');
             -- Which entry was chosen for which search: what search quality is measured by.
             CREATE TABLE IF NOT EXISTS choices (at INTEGER NOT NULL, query TEXT NOT NULL, entry INTEGER NOT NULL, rank INTEGER NOT NULL);
             -- What an entry means, as the model named saw it: 32-bit floats, or nothing
             -- for an entry that was looked at and has nothing to find by meaning.
             CREATE TABLE IF NOT EXISTS vectors (entry INTEGER PRIMARY KEY, model TEXT NOT NULL, vector BLOB NOT NULL);",
        )?;
        Ok(Store { db })
    }

    pub(super) fn insert(&self, entry: &Entry) -> rusqlite::Result<()> {
        self.db.execute(
            &format!("INSERT OR REPLACE INTO entries ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"),
            params![
                entry.id as i64,
                kind_name(entry.kind),
                entry.text,
                entry.image,
                entry.width,
                entry.height,
                entry.app,
                entry.copied_at as i64,
                entry.first_copied_at as i64,
                entry.times_copied,
                entry.pinned,
                entry.hash as i64
            ],
        )?;
        if let Some(text) = &entry.text {
            self.db.execute("INSERT INTO words (rowid, tokens) VALUES (?1, ?2)", params![entry.id as i64, query::words(text)])?;
            self.db.execute("INSERT INTO grams (rowid, text) VALUES (?1, ?2)", params![entry.id as i64, text])?;
        }
        Ok(())
    }

    /// Adds what was copied, or, if it is in the history already, notes that
    /// it was copied again. `make` builds the entry from its id.
    pub fn record(&self, hash: u64, app: Option<String>, time: u64, make: impl FnOnce(u64) -> Option<Entry>) -> rusqlite::Result<()> {
        let existing: Option<i64> = self.db.query_row("SELECT id FROM entries WHERE hash = ?1", [hash as i64], |row| row.get(0)).optional()?;
        if let Some(id) = existing {
            self.db.execute(
                "UPDATE entries SET copied_at = ?2, times_copied = times_copied + 1, app = coalesce(?3, app) WHERE id = ?1",
                params![id, time as i64, app],
            )?;
            return Ok(());
        }
        let id: i64 = self.db.query_row("SELECT coalesce(max(id), 0) + 1 FROM entries", [], |row| row.get(0))?;
        let Some(mut entry) = make(id as u64) else { return Ok(()) };
        entry.app = app;
        self.insert(&entry)
    }

    fn select(&self, conditions: &[String], values: Vec<Value>, limit: usize) -> rusqlite::Result<Vec<Entry>> {
        let filter = if conditions.is_empty() { String::new() } else { format!("WHERE {}", conditions.join(" AND ")) };
        // The newest first: with one's own things, what is wanted is usually recent.
        let sql = format!("SELECT {COLUMNS} FROM entries {filter} ORDER BY copied_at DESC, id DESC LIMIT {limit}");
        let mut statement = self.db.prepare_cached(&sql)?;
        let rows = statement.query_map(params_from_iter(values), entry)?;
        rows.collect()
    }

    /// The most recent entries, and all pinned ones.
    pub fn recent(&self, kind: Option<Kind>, limit: usize) -> rusqlite::Result<Vec<Entry>> {
        let (conditions, values) = match kind {
            Some(kind) => (vec!["kind = ?".to_owned()], vec![Value::Text(kind_name(kind).into())]),
            None => (Vec::new(), Vec::new()),
        };
        let mut entries = self.select(&conditions, values.clone(), limit)?;
        let mut pinned = conditions;
        pinned.push("pinned".into());
        for entry in self.select(&pinned, values, limit)? {
            if !entries.iter().any(|e| e.id == entry.id) {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    /// The conditions an entry must meet whatever its words: the time,
    /// source and kind read out of the query, and the kind picked in the list.
    fn filters(query: &Query, kind: Option<Kind>) -> (Vec<String>, Vec<Value>) {
        let mut conditions = Vec::new();
        let mut values = Vec::new();
        if let Some(time) = &query.time {
            // Copied at some point in the span: it may have been copied since, too.
            conditions.push("first_copied_at < ? AND copied_at >= ?".to_owned());
            values.extend([Value::Integer(time.end), Value::Integer(time.start)]);
        }
        if let Some(app) = &query.app {
            conditions.push("instr(lower(app), ?) > 0".to_owned());
            values.push(Value::Text(app.to_lowercase()));
        }
        // The kind picked in the list, and the kind asked for in words.
        for kind in [kind, query.kind].into_iter().flatten() {
            conditions.push("kind = ?".to_owned());
            values.push(Value::Text(kind_name(kind).into()));
        }
        (conditions, values)
    }

    /// The entries that pass the query's time, source and kind, whatever
    /// their words; `None` when the query asks for none of those, and every
    /// entry passes.
    pub(super) fn passing(&self, query: &Query, kind: Option<Kind>) -> rusqlite::Result<Option<std::collections::HashSet<u64>>> {
        let (conditions, values) = Store::filters(query, kind);
        if conditions.is_empty() {
            return Ok(None);
        }
        let mut statement = self.db.prepare_cached(&format!("SELECT id FROM entries WHERE {}", conditions.join(" AND ")))?;
        let rows = statement.query_map(params_from_iter(values), |row| row.get::<_, i64>(0).map(|id| id as u64))?;
        rows.collect::<rusqlite::Result<_>>().map(Some)
    }

    pub(super) fn run(&self, query: &Query, kind: Option<Kind>, limit: usize) -> rusqlite::Result<Vec<Entry>> {
        let (mut conditions, mut values) = Store::filters(query, kind);
        let (mut words, mut grams) = (Vec::new(), Vec::new());
        for term in &query.terms {
            match query::lookup(term) {
                Lookup::Words(phrase) => words.push(phrase),
                Lookup::Substring(phrase) => grams.push(phrase),
                Lookup::Scan(text) => {
                    conditions.push("instr(text, ?) > 0".to_owned());
                    values.push(Value::Text(text));
                }
            }
        }
        for (table, phrases) in [("words", words), ("grams", grams)] {
            if !phrases.is_empty() {
                conditions.push(format!("id IN (SELECT rowid FROM {table} WHERE {table} MATCH ?)"));
                values.push(Value::Text(phrases.join(" AND ")));
            }
        }
        self.select(&conditions, values, limit)
    }

    /// Finds entries. A time, source or kind read out of the query that
    /// matches nothing is taken back, and the query searched for as words.
    pub fn search(&self, text: &str, kind: Option<Kind>, limit: usize) -> rusqlite::Result<Found> {
        self.search_at(text, kind, limit, Local::now())
    }

    /// `search` as if asked at `now`, which is what "昨天" is counted from.
    pub fn search_at(&self, text: &str, kind: Option<Kind>, limit: usize, now: chrono::DateTime<Local>) -> rusqlite::Result<Found> {
        let query = query::parse(text, now, &self.apps()?);
        let entries = self.run(&query, kind, limit)?;
        if entries.is_empty() && query.filtered() {
            let literal = Query::literal(text);
            let entries = self.run(&literal, kind, limit)?;
            return Ok(Found { entries, query: literal, relaxed: true, by_meaning: Vec::new() });
        }
        Ok(Found { entries, query, relaxed: false, by_meaning: Vec::new() })
    }

    /// The applications there are copies from.
    pub fn apps(&self) -> rusqlite::Result<Vec<String>> {
        let mut statement = self.db.prepare_cached("SELECT DISTINCT app FROM entries WHERE app IS NOT NULL")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect()
    }

    pub fn get(&self, id: u64) -> Option<Entry> {
        self.db.query_row(&format!("SELECT {COLUMNS} FROM entries WHERE id = ?1"), [id as i64], entry).optional().ok().flatten()
    }

    pub fn set_pinned(&self, id: u64, pinned: bool) -> rusqlite::Result<()> {
        self.db.execute("UPDATE entries SET pinned = ?2 WHERE id = ?1", params![id as i64, pinned]).map(|_| ())
    }

    /// Deletes the entries the condition selects, their index entries and
    /// their image files. Returns how many.
    fn delete_where(&self, condition: &str, values: Vec<Value>) -> rusqlite::Result<usize> {
        let doomed: Vec<(i64, Option<String>)> = {
            let mut statement = self.db.prepare(&format!("SELECT id, image FROM entries WHERE {condition}"))?;
            let rows = statement.query_map(params_from_iter(values), |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        if doomed.is_empty() {
            return Ok(0);
        }
        self.db.execute_batch("BEGIN")?;
        for (id, image) in &doomed {
            // The vector too: it is the text, in another form.
            for (table, key) in [("entries", "id"), ("words", "rowid"), ("grams", "rowid"), ("vectors", "entry")] {
                self.db.execute(&format!("DELETE FROM {table} WHERE {key} = ?1"), [id])?;
            }
            // What it was found by says what it was.
            self.db.execute("DELETE FROM choices WHERE entry = ?1", [id])?;
            if let Some(path) = image {
                let _ = std::fs::remove_file(path);
            }
        }
        self.db.execute_batch("COMMIT")?;
        Ok(doomed.len())
    }

    pub fn delete(&self, id: u64) -> rusqlite::Result<usize> {
        self.delete_where("id = ?", vec![Value::Integer(id as i64)])
    }

    /// Deletes all entries except pinned ones. Returns how many.
    pub fn clear(&self) -> rusqlite::Result<usize> {
        let deleted = self.delete_where("NOT pinned", Vec::new())?;
        self.db.execute_batch("DELETE FROM choices; PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(deleted)
    }

    /// Drops unpinned entries last copied before `cutoff`, and the oldest
    /// beyond the size limit.
    pub fn prune(&self, cutoff: u64) -> rusqlite::Result<usize> {
        let old = self.delete_where("NOT pinned AND copied_at < ?", vec![Value::Integer(cutoff as i64)])?;
        let excess = self.delete_where(
            "NOT pinned AND id IN (SELECT id FROM entries WHERE NOT pinned ORDER BY copied_at DESC LIMIT -1 OFFSET ?)",
            vec![Value::Integer(MAX_ENTRIES)],
        )?;
        Ok(old + excess)
    }

    /// Notes that a search led to an entry being used, at which place in the results.
    pub fn chose(&self, time: u64, query: &str, id: u64, rank: u32) -> rusqlite::Result<()> {
        self.db.execute("INSERT INTO choices (at, query, entry, rank) VALUES (?1, ?2, ?3, ?4)", params![time as i64, query, id as i64, rank]).map(|_| ())
    }

    /// The searches that led somewhere: (query, entry, when).
    pub fn choices(&self) -> rusqlite::Result<Vec<(String, u64, u64)>> {
        let mut statement = self.db.prepare("SELECT query, entry, at FROM choices ORDER BY at")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get::<_, i64>(1)? as u64, row.get::<_, i64>(2)? as u64)))?;
        rows.collect()
    }

    pub fn count(&self) -> rusqlite::Result<usize> {
        self.db.query_row("SELECT count(*) FROM entries", [], |row| row.get::<_, i64>(0)).map(|n| n as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400_000;

    fn now() -> u64 {
        Local::now().timestamp_millis() as u64
    }

    fn add(store: &Store, text: &str, app: &str, time: u64) -> u64 {
        let hash = super::super::hash_of(text.as_bytes());
        let mut made = 0;
        store
            .record(hash, Some(app.to_owned()), time, |id| {
                made = id;
                Some(Entry {
                    id,
                    kind: super::super::classify(text),
                    text: Some(text.to_owned()),
                    image: None,
                    width: None,
                    height: None,
                    app: None,
                    copied_at: time,
                    first_copied_at: time,
                    times_copied: 1,
                    pinned: false,
                    hash,
                })
            })
            .unwrap();
        made
    }

    fn texts(found: &Found) -> Vec<&str> {
        found.entries.iter().map(|e| e.text.as_deref().unwrap_or("")).collect()
    }

    fn sample() -> Store {
        let store = Store::in_memory();
        let now = now();
        add(&store, "上个月的发票已经寄出，请查收", "wechat", now - 3 * DAY);
        add(&store, "https://github.com/heyo-app/heyo/issues/42", "firefox", now - 2 * DAY);
        add(&store, "ssh deploy@10.0.3.17 -p 2222", "org.gnome.Terminal", now - DAY);
        add(&store, "会议纪要：周三下午讨论 Wayland 支持", "Code", now - 60_000);
        add(&store, "const x = a => a + 1", "Code", now - 30_000);
        store
    }

    #[test]
    fn chinese_words_of_any_length() {
        let store = sample();
        assert_eq!(texts(&store.search("发票", None, 10).unwrap()), ["上个月的发票已经寄出，请查收"]);
        assert_eq!(texts(&store.search("票", None, 10).unwrap()), ["上个月的发票已经寄出，请查收"]);
        assert_eq!(texts(&store.search("会议纪要", None, 10).unwrap()), ["会议纪要：周三下午讨论 Wayland 支持"]);
        // Each word must occur, in any order; Chinese and English together.
        assert_eq!(texts(&store.search("wayland 纪要", None, 10).unwrap()), ["会议纪要：周三下午讨论 Wayland 支持"]);
        assert!(store.search("发货", None, 10).unwrap().entries.is_empty());
    }

    #[test]
    fn pieces_of_urls_and_commands() {
        let store = sample();
        assert_eq!(texts(&store.search("heyo/issues", None, 10).unwrap()), ["https://github.com/heyo-app/heyo/issues/42"]);
        assert_eq!(texts(&store.search("0.3.17", None, 10).unwrap()), ["ssh deploy@10.0.3.17 -p 2222"]);
        assert_eq!(texts(&store.search("GITHUB", None, 10).unwrap()), ["https://github.com/heyo-app/heyo/issues/42"]);
        // Two letters: the beginning of a word. Punctuation alone: a scan.
        assert_eq!(texts(&store.search("ss", None, 10).unwrap()), ["ssh deploy@10.0.3.17 -p 2222"]);
        assert_eq!(texts(&store.search("=>", None, 10).unwrap()), ["const x = a => a + 1"]);
        // Nothing the query syntax of the index can trip over.
        for odd in ["\"", "a\"b", "NOT", "foo AND", "(", "*", "col:on", "^x", "-p"] {
            store.search(odd, None, 10).unwrap();
        }
    }

    #[test]
    fn time_source_and_kind() {
        let store = sample();
        assert_eq!(texts(&store.search("昨天", None, 10).unwrap()), ["ssh deploy@10.0.3.17 -p 2222"]);
        assert_eq!(texts(&store.search("from firefox", None, 10).unwrap()), ["https://github.com/heyo-app/heyo/issues/42"]);
        assert_eq!(texts(&store.search("code里的 纪要", None, 10).unwrap()), ["会议纪要：周三下午讨论 Wayland 支持"]);
        assert_eq!(texts(&store.search("链接", None, 10).unwrap()), ["https://github.com/heyo-app/heyo/issues/42"]);
        assert_eq!(texts(&store.search("", Some(Kind::Link), 10).unwrap()), ["https://github.com/heyo-app/heyo/issues/42"]);
        // The newest first.
        assert_eq!(store.search("", None, 10).unwrap().entries.len(), 5);
        assert_eq!(texts(&store.search("", None, 1).unwrap()), ["const x = a => a + 1"]);
    }

    #[test]
    fn a_reading_that_finds_nothing_is_taken_back() {
        let store = sample();
        add(&store, "昨天的报告写完了", "wechat", now() - 5 * DAY);
        // "昨天" reads as a time, but no "报告" was copied yesterday: the words win.
        let found = store.search("昨天的报告", None, 10).unwrap();
        assert!(found.relaxed);
        assert_eq!(texts(&found), ["昨天的报告写完了"]);
        let found = store.search("昨天", None, 10).unwrap();
        assert!(!found.relaxed);
        assert_eq!(found.query.time.as_ref().unwrap().text, "昨天");
        assert_eq!(texts(&found), ["ssh deploy@10.0.3.17 -p 2222"]);
    }

    #[test]
    fn copying_again_and_deleting() {
        let store = sample();
        let now = now();
        let id = add(&store, "to be forgotten", "Code", now - 5 * DAY);
        assert_eq!(add(&store, "to be forgotten", "firefox", now), 0);
        let entry = store.get(id).unwrap();
        assert_eq!((entry.times_copied, entry.app.as_deref(), entry.copied_at), (2, Some("firefox"), now));
        // Found by either day it was copied on.
        assert_eq!(store.search("forgotten 5天前", None, 10).unwrap().entries.len(), 1);

        store.chose(now, "forgot", id, 0).unwrap();
        store.delete(id).unwrap();
        assert!(store.search("forgotten", None, 10).unwrap().entries.is_empty());
        assert!(store.choices().unwrap().is_empty());

        let kept = add(&store, "keep me", "Code", now - 100 * DAY);
        store.set_pinned(kept, true).unwrap();
        assert_eq!(store.prune(now - 2 * DAY - 1000).unwrap(), 1);
        assert_eq!(store.count().unwrap(), 5);
        store.clear().unwrap();
        assert_eq!(texts(&store.search("", None, 10).unwrap()), ["keep me"]);
        assert!(store.search("发票", None, 10).unwrap().entries.is_empty());
    }

    #[test]
    #[ignore]
    fn speed() {
        let store = Store::in_memory();
        let now = now();
        let nouns = ["发票", "合同", "会议纪要", "服务器地址", "password reset", "invoice", "deploy script", "接口文档", "报错日志", "docker compose"];
        store.db.execute_batch("BEGIN").unwrap();
        for i in 0..50_000u64 {
            let text = format!("{} {} https://example.com/item/{i}?ref={} 第{i}条记录，内容是{}", nouns[(i % 10) as usize], i, i * 7919 % 1000, nouns[(i * 7 % 10) as usize]);
            add(&store, &text, ["firefox", "Code", "wechat"][(i % 3) as usize], now - i * 60_000);
        }
        store.db.execute_batch("COMMIT").unwrap();
        for query in ["发票", "票", "会议纪要 报错", "item/4242", "ref=77", "de", "昨天 合同", "from firefox invoice", "上周的链接", "没有这个词"] {
            let mut times: Vec<f64> = (0..50)
                .map(|_| {
                    let start = std::time::Instant::now();
                    store.search(query, None, 200).unwrap();
                    start.elapsed().as_secs_f64() * 1000.0
                })
                .collect();
            times.sort_by(f64::total_cmp);
            println!("{query:<24} p50 {:6.2} ms   p95 {:6.2} ms   ({} found)", times[25], times[47], store.search(query, None, 200).unwrap().entries.len());
        }
    }
}
