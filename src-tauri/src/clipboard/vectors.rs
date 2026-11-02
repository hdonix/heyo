//! Where the vectors of the entries are kept: the `vectors` table of the
//! history. What they are and how the nearest are found is `index::vectors`.

use rusqlite::params;

use super::store::Store;
use super::Kind;
use crate::index::vectors::{blob, floats, language, Index};

/// As much of an entry as is read for its meaning, in characters. The model
/// reads less (`runtime/src/embed.ts`); this only keeps a long copy from
/// being handed over whole.
const READ: usize = 3072;

/// The text of an entry that is worth a vector, if any is. What a person
/// would only ever look for by its letters is not: an address, a path, a
/// colour, a number, a key. Those are found by the words and substrings
/// indexes, and a vector for them would only be there to be near things it
/// has nothing to do with.
pub fn worth(kind: Kind, text: &str) -> Option<&str> {
    let text = text.trim();
    if kind != Kind::Text || text.chars().count() < 3 || !text.chars().any(char::is_alphabetic) {
        return None;
    }
    // One unbroken run of letters and digits: an identifier, a token, a hash.
    let opaque = text.is_ascii() && text.len() >= 12 && !text.contains(char::is_whitespace) && text.contains(|c: char| c.is_ascii_digit());
    if opaque {
        return None;
    }
    Some(match text.char_indices().nth(READ) {
        Some((end, _)) => &text[..end],
        None => text,
    })
}

impl Store {
    /// Entries `model` has not looked at yet, the most recent first: those
    /// are the ones most likely to be looked for.
    pub fn without_vector(&self, model: &str, limit: usize) -> rusqlite::Result<Vec<(u64, Kind, String)>> {
        let mut statement = self.db.prepare_cached(
            "SELECT id, kind, substr(text, 1, 8192) FROM entries
             WHERE text IS NOT NULL AND NOT EXISTS (SELECT 1 FROM vectors WHERE entry = entries.id AND model = ?1)
             ORDER BY copied_at DESC, id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![model, limit as i64], |row| {
            Ok((row.get::<_, i64>(0)? as u64, super::store::kind_named(&row.get::<_, String>(1)?), row.get(2)?))
        })?;
        rows.collect()
    }

    /// Notes what `model` made of an entry: its vector, or nothing for an
    /// entry that has nothing to find by meaning. Says whether it was noted:
    /// an entry deleted in the meantime is not given a vector to outlive it.
    pub fn set_vector(&self, id: u64, model: &str, vector: Option<&[f32]>) -> rusqlite::Result<bool> {
        let noted = self.db.execute(
            "INSERT OR REPLACE INTO vectors (entry, model, vector) SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM entries WHERE id = ?1)",
            params![id as i64, model, blob(vector.unwrap_or_default())],
        )?;
        Ok(noted == 1)
    }

    /// The vectors `model` made. Those of another model are not read: they
    /// are replaced one by one as their entries are looked at again.
    pub fn vectors(&self, model: &str) -> rusqlite::Result<Index> {
        let mut index = Index::default();
        // As much of the text as says what language it is in.
        let mut statement = self.db.prepare(
            "SELECT entry, vector, substr(text, 1, 256) FROM vectors JOIN entries ON entries.id = vectors.entry
             WHERE model = ?1 AND length(vector) > 0",
        )?;
        let mut rows = statement.query([model])?;
        while let Some(row) = rows.next()? {
            let text: Option<String> = row.get(2)?;
            index.push(row.get::<_, i64>(0)? as u64, language(text.as_deref().unwrap_or_default()), &floats(row.get_ref(1)?.as_blob()?));
        }
        Ok(index)
    }

    /// How many entries have a vector of `model`'s, and how many it has yet
    /// to look at.
    pub fn vector_counts(&self, model: &str) -> rusqlite::Result<(usize, usize)> {
        let count = |sql: &str| self.db.query_row(sql, [model], |row| row.get::<_, i64>(0)).map(|n| n as usize);
        Ok((
            count("SELECT count(*) FROM vectors WHERE model = ?1 AND length(vector) > 0")?,
            count("SELECT count(*) FROM entries WHERE text IS NOT NULL AND NOT EXISTS (SELECT 1 FROM vectors WHERE entry = entries.id AND model = ?1)")?,
        ))
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::clipboard::Entry;
    pub(crate) use crate::index::vectors::tests::unit;

    pub fn add(store: &Store, text: &str, time: u64) -> u64 {
        let hash = crate::clipboard::hash_of(text.as_bytes());
        let mut made = 0;
        store
            .record(hash, None, time, |id| {
                made = id;
                Some(Entry {
                    id,
                    kind: crate::clipboard::classify(text),
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

    #[test]
    fn what_is_worth_a_vector() {
        assert_eq!(worth(Kind::Text, "  会议纪要：周三下午讨论 Wayland 支持 "), Some("会议纪要：周三下午讨论 Wayland 支持"));
        assert_eq!(worth(Kind::Text, "发票已寄出"), Some("发票已寄出"));
        assert_eq!(worth(Kind::Text, "git rebase --onto main"), Some("git rebase --onto main"));
        // Found by their letters, if at all.
        assert_eq!(worth(Kind::Link, "https://example.com/a-page-about-things"), None);
        assert_eq!(worth(Kind::Color, "#ff8800"), None);
        assert_eq!(worth(Kind::File, "/home/someone/report.pdf"), None);
        assert_eq!(worth(Kind::Text, "ok"), None);
        assert_eq!(worth(Kind::Text, "2026-10-10 15:00"), None);
        assert_eq!(worth(Kind::Text, "550e8400-e29b-41d4-a716-446655440000"), None);
        assert_eq!(worth(Kind::Text, "key_4eC39HqLyjWDarjtT1zdp7dc"), None);
        // A long copy is read from its beginning.
        let long = "字".repeat(READ + 10);
        assert_eq!(worth(Kind::Text, &long).unwrap().chars().count(), READ);
    }

    #[test]
    fn vectors_are_kept_with_the_history() {
        let store = Store::in_memory();
        let old = add(&store, "an older note about invoices", 1000);
        let new = add(&store, "a newer note about servers", 2000);
        let link = add(&store, "https://example.com", 3000);
        // The most recent first.
        let waiting: Vec<u64> = store.without_vector("m1", 10).unwrap().into_iter().map(|(id, _, _)| id).collect();
        assert_eq!(waiting, [link, new, old]);
        assert_eq!(store.vector_counts("m1").unwrap(), (0, 3));

        assert!(store.set_vector(new, "m1", Some(&unit(1, 8))).unwrap());
        assert!(store.set_vector(link, "m1", None).unwrap());
        assert_eq!(store.without_vector("m1", 10).unwrap().len(), 1);
        assert_eq!(store.vector_counts("m1").unwrap(), (1, 1));
        let mut index = store.vectors("m1").unwrap();
        let found = index.nearest(&unit(1, 8), 0, None, 5);
        assert_eq!(found.len(), 1);
        assert!(found[0].0 == new && (found[0].1 - 1.0).abs() < 1e-5);

        // Another model starts over, and does not see the first one's vectors.
        assert_eq!(store.without_vector("m2", 10).unwrap().len(), 3);
        assert_eq!(store.vectors("m2").unwrap().len(), 0);
        assert!(store.set_vector(new, "m2", Some(&unit(2, 4))).unwrap());
        assert_eq!(store.vectors("m1").unwrap().len(), 0);
        assert_eq!(store.vectors("m2").unwrap().len(), 1);
    }

    #[test]
    fn a_deleted_entry_leaves_no_vector() {
        let store = Store::in_memory();
        let id = add(&store, "to be forgotten entirely", 1000);
        assert!(store.set_vector(id, "m1", Some(&unit(1, 8))).unwrap());
        store.delete(id).unwrap();
        assert_eq!(store.vectors("m1").unwrap().len(), 0);
        let rows: i64 = store.db.query_row("SELECT count(*) FROM vectors", [], |row| row.get(0)).unwrap();
        assert_eq!(rows, 0);
        // One that arrives after the entry is gone is not kept either.
        assert!(!store.set_vector(id, "m1", Some(&unit(1, 8))).unwrap());
        let rows: i64 = store.db.query_row("SELECT count(*) FROM vectors", [], |row| row.get(0)).unwrap();
        assert_eq!(rows, 0);
    }
}
