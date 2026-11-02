//! What finding by meaning takes, whatever is searched: the vectors of the
//! queries being typed, made in the background, the latest query only; and
//! the vectors of what is searched, given a few at a time, the most recent
//! first. What is waiting is whatever has none yet, so it carries on where
//! it stopped. Nothing waits for either.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use super::embed::{Embed, Role};

/// How many queries' vectors are remembered: going back over what was
/// typed, or asking again, costs nothing.
const REMEMBERED: usize = 32;
/// What is searched waits for its vectors while someone is typing a query.
const TYPING: Duration = Duration::from_millis(1500);

#[derive(Default)]
struct Asked {
    /// The query to make a vector for next. A newer one replaces it: the
    /// one before is no longer what is being asked.
    next: Option<String>,
    /// The vectors made, or why one could not be, the most recent last.
    known: VecDeque<(String, Result<Vec<f32>, String>)>,
    /// When a query last came.
    at: Option<Instant>,
}

/// The vectors of the queries asked.
#[derive(Default)]
pub struct Queries {
    asked: Mutex<Asked>,
    signal: Condvar,
}

/// The vector of a query.
pub enum Vector {
    Ready(Vec<f32>),
    /// Being made; ask again.
    Coming,
    Failed(String),
}

impl Queries {
    /// Whether a query came a moment ago: someone is typing.
    pub fn typing(&self) -> bool {
        self.asked.lock().unwrap().at.is_some_and(|at| at.elapsed() < TYPING)
    }

    /// The vector of a query if it is known, and otherwise asks for it.
    pub fn vector(&self, text: &str) -> Vector {
        let mut asked = self.asked.lock().unwrap();
        asked.at = Some(Instant::now());
        if let Some(at) = asked.known.iter().position(|(known, _)| known == text) {
            return match &asked.known[at].1 {
                Ok(vector) => Vector::Ready(vector.clone()),
                // Forgotten, so that asking again tries again.
                Err(_) => Vector::Failed(asked.known.remove(at).and_then(|(_, result)| result.err()).unwrap_or_default()),
            };
        }
        asked.next = Some(text.to_owned());
        self.signal.notify_all();
        Vector::Coming
    }

    /// The next query to make a vector for, waiting for one to be asked.
    pub fn next(&self) -> String {
        let mut asked = self.asked.lock().unwrap();
        loop {
            match asked.next.take() {
                Some(text) => return text,
                None => asked = self.signal.wait(asked).unwrap(),
            }
        }
    }

    /// Notes the vector made for a query, or why there is none.
    pub fn answered(&self, text: String, result: Result<Vec<f32>, String>) {
        let mut asked = self.asked.lock().unwrap();
        asked.known.push_back((text, result));
        if asked.known.len() > REMEMBERED {
            asked.known.pop_front();
        }
    }

    /// Makes the vectors of the queries asked for with `model`, one at a
    /// time, for as long as Heyo runs. `None`: not now; the query is dropped.
    pub fn answer<'a>(&self, model: impl Fn() -> Option<&'a dyn Embed>) {
        loop {
            let text = self.next();
            let Some(model) = model() else { continue };
            let result = model.embed(Role::Query, std::slice::from_ref(&text)).and_then(|mut vectors| vectors.pop().ok_or_else(|| "no vector".to_owned()));
            self.answered(text, result);
        }
    }

    /// Forgets the vectors made: they take memory.
    pub fn clear(&self) {
        self.asked.lock().unwrap().known.clear();
    }
}

/// One of the things searched that has no vector of the model's yet.
#[derive(Debug, Clone, PartialEq)]
pub struct Waiting {
    pub id: u64,
    /// What of it is read for its meaning: nothing, for one there is nothing
    /// in to find by meaning (an address, a number).
    pub read: Option<String>,
    /// The language of its text (`vectors::language`).
    pub language: usize,
}

/// Where vectors are kept.
pub trait Vectored {
    /// At most `count` of the things searched that `model` has not looked at
    /// yet, the most recent first: those are the ones most likely to be looked for.
    fn waiting(&self, model: &str, count: usize) -> Result<Vec<Waiting>, String>;
    /// Notes what `model` made of each: its vector, or nothing. Says of each
    /// whether it was noted: one deleted in the meantime is not given a
    /// vector to outlive it.
    fn note(&self, model: &str, made: &[(u64, Option<&[f32]>)]) -> Result<Vec<bool>, String>;
}

/// Gives the next things that have none their vectors, the most recent
/// first, or notes that there is nothing in them to find by meaning.
/// `added` hears of each new vector, with the language of its text. Returns
/// how many it dealt with: none means all are done. On failure nothing is
/// noted, and the same ones are next again.
pub fn step(store: &impl Vectored, model: &dyn Embed, count: usize, mut added: impl FnMut(u64, usize, &[f32])) -> Result<usize, String> {
    let waiting = store.waiting(model.model(), count)?;
    let texts: Vec<String> = waiting.iter().filter_map(|waiting| waiting.read.clone()).collect();
    // Not while the store is locked: this is the slow part.
    let made = if texts.is_empty() { Vec::new() } else { model.embed(Role::Passage, &texts)? };
    if made.len() != texts.len() {
        return Err(format!("asked for {} vectors and was given {}", texts.len(), made.len()));
    }
    let mut vectors = made.iter();
    let made: Vec<(u64, Option<&[f32]>)> = waiting.iter().map(|waiting| (waiting.id, waiting.read.as_ref().and_then(|_| vectors.next()).map(Vec::as_slice))).collect();
    let noted = store.note(model.model(), &made)?;
    // One deleted meanwhile took no vector, and none is announced.
    for (((id, vector), waiting), noted) in made.iter().zip(&waiting).zip(noted) {
        if let (true, Some(vector)) = (noted, vector) {
            added(*id, waiting.language, vector);
        }
    }
    Ok(waiting.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_is_asked_for_once_and_then_known() {
        let queries = Queries::default();
        assert!(matches!(queries.vector("server login"), Vector::Coming));
        // A newer query replaces one not started on.
        assert!(matches!(queries.vector("server login command"), Vector::Coming));
        assert_eq!(queries.asked.lock().unwrap().next.as_deref(), Some("server login command"));
        assert!(queries.typing());
        assert_eq!(queries.next(), "server login command");
        queries.answered("server login command".into(), Ok(vec![1.0]));
        assert!(matches!(queries.vector("server login command"), Vector::Ready(v) if v == [1.0]));
        // A failure is told once; asking again tries again.
        queries.answered("发票".into(), Err("no model".into()));
        assert!(matches!(queries.vector("发票"), Vector::Failed(e) if e == "no model"));
        assert!(matches!(queries.vector("发票"), Vector::Coming));
        // At most so many are remembered, the oldest forgotten first.
        for n in 0..REMEMBERED {
            queries.answered(format!("query {n}"), Ok(vec![n as f32]));
        }
        assert!(matches!(queries.vector("server login command"), Vector::Coming));
        assert!(matches!(queries.vector("query 1"), Vector::Ready(_)));
    }
}
