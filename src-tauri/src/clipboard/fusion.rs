//! Searching the history by words and meaning together; how the two are
//! put together is `index::fusion`.

use std::collections::HashMap;

use chrono::{DateTime, Local};

use crate::index::fusion::near;
pub use crate::index::fusion::{fuse, weight, Tuning};
use crate::index::vectors::{self, Index};

use super::query::{self, Query};
use super::store::{Found, Store};
use super::Kind;

/// What of a query is put to the model, if anything is.
pub fn asked(query: &Query, tuning: &Tuning) -> Option<String> {
    crate::index::fusion::asked(&query.terms, tuning)
}

impl Store {
    /// How a query is read at `now`.
    pub fn reading(&self, text: &str, now: DateTime<Local>) -> rusqlite::Result<Query> {
        Ok(query::parse(text, now, &self.apps()?))
    }

    /// `search_at`, with what the query means as well as its words:
    /// `vector` is the model's vector for `asked(query)`. The time, source
    /// and kind read out of the query hold for both.
    pub fn search_by_meaning_at(
        &self,
        text: &str,
        kind: Option<Kind>,
        limit: usize,
        now: DateTime<Local>,
        index: &mut Index,
        vector: &[f32],
        tuning: &Tuning,
    ) -> rusqlite::Result<Found> {
        let mut query = self.reading(text, now)?;
        let Some(weight) = weight(&query.terms, tuning) else { return self.search_at(text, kind, limit, now) };
        let language = vectors::language(&query.terms.join(" "));
        let mut words = self.run(&query, kind, limit)?;
        let mut meant = near(index, vector, language, self.passing(&query, kind)?.as_ref(), tuning);
        let relaxed = words.is_empty() && meant.is_empty() && query.filtered();
        if relaxed {
            // As without the meaning: the reading that found nothing is taken back.
            query = Query::literal(text);
            words = self.run(&query, kind, limit)?;
            meant = near(index, vector, language, self.passing(&query, kind)?.as_ref(), tuning);
        }
        let order = fuse(&words.iter().map(|entry| entry.id).collect::<Vec<_>>(), &meant, weight);
        let mut by_id: HashMap<u64, super::Entry> = words.into_iter().map(|entry| (entry.id, entry)).collect();
        let mut by_meaning = Vec::new();
        let mut entries = Vec::with_capacity(order.len().min(limit));
        for (id, meaning_alone) in order {
            // One deleted since its vector was read is not there to be found.
            let Some(entry) = by_id.remove(&id).or_else(|| self.get(id)) else { continue };
            if meaning_alone {
                by_meaning.push(id);
            }
            entries.push(entry);
            if entries.len() == limit {
                break;
            }
        }
        Ok(Found { entries, query, relaxed, by_meaning })
    }
}

#[cfg(test)]
mod tests {
    use super::super::vectors::tests::{add, unit};
    use super::*;

    /// A history where the vector of entry `n` points along axis `n`.
    fn sample() -> (Store, Index, Vec<u64>) {
        let store = Store::in_memory();
        let now = Local::now().timestamp_millis() as u64;
        let day = 86_400_000;
        let ids = vec![
            add(&store, "ssh deploy@10.0.3.17 -p 2222", now - 3 * day),
            add(&store, "the server login is in the wiki", now - 2 * day),
            add(&store, "上个月的发票已经寄出，请查收", now - day),
            add(&store, "lunch order: two noodles", now - 60_000),
        ];
        let mut index = Index::default();
        for (axis, id) in ids.iter().enumerate() {
            store.set_vector(*id, "m", Some(&unit(axis, 16))).unwrap();
            index.push(*id, 0, &unit(axis, 16));
        }
        (store, index, ids)
    }

    fn found(store: &Store, index: &mut Index, text: &str, vector: &[f32]) -> (Vec<u64>, Vec<u64>) {
        let tuning = Tuning { floor: 0.5, ..Tuning::default() };
        let found = store.search_by_meaning_at(text, None, 10, Local::now(), index, vector, &tuning).unwrap();
        (found.entries.iter().map(|entry| entry.id).collect(), found.by_meaning)
    }

    #[test]
    fn the_meaning_finds_what_the_words_do_not() {
        let (store, mut index, ids) = sample();
        // No entry has both words; the one that means it is found.
        assert_eq!(found(&store, &mut index, "server command", &unit(0, 16)), (vec![ids[0]], vec![ids[0]]));
        // The words find one entry and the meaning another: both, and which is which.
        assert_eq!(found(&store, &mut index, "server login", &unit(0, 16)), (vec![ids[1], ids[0]], vec![ids[0]]));
        // Both agree: once, and not as a guess.
        assert_eq!(found(&store, &mut index, "server login", &unit(1, 16)), (vec![ids[1]], vec![]));
        // Nothing near enough: the words alone.
        let nowhere = vec![-0.25f32; 16];
        assert_eq!(found(&store, &mut index, "server login", &nowhere), (vec![ids[1]], vec![]));
        // Letters, not meaning: the vector is not consulted.
        assert_eq!(found(&store, &mut index, "0.3.17", &unit(3, 16)), (vec![ids[0]], vec![]));
    }

    #[test]
    fn a_time_holds_for_the_meaning_too() {
        let (store, mut index, ids) = sample();
        // The invoice was copied yesterday; the server entries were not.
        assert_eq!(found(&store, &mut index, "昨天 bill sent", &unit(2, 16)), (vec![ids[2]], vec![ids[2]]));
        // Nothing of yesterday's means that: "昨天" is taken back, as it is
        // when the words find nothing.
        let tuning = Tuning { floor: 0.5, ..Tuning::default() };
        let relaxed = store.search_by_meaning_at("昨天 server command", None, 10, Local::now(), &mut index, &unit(0, 16), &tuning).unwrap();
        assert!(relaxed.relaxed && relaxed.query.time.is_none());
        assert_eq!(relaxed.entries.iter().map(|entry| entry.id).collect::<Vec<_>>(), [ids[0]]);
        // An entry deleted since the vectors were read is not found.
        store.delete(ids[0]).unwrap();
        assert_eq!(found(&store, &mut index, "server command", &unit(0, 16)).0, Vec::<u64>::new());
    }
}
