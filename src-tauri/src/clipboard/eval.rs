//! Measuring the clipboard search: `heyo --clipboard-eval [CASES] [options]`.
//!
//! Each case is a query and the entry it should find. Without a file, the
//! cases are the searches that led to an entry being used, as the history
//! recorded them. A file has one JSON object a line:
//!
//!   {"query": "发票", "expect": 412}
//!   {"query": "ref=77", "expectText": "example.com/item", "bucket": "exact", "now": 1759900000000}
//!   {"query": "the cat's birthday", "bucket": "nothing to find"}
//!
//! `expect` is an entry id or a list of ids, any of which will do;
//! `expectText` is text the right entry contains; `now` is when the query is
//! asked (for "昨天"), by default the present. A case that expects nothing
//! is a query the history has no answer to: what counts there is how much
//! is shown all the same.
//!
//! Options:
//!
//!   --corpus FILE   search these entries instead of the history: a line
//!                   {"now": "2026-10-10T15:00:00"} (when the cases are asked,
//!                   local time), then {"id": 7, "text": "…", "app": "firefox",
//!                   "ago": "3d"} a line ("20m", "5h", "3d").
//!   --meaning       measure the search by words and by meaning side by side.
//!                   Entries without a vector are given one first.
//!   --tuning K=V,…  how the two are put together (`fusion::Tuning`).
//!   --misses        list the cases not found among the first five.
//!   --export        print the history's own cases (query, entry id, time)
//!                   in the format above, to keep or to edit, and stop.
//!   --generate N    have the local model write cases for N entries of the
//!                   history into `clipboard/eval/cases.jsonl` beside it, and
//!                   stop. Only a model on this computer is asked: the cases
//!                   hold what was copied, and stay there.
//!
//! Cases from the history flatter the search a little: using an entry makes
//! it the most recent, and the most recent come first. Cases a model wrote
//! are longer and tidier than what people type; they are for comparing two
//! ways of searching, not for saying how good either is.
//!
//! The numbers say how often the right entry is among the first 1, 5 and 10
//! results, and how high (mean reciprocal rank), by kind of query: a mean
//! over all of them would hide that, say, every two-character query fails.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;

use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use serde::Deserialize;
use serde_json::json;

use super::cases::{self, CaseEntry};
use super::embed::{self, Embed, Role};
use super::fusion::{self, Tuning};
use super::store::Store;
use super::vectors::{self, Index};
use super::{query, Entry, Kind};

#[derive(Deserialize)]
#[serde(untagged)]
enum Ids {
    One(u64),
    Any(Vec<u64>),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    query: String,
    expect: Option<Ids>,
    expect_text: Option<String>,
    bucket: Option<String>,
    now: Option<i64>,
}

impl Case {
    fn has_answer(&self) -> bool {
        self.expect.is_some() || self.expect_text.is_some()
    }

    fn is_answered_by(&self, entry: &Entry) -> bool {
        let by_id = match &self.expect {
            Some(Ids::One(id)) => entry.id == *id,
            Some(Ids::Any(ids)) => ids.contains(&entry.id),
            None => false,
        };
        by_id || self.expect_text.as_ref().is_some_and(|text| entry.text.as_ref().is_some_and(|t| t.contains(text.as_str())))
    }
}

/// The kind of query, where the case does not say.
fn bucket(text: &str, parsed: &query::Query) -> &'static str {
    let chars = text.chars().count();
    if parsed.filtered() {
        "time, source or kind"
    } else if text.contains("://") || text.contains('/') || text.chars().filter(char::is_ascii_digit).count() >= 4 {
        "exact string"
    } else if !text.is_ascii() && chars <= 4 {
        "short Chinese"
    } else if text.split_whitespace().count() >= 4 || chars >= 12 {
        "long"
    } else {
        "words"
    }
}

/// The measurements of one way of searching over one kind of query.
#[derive(Default, Clone)]
struct Tally {
    cases: usize,
    within: [usize; 3],
    reciprocal: f64,
    millis: Vec<f64>,
    /// For queries with no answer: how many entries were shown.
    shown: usize,
}

impl Tally {
    fn note(&mut self, case: &Case, found: &[Entry], millis: f64) {
        self.cases += 1;
        self.millis.push(millis);
        if !case.has_answer() {
            self.shown += found.len();
            return;
        }
        let rank = found.iter().position(|entry| case.is_answered_by(entry));
        for (slot, limit) in [1, 5, 10].into_iter().enumerate() {
            self.within[slot] += usize::from(rank.is_some_and(|r| r < limit));
        }
        self.reciprocal += rank.filter(|r| *r < 10).map_or(0.0, |r| 1.0 / (r + 1) as f64);
    }

    fn share(&self, slot: usize) -> f64 {
        100.0 * self.within[slot] as f64 / self.cases.max(1) as f64
    }
}

fn percentile(millis: &[f64], fraction: f64) -> f64 {
    let mut sorted = millis.to_vec();
    sorted.sort_by(f64::total_cmp);
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * fraction).round() as usize]
}

const NOTHING: &str = "nothing to find";

fn table(title: &str, tallies: &BTreeMap<String, Tally>) {
    println!("{title}");
    println!("{:<22} {:>5} {:>7} {:>7} {:>7} {:>7} {:>8} {:>8}", "queries", "n", "R@1", "R@5", "R@10", "MRR@10", "p50 ms", "p95 ms");
    let last = |name: &str| (name == "all", name == NOTHING);
    let mut names: Vec<&String> = tallies.keys().collect();
    names.sort_by_key(|name| last(name));
    for name in names {
        let tally = &tallies[name];
        let (p50, p95) = (percentile(&tally.millis, 0.5), percentile(&tally.millis, 0.95));
        if name == NOTHING {
            println!("{:<22} {:>5} {:>31.1} entries shown a query {:>5.2} {:>8.2}", name, tally.cases, tally.shown as f64 / tally.cases as f64, p50, p95);
            continue;
        }
        println!(
            "{:<22} {:>5} {:>6.0}% {:>6.0}% {:>6.0}% {:>7.2} {:>8.2} {:>8.2}",
            name,
            tally.cases,
            tally.share(0),
            tally.share(1),
            tally.share(2),
            tally.reciprocal / tally.cases as f64,
            p50,
            p95
        );
    }
    println!();
}

/// What the command line asks for.
#[derive(Default)]
struct Options {
    cases: Option<String>,
    corpus: Option<String>,
    meaning: bool,
    misses: bool,
    export: bool,
    generate: Option<usize>,
    tuning: Tuning,
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| args.next().cloned().ok_or(format!("{arg} needs {what}"));
        match arg.as_str() {
            "--corpus" => options.corpus = Some(value("a file")?),
            "--meaning" => options.meaning = true,
            "--misses" => options.misses = true,
            "--export" => options.export = true,
            "--generate" => options.generate = Some(value("a number of entries")?.parse().map_err(|_| "--generate needs a number of entries")?),
            "--tuning" => {
                for setting in value("settings")?.split(',') {
                    let (name, number) = setting.split_once('=').ok_or(format!("{setting}: not name=value"))?;
                    let number: f32 = number.parse().map_err(|_| format!("{setting}: not a number"))?;
                    match name {
                        "short" => options.tuning.short = number,
                        "long" => options.tuning.long = number,
                        "floor" => options.tuning.floor = number,
                        "count" => options.tuning.count = number as usize,
                        _ => return Err(format!("{name}: there is no such setting")),
                    }
                }
            }
            other if other.starts_with("--") => return Err(format!("{other}: there is no such option")),
            file => options.cases = Some(file.to_owned()),
        }
    }
    Ok(options)
}

/// A corpus line: when the cases are asked, or an entry.
#[derive(Deserialize)]
#[serde(untagged)]
enum Line {
    Now { now: String },
    Entry { id: u64, text: String, app: Option<String>, ago: String },
}

/// How long ago "20m", "5h" or "3d" is, in milliseconds.
fn ago(text: &str) -> Option<i64> {
    let unit = match text.chars().last()? {
        'm' => 60_000,
        'h' => 3_600_000,
        'd' => 86_400_000,
        _ => return None,
    };
    text[..text.len() - 1].parse::<i64>().ok().map(|n| n * unit)
}

/// The entries of a corpus file as a history, and when its cases are asked.
fn corpus(path: &str) -> Result<(Store, DateTime<Local>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let store = Store::in_memory();
    let mut now = Local::now();
    for (number, line) in text.lines().enumerate().filter(|(_, line)| !line.trim().is_empty() && !line.trim_start().starts_with("//")) {
        let wrong = |e: String| format!("{path}:{}: {e}", number + 1);
        match serde_json::from_str::<Line>(line).map_err(|e| wrong(e.to_string()))? {
            Line::Now { now: at } => {
                let naive = NaiveDateTime::parse_from_str(&at, "%Y-%m-%dT%H:%M:%S").map_err(|e| wrong(e.to_string()))?;
                now = Local.from_local_datetime(&naive).earliest().ok_or_else(|| wrong("no such local time".into()))?;
            }
            Line::Entry { id, text, app, ago: age } => {
                let time = (now.timestamp_millis() - ago(&age).ok_or_else(|| wrong(format!("{age}: not a time ago")))?) as u64;
                let entry = Entry {
                    id,
                    kind: super::classify(&text),
                    hash: super::hash_of(text.as_bytes()),
                    text: Some(text),
                    image: None,
                    width: None,
                    height: None,
                    app,
                    copied_at: time,
                    first_copied_at: time,
                    times_copied: 1,
                    pinned: false,
                };
                store.insert(&entry).map_err(|e| wrong(e.to_string()))?;
            }
        }
    }
    Ok((store, now))
}

fn cases_of(store: &Store, file: Option<&str>) -> Result<Vec<Case>, String> {
    let Some(path) = file else {
        let chosen = store.choices().map_err(|e| e.to_string())?;
        return Ok(chosen.into_iter().map(|(query, entry, at)| Case { query, expect: Some(Ids::One(entry)), expect_text: None, bucket: None, now: Some(at as i64) }).collect());
    };
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    Ok(text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with("//"))
        .filter_map(|line| serde_json::from_str(line).map_err(|e| eprintln!("heyo: skipped a line of {path}: {e}")).ok())
        .collect())
}

/// The history's own cases, as lines of a case file. No text of an entry is
/// in them: the entry is named by its id.
fn export(store: &Store) -> i32 {
    for (query, entry, at) in store.choices().unwrap_or_default() {
        println!("{}", json!({ "query": query, "expect": entry, "now": at }));
    }
    0
}

/// How a person would say when something was copied.
fn said(age_ms: i64) -> String {
    match age_ms / 86_400_000 {
        0 => "today".into(),
        1 => "yesterday".into(),
        days @ 2..=6 => format!("{days} days ago"),
        7..=13 => "last week".into(),
        days => format!("{} weeks ago", days / 7),
    }
}

/// Entries to write cases for: as many of each sort as there are sorts to
/// go round — Chinese and not, short and long, recent and old — rather than
/// whatever the history has most of.
fn sample(entries: Vec<Entry>, count: usize, now: i64) -> Vec<Entry> {
    let mut sorts: BTreeMap<(bool, usize, usize), Vec<Entry>> = BTreeMap::new();
    for entry in entries {
        let Some(text) = entry.text.as_deref().and_then(|text| vectors::worth(entry.kind, text)) else { continue };
        let chinese = text.chars().any(|c| matches!(c as u32, 0x4e00..=0x9fff));
        let length = match text.chars().count() {
            ..40 => 0,
            40..200 => 1,
            _ => 2,
        };
        let age = match (now - entry.copied_at as i64) / 86_400_000 {
            ..7 => 0,
            7..30 => 1,
            _ => 2,
        };
        sorts.entry((chinese, length, age)).or_default().push(entry);
    }
    let mut picked = Vec::new();
    while picked.len() < count && sorts.values().any(|sort| !sort.is_empty()) {
        for sort in sorts.values_mut() {
            // From the middle outwards, so that neither end of a sort is favoured.
            if !sort.is_empty() && picked.len() < count {
                picked.push(sort.remove(sort.len() / 2));
            }
        }
    }
    picked
}

/// Has the local model write cases for entries of the history.
fn generate(store: &Store, count: usize) -> Result<i32, String> {
    let now = Local::now().timestamp_millis();
    let entries = sample(store.recent(Some(Kind::Text), 50_000).map_err(|e| e.to_string())?, count, now);
    if entries.is_empty() {
        println!("The history has no entries to write cases for.");
        return Ok(0);
    }
    let settings = crate::settings::Settings::load_from(&crate::paths::settings()).0;
    let model = cases::local_only(&settings)?;

    let dir = super::dir().join("eval");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    super::private(&dir, 0o700);
    let path = dir.join("cases.jsonl");
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    super::private(&path, 0o600);
    let (mut written, mut failure) = (BTreeMap::<String, usize>::new(), None);
    for entry in &entries {
        let asked = CaseEntry { text: entry.text.clone().unwrap_or_default(), app: entry.app.clone(), copied: said(now - entry.copied_at as i64) };
        match tauri::async_runtime::block_on(cases::cases_for(&asked, &model)) {
            Ok(found) => {
                for case in found {
                    let line = json!({ "query": case.query, "expect": entry.id, "bucket": case.kind, "now": now });
                    writeln!(file, "{line}").map_err(|e| e.to_string())?;
                    *written.entry(case.kind.to_owned()).or_default() += 1;
                }
            }
            // Without the model there is nothing more to do; say so once.
            Err(why) => {
                failure = Some(why);
                break;
            }
        }
    }
    // Counts only: what the cases say is the user's.
    println!("{} cases for {} entries added to {}", written.values().sum::<usize>(), entries.len(), path.display());
    for (kind, count) in &written {
        println!("  {kind:<16} {count}");
    }
    match failure {
        Some(why) => Err(why),
        None => Ok(0),
    }
}

/// Runs what the arguments ask for and prints the measurements. The exit code.
pub fn run(args: &[String]) -> i32 {
    match measure(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("heyo: {e}");
            1
        }
    }
}

fn measure(args: &[String]) -> Result<i32, String> {
    let options = options(args)?;
    let (store, asked_at) = match &options.corpus {
        Some(path) => {
            let (store, now) = corpus(path)?;
            (store, Some(now))
        }
        None => (Store::open(&super::dir()).map_err(|e| format!("could not open the clipboard history: {e}"))?, None),
    };
    if options.export {
        return Ok(export(&store));
    }
    if let Some(count) = options.generate {
        return generate(&store, count);
    }
    let cases = cases_of(&store, options.cases.as_deref())?;
    if cases.is_empty() {
        println!("No cases: nothing has been picked from a clipboard search yet. Pass a file of cases to measure with.");
        return Ok(0);
    }
    let apps = store.apps().unwrap_or_default();
    let entries = store.count().unwrap_or(0);
    let store = Mutex::new(store);

    // The vectors, when the meaning is measured too.
    let model = options.meaning.then(|| embed::Model::new(embed::command_alone));
    let mut index = Index::default();
    if let Some(model) = &model {
        let started = Instant::now();
        let mut read = 0;
        loop {
            match super::meaning::step(&store, model, 8, |_, _, _| read += 1)? {
                0 => break,
                _ => continue,
            }
        }
        if read > 0 {
            let seconds = started.elapsed().as_secs_f64();
            println!("{read} entries read for their meaning in {seconds:.1} s ({:.1} ms each, the model's start included)", 1000.0 * seconds / read as f64);
        }
        index = store.lock().unwrap().vectors(model.model()).map_err(|e| e.to_string())?;
    }
    let store = store.into_inner().unwrap();

    let mut by_words: BTreeMap<String, Tally> = BTreeMap::new();
    let mut by_both: BTreeMap<String, Tally> = BTreeMap::new();
    let mut vectors: HashMap<String, Vec<f32>> = HashMap::new();
    let (mut asking, mut misses) = (Vec::new(), Vec::new());
    // Found among the first five with the meaning and not without, and the other way round.
    let (mut gained, mut lost) = (0, 0);
    for case in &cases {
        let now = case.now.and_then(|ms| Local.timestamp_millis_opt(ms).single()).or(asked_at).unwrap_or_else(Local::now);
        let parsed = query::parse(&case.query, now, &apps);
        let name = match &case.bucket {
            Some(name) => name.clone(),
            None if !case.has_answer() => NOTHING.to_owned(),
            None => bucket(&case.query, &parsed).to_owned(),
        };
        let start = Instant::now();
        let words = store.search_at(&case.query, None, 50, now).map(|found| found.entries).unwrap_or_default();
        let millis = start.elapsed().as_secs_f64() * 1000.0;
        let names = if case.has_answer() { vec![name.as_str(), "all"] } else { vec![name.as_str()] };
        for name in &names {
            by_words.entry((*name).to_owned()).or_default().note(case, &words, millis);
        }
        let Some(model) = &model else { continue };

        let start = Instant::now();
        let both = match fusion::asked(&parsed, &options.tuning) {
            None => words.clone(),
            Some(text) => {
                if !vectors.contains_key(&text) {
                    let asked = Instant::now();
                    let vector = model.embed(Role::Query, std::slice::from_ref(&text))?.pop().ok_or("no vector")?;
                    asking.push(asked.elapsed().as_secs_f64() * 1000.0);
                    vectors.insert(text.clone(), vector);
                }
                store.search_by_meaning_at(&case.query, None, 50, now, &mut index, &vectors[&text], &options.tuning).map(|found| found.entries).unwrap_or_default()
            }
        };
        let millis = start.elapsed().as_secs_f64() * 1000.0;
        for name in &names {
            by_both.entry((*name).to_owned()).or_default().note(case, &both, millis);
        }
        let top = |found: &[Entry]| found.iter().take(5).any(|entry| case.is_answered_by(entry));
        if case.has_answer() {
            gained += usize::from(top(&both) && !top(&words));
            lost += usize::from(top(&words) && !top(&both));
            if !top(&both) {
                misses.push(format!("  [{name}] {}", case.query));
            }
        }
    }

    println!("{entries} entries in the history, {} cases\n", cases.len());
    table(if model.is_some() { "By words alone" } else { "" }, &by_words);
    if model.is_none() {
        return Ok(0);
    }
    table("By words and meaning", &by_both);
    println!("{:<22} {:>7} {:>7} {:>7}   (points gained with the meaning)", "queries", "R@1", "R@5", "R@10");
    for (name, both) in by_both.iter().filter(|(name, _)| *name != NOTHING) {
        let words = &by_words[name];
        println!("{:<22} {:>+7.0} {:>+7.0} {:>+7.0}", name, both.share(0) - words.share(0), both.share(1) - words.share(1), both.share(2) - words.share(2));
    }
    let answerable = cases.iter().filter(|case| case.has_answer()).count().max(1);
    println!("\nAmong the first five through the meaning alone: {gained} of {answerable} cases ({:.0}%); no longer among them: {lost}", 100.0 * gained as f64 / answerable as f64);
    println!("A query's vector (asking the model): p50 {:.1} ms, p95 {:.1} ms over {} queries", percentile(&asking, 0.5), percentile(&asking, 0.95), asking.len());
    println!("{} vectors, {:.1} MB", index.len(), index.len() as f64 * 384.0 * 4.0 / 1e6);
    if options.misses {
        println!("\nNot among the first five by words and meaning:\n{}", misses.join("\n"));
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/clipboard/fixtures/corpus.jsonl");
    const CASES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/clipboard/fixtures/cases.jsonl");

    #[test]
    fn options_are_read() {
        let args: Vec<String> = ["cases.jsonl", "--meaning", "--corpus", "c.jsonl", "--tuning", "short=0.25,floor=0.82,count=5"].map(String::from).into();
        let options = options(&args).unwrap();
        assert_eq!((options.cases.as_deref(), options.corpus.as_deref(), options.meaning), (Some("cases.jsonl"), Some("c.jsonl"), true));
        assert_eq!(options.tuning, Tuning { short: 0.25, floor: 0.82, count: 5, ..Tuning::default() });
        assert!(super::options(&["--corpus".to_owned()]).is_err());
        assert!(super::options(&["--nope".to_owned()]).is_err());
        assert!(super::options(&["--tuning".to_owned(), "tall=1".to_owned()]).is_err());
    }

    #[test]
    fn times_ago() {
        assert_eq!((ago("20m"), ago("5h"), ago("3d")), (Some(1_200_000), Some(18_000_000), Some(259_200_000)));
        assert_eq!((ago("3w"), ago("d"), ago("")), (None, None, None));
        assert_eq!((said(3_600_000), said(86_400_000), said(3 * 86_400_000), said(8 * 86_400_000), said(30 * 86_400_000)), ("today".into(), "yesterday".into(), "3 days ago".into(), "last week".into(), "4 weeks ago".into()));
    }

    /// The cases that come with the source are about the entries that come
    /// with it: each names an entry there is, and the ones for letters and
    /// words are found by the search by words as it stands.
    #[test]
    fn the_fixture_cases_fit_the_fixture_corpus() {
        let (store, now) = corpus(CORPUS).unwrap();
        let cases = cases_of(&store, Some(CASES)).unwrap();
        assert!(cases.len() >= 100, "{} cases", cases.len());
        assert!(store.count().unwrap() >= 100);
        let mut found = BTreeMap::<String, (usize, usize)>::new();
        for case in &cases {
            let bucket = case.bucket.clone().expect("every fixture case says its kind");
            if let Some(Ids::One(id)) = case.expect {
                assert!(store.get(id).is_some(), "{}: there is no entry {id}", case.query);
            }
            let entries = store.search_at(&case.query, None, 50, now).unwrap().entries;
            let tally = found.entry(bucket).or_default();
            tally.0 += 1;
            tally.1 += usize::from(entries.iter().take(10).any(|entry| case.is_answered_by(entry)));
        }
        // What the search by words is for, it does.
        for bucket in ["keyword", "exact", "hinted"] {
            let (cases, found) = found[bucket];
            assert!(cases >= 10 && found == cases, "{bucket}: {found} of {cases}");
        }
        // And what it cannot do is in the cases too, or they would measure nothing.
        for bucket in ["paraphrase", "description", "cross-language"] {
            let (cases, found) = found[bucket];
            assert!(cases >= 10 && found * 2 < cases, "{bucket}: {found} of {cases}");
        }
    }

    #[test]
    fn the_history_gives_its_own_cases() {
        let (store, _) = corpus(CORPUS).unwrap();
        assert!(cases_of(&store, None).unwrap().is_empty());
        store.chose(1_759_900_000_000, "ssh", 1, 0).unwrap();
        let cases = cases_of(&store, None).unwrap();
        assert_eq!(cases.len(), 1);
        // Asked when it was asked, and answered by the entry that was picked.
        assert_eq!((cases[0].query.as_str(), cases[0].now), ("ssh", Some(1_759_900_000_000)));
        assert!(cases[0].is_answered_by(&store.get(1).unwrap()) && !cases[0].is_answered_by(&store.get(2).unwrap()));
    }

    #[test]
    fn entries_are_sampled_across_their_sorts() {
        let now = 100 * 86_400_000i64;
        let entry = |id: u64, text: &str, days: i64| Entry { id, kind: Kind::Text, text: Some(text.to_owned()), image: None, width: None, height: None, app: None, copied_at: (now - days * 86_400_000) as u64, first_copied_at: 0, times_copied: 1, pinned: false, hash: id };
        let mut entries: Vec<Entry> = (0..50).map(|id| entry(id, "a short english note", 1)).collect();
        entries.push(entry(100, "一条中文的记录", 1));
        entries.push(entry(101, "an old english note", 60));
        entries.push(entry(102, "12345", 1));
        let picked: Vec<u64> = sample(entries, 4, now).iter().map(|entry| entry.id).collect();
        // One of each sort before a second of any; nothing that has no meaning to find.
        assert_eq!(picked.len(), 4);
        assert!(picked.contains(&100) && picked.contains(&101) && !picked.contains(&102));
    }
}
