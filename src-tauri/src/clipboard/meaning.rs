//! Finding clipboard entries by what they mean ("that server login command"
//! finds an `ssh` line), in Chinese and English alike. Off unless the user
//! turns it on (`Settings::clipboard_meaning`) and downloads the model.
//!
//! Two things happen in the background, and nothing waits for either
//! (`index::meaning`): entries get their vectors, and the query being typed
//! gets its own. The search by words answers at once as it always did; the
//! one by meaning is asked for separately, and says "not yet" until the
//! vector is there.
//! Whenever anything is missing or fails, the search by words is all there is.

use crate::i18n::t;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::Local;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::fusion::{self, Tuning};
use super::store::{Found, Store};
use super::vectors;
use super::{ClipboardHistory, Kind};
use crate::index::embed;
use crate::index::meaning::{Queries, Vector, Vectored, Waiting};
use crate::index::vectors::{language, Index};
use crate::helper::InstallProgress;
use crate::wire::event;
use crate::SettingsState;

/// How many entries are given their vectors in one go. Few: the history's
/// lock is taken between goes, and a query is answered between entries.
const BATCH: usize = 8;
/// While finding by meaning is on, how long the indexer sleeps when nothing
/// wakes it (a copy, the settings page, a finished download do). It is what
/// tries again after a failure, and then it grows.
const NAP: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct Meaning {
    /// There from `start` on: running the helper takes a running Heyo.
    /// The connections' items are found by meaning with the same one
    /// (`model`), so that one helper runs.
    model: OnceLock<Arc<embed::Model>>,
    /// The vectors, in memory while the search is on.
    index: Mutex<Option<Index>>,
    /// Entries were deleted since the vectors were read.
    stale: AtomicBool,
    queries: Queries,
    /// Whether there may be something for the indexer to do.
    work: Mutex<bool>,
    work_signal: Condvar,
    installing: AtomicBool,
    /// Why entries are not getting their vectors, when they are not.
    problem: Mutex<Option<String>>,
}

/// Where finding by meaning stands, for the settings page.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ClipboardMeaningStatus")]
pub struct Status {
    pub enabled: bool,
    pub installed: bool,
    pub installing: bool,
    /// Entries that can be found by meaning.
    pub indexed: usize,
    /// Entries not looked at yet.
    pub waiting: usize,
    pub problem: Option<String>,
}

/// What the Clipboard History view is told with every answer while finding
/// by meaning is on.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Progress {
    pub indexed: usize,
    pub waiting: usize,
}

pub use crate::index::meaning::step;

impl Vectored for Mutex<Store> {
    fn waiting(&self, model: &str, count: usize) -> Result<Vec<Waiting>, String> {
        let waiting = self.lock().unwrap().without_vector(model, count).map_err(|e| e.to_string())?;
        Ok(waiting
            .into_iter()
            .map(|(id, kind, text)| Waiting { id, read: vectors::worth(kind, &text).map(str::to_owned), language: language(&text) })
            .collect())
    }

    fn note(&self, model: &str, made: &[(u64, Option<&[f32]>)]) -> Result<Vec<bool>, String> {
        let store = self.lock().unwrap();
        made.iter().map(|(id, vector)| store.set_vector(*id, model, *vector).map_err(|e| e.to_string())).collect()
    }
}

impl Meaning {
    /// Tells the indexer there may be something to do.
    pub fn wake(&self) {
        *self.work.lock().unwrap() = true;
        self.work_signal.notify_all();
    }

    /// Entries were deleted: their vectors are gone from the history, and
    /// must go from memory before the next search.
    pub fn forget(&self) {
        self.stale.store(true, Ordering::Relaxed);
    }

    /// Makes the vectors of the queries asked for, one at a time, for as
    /// long as Heyo runs.
    fn answer_queries(&self) {
        self.queries.answer(|| self.model.get().map(|model| model.as_ref() as &dyn embed::Embed));
    }

    /// Stops everything that takes memory: the model, the vectors.
    fn rest(&self) {
        if let Some(model) = self.model.get() {
            model.stop();
        }
        *self.index.lock().unwrap() = None;
        self.queries.clear();
    }
}

/// The embedding model, one for all that finds by meaning: the clipboard's
/// entries and the connections' items.
pub fn model(app: &AppHandle) -> Arc<embed::Model> {
    let helper = app.clone();
    app.state::<ClipboardHistory>().meaning.model.get_or_init(|| Arc::new(embed::Model::new(move || embed::command(&helper)))).clone()
}

/// Whether finding by meaning is on: the setting, and the model there.
/// The connections' items follow the same setting, whatever the history.
pub fn wanted(app: &AppHandle) -> bool {
    app.state::<SettingsState>().0.read().unwrap().clipboard_meaning && embed::installed()
}

fn enabled(app: &AppHandle) -> bool {
    let settings = app.state::<SettingsState>();
    let settings = settings.0.read().unwrap();
    settings.clipboard_meaning && settings.clipboard_history
}

/// Gives entries their vectors whenever there are some without, for as long
/// as Heyo runs.
fn index_entries(app: &AppHandle) {
    let history = app.state::<ClipboardHistory>();
    let meaning = &history.meaning;
    // After a failure, longer and longer before the next try.
    let mut wait = NAP;
    let mut resting = false;
    loop {
        {
            let mut work = meaning.work.lock().unwrap();
            // While it is off there is nothing to look in on: only a wake ends the wait.
            while !*work && resting {
                work = meaning.work_signal.wait(work).unwrap();
            }
            if !*work {
                work = meaning.work_signal.wait_timeout(work, wait).unwrap().0;
            }
            *work = false;
        }
        resting = !enabled(app) || !embed::installed();
        if resting {
            meaning.rest();
            continue;
        }
        let Some(model) = meaning.model.get() else { continue };
        loop {
            if !enabled(app) {
                break;
            }
            if meaning.queries.typing() {
                std::thread::sleep(Duration::from_millis(250));
                continue;
            }
            let started = Instant::now();
            let done = step(&history.store, model.as_ref(), BATCH, |id, language, vector| {
                if let Some(index) = &mut *meaning.index.lock().unwrap() {
                    index.push(id, language, vector);
                }
            });
            *meaning.problem.lock().unwrap() = done.as_ref().err().cloned();
            match done {
                Ok(0) => {
                    wait = NAP;
                    break;
                }
                // As long again doing nothing: indexing a long history is
                // not to be what the computer is busy with.
                Ok(_) => std::thread::sleep(started.elapsed().max(Duration::from_millis(20))),
                Err(e) => {
                    eprintln!("heyo: clipboard history: could not read entries for their meaning: {e}");
                    wait = (wait * 2).min(Duration::from_secs(3600));
                    break;
                }
            }
        }
    }
}

/// Starts the background work. It does nothing until the setting is on and
/// the model is there.
pub fn start(app: &AppHandle) {
    model(app);
    for work in [index_entries as fn(&AppHandle), |app| app.state::<ClipboardHistory>().meaning.answer_queries()] {
        let app = app.clone();
        std::thread::spawn(move || work(&app));
    }
    app.state::<ClipboardHistory>().meaning.wake();
}

pub fn status(app: &AppHandle) -> Status {
    let history = app.state::<ClipboardHistory>();
    let (indexed, waiting) = history.store.lock().unwrap().vector_counts(embed::MODEL).unwrap_or_default();
    // Whoever asks is looking: what failed is tried again now, not when its
    // wait is over.
    history.meaning.wake();
    let problem = history.meaning.problem.lock().unwrap().clone();
    let enabled = app.state::<SettingsState>().0.read().unwrap().clipboard_meaning;
    Status {
        enabled,
        installed: embed::installed(),
        installing: history.meaning.installing.load(Ordering::Relaxed),
        indexed,
        waiting,
        problem,
    }
}

/// How far the entries have got, when finding by meaning is on and usable.
pub fn progress(app: &AppHandle) -> Option<Progress> {
    if !enabled(app) || !embed::installed() {
        return None;
    }
    let history = app.state::<ClipboardHistory>();
    let (indexed, waiting) = history.store.lock().unwrap().vector_counts(embed::MODEL).ok()?;
    Some(Progress { indexed, waiting })
}

/// Downloads the model, reporting to the settings window as
/// `heyo://clipboard-meaning-install` events.
pub fn install(app: &AppHandle) -> Result<(), String> {
    let meaning = &app.state::<ClipboardHistory>().meaning;
    if meaning.installing.swap(true, Ordering::Relaxed) {
        return Ok(());
    }
    let _ = app.emit(event::CLIPBOARD_MEANING_INSTALL, InstallProgress::Progress { step: t("Language model").into(), received: None, total: None });
    let window = app.clone();
    let command = embed::command(app).map(|mut command| {
        command.arg("install");
        command
    });
    crate::helper::install(command, move |progress| {
        if progress.concludes() {
            let meaning = &window.state::<ClipboardHistory>().meaning;
            meaning.installing.store(false, Ordering::Relaxed);
            meaning.wake();
        }
        let _ = window.emit(event::CLIPBOARD_MEANING_INSTALL, progress);
    });
    Ok(())
}

/// What a search by meaning has to say.
pub enum Answer {
    /// What the words and the meaning found together.
    Found(Found),
    /// The query's vector is being made: ask again shortly.
    Coming,
    /// There is nothing to add to what the words found: finding by meaning
    /// is off or not ready, the query is one for letters, or it failed.
    Nothing,
}

/// Searches by words and meaning together, if the query's vector is there.
pub fn search(app: &AppHandle, text: &str, kind: Option<Kind>, limit: usize) -> Answer {
    if !enabled(app) || !embed::installed() {
        return Answer::Nothing;
    }
    let history = app.state::<ClipboardHistory>();
    let meaning = &history.meaning;
    let tuning = Tuning::default();
    let now = Local::now();
    let asked = history.store.lock().unwrap().reading(text, now).ok().and_then(|query| fusion::asked(&query, &tuning));
    let Some(asked) = asked else { return Answer::Nothing };
    let vector = match meaning.queries.vector(&asked) {
        Vector::Ready(vector) => vector,
        Vector::Coming => return Answer::Coming,
        Vector::Failed(e) => {
            eprintln!("heyo: clipboard history: could not search by meaning: {e}");
            return Answer::Nothing;
        }
    };
    // The history first, then the vectors: the order the indexer takes them in.
    let store = history.store.lock().unwrap();
    let mut index = meaning.index.lock().unwrap();
    if index.is_none() || meaning.stale.swap(false, Ordering::Relaxed) {
        *index = store.vectors(embed::MODEL).map_err(|e| eprintln!("heyo: clipboard history: could not read the vectors: {e}")).ok();
    }
    let Some(index) = &mut *index else { return Answer::Nothing };
    match store.search_by_meaning_at(text, kind, limit, now, index, &vector, &tuning) {
        Ok(found) => Answer::Found(found),
        Err(e) => {
            eprintln!("heyo: clipboard history: could not search by meaning for {text:?}: {e}");
            Answer::Nothing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::vectors::tests::{add, unit};
    use super::*;
    use crate::index::embed::{Embed, Role};
    use std::sync::atomic::AtomicUsize;

    /// Gives each text a vector along the axis of its length, and can be
    /// made to fail.
    struct Fake {
        name: &'static str,
        failing: AtomicBool,
        asked: AtomicUsize,
    }

    impl Fake {
        fn new(name: &'static str) -> Fake {
            Fake { name, failing: AtomicBool::new(false), asked: AtomicUsize::new(0) }
        }
    }

    impl Embed for Fake {
        fn model(&self) -> &str {
            self.name
        }

        fn embed(&self, _role: Role, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            if self.failing.load(Ordering::Relaxed) {
                return Err("the model fell over".into());
            }
            self.asked.fetch_add(texts.len(), Ordering::Relaxed);
            Ok(texts.iter().map(|text| unit(text.chars().count(), 16)).collect())
        }
    }

    fn history() -> (Mutex<Store>, Vec<u64>) {
        let store = Store::in_memory();
        let ids = vec![
            add(&store, "the oldest note", 1000),
            add(&store, "https://example.com/not-for-meaning", 2000),
            add(&store, "a note in the middle of them", 3000),
            add(&store, "会议纪要：周三下午讨论", 4000),
            add(&store, "the newest note of all there are", 5000),
        ];
        (Mutex::new(store), ids)
    }

    #[test]
    fn entries_get_their_vectors_a_few_at_a_time_the_newest_first() {
        let (store, ids) = history();
        let model = Fake::new("m");
        let mut heard = Vec::new();
        assert_eq!(step(&store, &model, 2, |id, _, _| heard.push(id)), Ok(2));
        assert_eq!(heard, [ids[4], ids[3]]);
        assert_eq!(store.lock().unwrap().vector_counts("m").unwrap(), (2, 3));
        assert_eq!(step(&store, &model, 2, |id, _, _| heard.push(id)), Ok(2));
        // The address was looked at and has no vector; it is not looked at again.
        assert_eq!(heard, [ids[4], ids[3], ids[2]]);
        assert_eq!(step(&store, &model, 2, |id, _, _| heard.push(id)), Ok(1));
        assert_eq!(step(&store, &model, 2, |id, _, _| heard.push(id)), Ok(0));
        assert_eq!(heard, [ids[4], ids[3], ids[2], ids[0]]);
        assert_eq!(store.lock().unwrap().vector_counts("m").unwrap(), (4, 0));
        assert_eq!(model.asked.load(Ordering::Relaxed), 4);
        // The vectors are the model's.
        let mut index = store.lock().unwrap().vectors("m").unwrap();
        assert_eq!(index.nearest(&unit("the oldest note".len(), 16), 0, None, 1)[0].0, ids[0]);
    }

    #[test]
    fn it_carries_on_where_it_stopped() {
        let (store, ids) = history();
        let model = Fake::new("m");
        assert_eq!(step(&store, &model, 2, |_, _, _| {}), Ok(2));
        // The model fails: nothing is noted, nothing is lost.
        model.failing.store(true, Ordering::Relaxed);
        assert_eq!(step(&store, &model, 2, |_, _, _| {}), Err("the model fell over".into()));
        assert_eq!(store.lock().unwrap().vector_counts("m").unwrap(), (2, 3));
        // Heyo is started again: another model object, the same history.
        let again = Fake::new("m");
        let mut heard = Vec::new();
        while step(&store, &again, 2, |id, _, _| heard.push(id)).unwrap() > 0 {}
        assert_eq!(heard, [ids[2], ids[0]]);
        assert_eq!(again.asked.load(Ordering::Relaxed), 2);
        // A copy made later is the only one waiting then.
        let late = add(&store.lock().unwrap(), "copied after the rest were done", 6000);
        let mut heard = Vec::new();
        assert_eq!(step(&store, &again, 8, |id, _, _| heard.push(id)), Ok(1));
        assert_eq!(heard, [late]);
    }

    #[test]
    fn another_model_starts_over() {
        let (store, _) = history();
        while step(&store, &Fake::new("m"), 8, |_, _, _| {}).unwrap() > 0 {}
        assert_eq!(store.lock().unwrap().vector_counts("m").unwrap(), (4, 0));
        let newer = Fake::new("m/2");
        assert_eq!(store.lock().unwrap().vector_counts("m/2").unwrap(), (0, 5));
        // Halfway, the entries done are found by the new model's vectors and
        // the rest not at all: never by the old model's, which mean something else.
        assert_eq!(step(&store, &newer, 3, |_, _, _| {}), Ok(3));
        assert_eq!(store.lock().unwrap().vectors("m/2").unwrap().len(), 3);
        assert_eq!(store.lock().unwrap().vectors("m").unwrap().len(), 1);
        while step(&store, &newer, 3, |_, _, _| {}).unwrap() > 0 {}
        assert_eq!(store.lock().unwrap().vectors("m").unwrap().len(), 0);
        assert_eq!(store.lock().unwrap().vector_counts("m/2").unwrap(), (4, 0));
    }

    #[test]
    fn an_entry_deleted_while_it_was_read_gets_no_vector() {
        struct Deleting<'a>(&'a Mutex<Store>, u64);
        impl Embed for Deleting<'_> {
            fn model(&self) -> &str {
                "m"
            }
            fn embed(&self, _role: Role, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
                self.0.lock().unwrap().delete(self.1).unwrap();
                Ok(texts.iter().map(|_| unit(1, 16)).collect())
            }
        }
        let (store, ids) = history();
        let mut heard = Vec::new();
        assert_eq!(step(&store, &Deleting(&store, ids[4]), 2, |id, _, _| heard.push(id)), Ok(2));
        assert_eq!(heard, [ids[3]]);
        assert_eq!(store.lock().unwrap().vectors("m").unwrap().len(), 1);
    }
}
