//! Text as the two full-text indexes take it, and a query's words as they
//! are looked up in them.
//!
//! `words` finds words and, for Chinese, any run of characters (see
//! `words`). `grams` finds any substring of three characters or more, which
//! is what a piece of a URL, a path or an identifier needs.

use rusqlite::types::Value;
use rusqlite::{params, Connection};

pub(crate) fn cjk(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff | 0x20000..=0x2a6df)
}

/// Text as it is indexed for word search: every Chinese, Japanese or Korean
/// character a word of its own (those languages do not put spaces between
/// words, and SQLite's tokenizer would take a whole sentence for one), the
/// rest as it is.
pub fn words(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 2);
    for c in text.chars() {
        if cjk(c) {
            out.push(' ');
            out.push(c);
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// How one term is looked up.
#[derive(Debug, PartialEq)]
pub enum Lookup {
    /// A phrase in the word index (the last word may be only begun).
    Words(String),
    /// A substring, through the trigram index: for URLs, paths, identifiers.
    Substring(String),
    /// Too short and too odd for either index (`=>`): a plain scan.
    Scan(String),
}

pub fn lookup(term: &str) -> Lookup {
    let has_word = |s: &str| s.chars().any(char::is_alphanumeric);
    if term.chars().any(cjk) {
        // A run of characters is a phrase of one-character words, which finds
        // any substring of any length; a trigram index finds nothing shorter
        // than three, and most Chinese words are two.
        Lookup::Words(format!("\"{}\" *", words(term).replace('"', " ")))
    } else if term.chars().count() >= 3 {
        Lookup::Substring(format!("\"{}\"", term.replace('"', "\"\"")))
    } else if has_word(term) {
        Lookup::Words(format!("\"{}\" *", term.replace('"', " ")))
    } else {
        Lookup::Scan(term.to_owned())
    }
}

/// The two indexes, for a table whose rows are numbered by `rowid`.
pub const SCHEMA: &str = "CREATE VIRTUAL TABLE IF NOT EXISTS words USING fts5 (tokens, content = '', contentless_delete = 1, tokenize = 'unicode61');
     CREATE VIRTUAL TABLE IF NOT EXISTS grams USING fts5 (text, content = '', contentless_delete = 1, tokenize = 'trigram');";

/// Indexes the text of row `id`.
pub fn add(db: &Connection, id: i64, text: &str) -> rusqlite::Result<()> {
    db.execute("INSERT INTO words (rowid, tokens) VALUES (?1, ?2)", params![id, words(text)])?;
    db.execute("INSERT INTO grams (rowid, text) VALUES (?1, ?2)", params![id, text])?;
    Ok(())
}

/// Takes row `id` out of both indexes.
pub fn remove(db: &Connection, id: i64) -> rusqlite::Result<()> {
    for table in ["words", "grams"] {
        db.execute(&format!("DELETE FROM {table} WHERE rowid = ?1"), [id])?;
    }
    Ok(())
}

/// The conditions under which a row, numbered by `id`, has every one of
/// `terms`; `text` is its text, for what neither index can find.
pub fn matching(terms: &[String], id: &str, text: &str, conditions: &mut Vec<String>, values: &mut Vec<Value>) {
    let (mut words, mut grams) = (Vec::new(), Vec::new());
    for term in terms {
        match lookup(term) {
            Lookup::Words(phrase) => words.push(phrase),
            Lookup::Substring(phrase) => grams.push(phrase),
            Lookup::Scan(term) => {
                conditions.push(format!("instr({text}, ?) > 0"));
                values.push(Value::Text(term));
            }
        }
    }
    for (table, phrases) in [("words", words), ("grams", grams)] {
        if !phrases.is_empty() {
            conditions.push(format!("{id} IN (SELECT rowid FROM {table} WHERE {table} MATCH ?)"));
            values.push(Value::Text(phrases.join(" AND ")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups() {
        assert_eq!(words("发票abc 2024"), " 发  票 abc 2024");
        assert_eq!(lookup("发票"), Lookup::Words("\" 发  票 \" *".into()));
        assert_eq!(lookup("github.com/fla"), Lookup::Substring("\"github.com/fla\"".into()));
        assert_eq!(lookup("ab"), Lookup::Words("\"ab\" *".into()));
        assert_eq!(lookup("=>"), Lookup::Scan("=>".into()));
        assert_eq!(lookup("say\"hi"), Lookup::Substring("\"say\"\"hi\"".into()));
    }
}
