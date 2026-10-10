//! File search: the names of everything under the home directory, held in
//! memory while the command is in use. Scanning takes about a second, so
//! there is no index on disk — each time the command opens, the previous
//! scan answers at once while a new one runs.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ignore::{WalkBuilder, WalkState};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use serde::Serialize;

/// Directories that are large and rarely what someone is looking for.
/// Hidden files and directories are skipped as well.
const PRUNE: &[&str] = &["node_modules", "target", "__pycache__", "site-packages", "venv", "snap", "miniconda3"];
/// The scan is dropped when the command has not been used for this long.
const KEEP: Duration = Duration::from_secs(300);
const MAX_RESULTS: usize = 200;

/// Paths relative to the root, packed into one buffer.
#[derive(Default)]
struct Scan {
    text: String,
    /// Where each path ends in `text`, and whether it is a directory.
    entries: Vec<(u32, bool)>,
}

impl Scan {
    fn push(&mut self, path: &str, is_dir: bool) {
        // Offsets are 32-bit: 4 GB of names is far more than a home directory has.
        if self.text.len() + path.len() > u32::MAX as usize {
            return;
        }
        self.text.push_str(path);
        self.entries.push((self.text.len() as u32, is_dir));
    }

    fn get(&self, index: usize) -> (&str, bool) {
        let start = if index == 0 { 0 } else { self.entries[index - 1].0 as usize };
        let (end, is_dir) = self.entries[index];
        (&self.text[start..end as usize], is_dir)
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Default)]
struct State {
    scan: Option<Arc<Scan>>,
    last_used: Option<Instant>,
}

#[derive(Default)]
pub struct Files {
    state: Mutex<State>,
    scanning: AtomicBool,
}

#[derive(Serialize)]
pub struct Found {
    path: String,
    #[serde(rename = "isDirectory")]
    is_dir: bool,
}

#[derive(Serialize)]
pub struct Results {
    files: Vec<Found>,
    /// A scan is running: results may be from the previous one, or missing.
    scanning: bool,
    /// How many files and folders are known.
    total: usize,
}

fn scan(root: &Path) -> Scan {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    let walker = WalkBuilder::new(root)
        .standard_filters(false)
        .hidden(true)
        .same_file_system(true)
        .threads(threads)
        .filter_entry(|entry| entry.file_name().to_str().is_none_or(|name| !PRUNE.contains(&name)))
        .build_parallel();
    let parts: Mutex<Vec<Scan>> = Mutex::default();
    walker.run(|| {
        // Each thread fills its own part, handed over when the thread ends.
        struct Part<'a>(Scan, &'a Mutex<Vec<Scan>>);
        impl Drop for Part<'_> {
            fn drop(&mut self) {
                self.1.lock().unwrap().push(std::mem::take(&mut self.0));
            }
        }
        let mut part = Part(Scan::default(), &parts);
        Box::new(move |entry| {
            let Ok(entry) = entry else { return WalkState::Continue };
            let relative = entry.path().strip_prefix(root).ok().and_then(Path::to_str).filter(|p| !p.is_empty());
            if let Some(path) = relative {
                part.0.push(path, entry.file_type().is_some_and(|t| t.is_dir()));
            }
            WalkState::Continue
        })
    });
    let mut all = Scan::default();
    for part in parts.into_inner().unwrap() {
        for index in 0..part.len() {
            let (path, is_dir) = part.get(index);
            all.push(path, is_dir);
        }
    }
    all
}

fn home() -> PathBuf {
    crate::paths::home()
}

impl Files {
    /// Starts a scan unless one is running. Called when the command opens.
    pub fn refresh(self: &Arc<Self>) {
        self.state.lock().unwrap().last_used = Some(Instant::now());
        if self.scanning.swap(true, Ordering::SeqCst) {
            return;
        }
        let files = self.clone();
        std::thread::spawn(move || {
            let scan = Arc::new(scan(&home()));
            files.state.lock().unwrap().scan = Some(scan);
            files.scanning.store(false, Ordering::SeqCst);
            // Give the memory back once the command is no longer in use.
            loop {
                std::thread::sleep(Duration::from_secs(30));
                if files.scanning.load(Ordering::SeqCst) {
                    return; // A newer scan's thread takes over.
                }
                let mut state = files.state.lock().unwrap();
                if state.last_used.is_none_or(|t| t.elapsed() > KEEP) {
                    state.scan = None;
                    return;
                }
            }
        });
    }

    pub fn search(self: &Arc<Self>, query: &str) -> Results {
        let scan = {
            let mut state = self.state.lock().unwrap();
            state.last_used = Some(Instant::now());
            state.scan.clone()
        };
        let Some(scan) = scan else {
            // Dropped after a long pause, or never scanned.
            self.refresh();
            return Results { files: Vec::new(), scanning: true, total: 0 };
        };
        let root = home();
        let files = best(&scan, query)
            .into_iter()
            .map(|index| {
                let (path, is_dir) = scan.get(index);
                Found { path: root.join(path).to_string_lossy().into_owned(), is_dir }
            })
            .collect();
        Results { files, scanning: self.scanning.load(Ordering::SeqCst), total: scan.len() }
    }
}

/// The entries matching `query` best, as indices into the scan.
fn best(scan: &Scan, query: &str) -> Vec<usize> {
    let query = query.trim();
    if query.is_empty() || scan.len() == 0 {
        return Vec::new();
    }
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(12);
    let chunk = scan.len().div_ceil(threads);
    let mut scored: Vec<(u32, usize)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|t| {
                let pattern = &pattern;
                scope.spawn(move || {
                    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
                    let mut buffer = Vec::new();
                    let mut found = Vec::new();
                    for index in (t * chunk)..((t + 1) * chunk).min(scan.len()) {
                        let (path, _) = scan.get(index);
                        if let Some(score) = score(pattern, &mut matcher, &mut buffer, path) {
                            found.push((score, index));
                        }
                    }
                    // Only the best of each chunk can be among the best overall.
                    found.sort_unstable_by(|a, b| b.cmp(a));
                    found.truncate(MAX_RESULTS);
                    found
                })
            })
            .collect();
        workers.into_iter().flat_map(|w| w.join().unwrap_or_default()).collect()
    });
    // Best score first; among equals, the shorter path.
    scored.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| scan.get(a.1).0.len().cmp(&scan.get(b.1).0.len())));
    scored.truncate(MAX_RESULTS);
    scored.into_iter().map(|(_, index)| index).collect()
}

/// A match in the file's own name counts for much more than one spread over
/// the folders above it.
fn score(pattern: &Pattern, matcher: &mut Matcher, buffer: &mut Vec<char>, path: &str) -> Option<u32> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if let Some(score) = pattern.score(Utf32Str::new(name, buffer), matcher) {
        // Shallow files first among equally good names.
        let depth = path.bytes().filter(|b| *b == b'/').count() as u32;
        return Some(score * 4 + 2000 - depth.min(50) * 8);
    }
    if name.len() == path.len() {
        return None;
    }
    pattern.score(Utf32Str::new(path, buffer), matcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Scan {
        let mut scan = Scan::default();
        for (path, is_dir) in [
            ("repos/heyo/src-tauri/tauri.conf.json", false),
            ("repos/heyo/src-tauri", true),
            ("Documents/tauri notes/config-ideas.md", false),
            ("Documents/报告 2026.pdf", false),
            ("Downloads/unrelated.txt", false),
        ] {
            scan.push(path, is_dir);
        }
        scan
    }

    fn names(scan: &Scan, query: &str) -> Vec<String> {
        best(scan, query).into_iter().map(|i| scan.get(i).0.to_owned()).collect()
    }

    #[test]
    fn name_matches_come_before_path_matches() {
        let scan = sample();
        let found = names(&scan, "tauri conf");
        assert_eq!(found[0], "repos/heyo/src-tauri/tauri.conf.json");
        assert!(found.contains(&"Documents/tauri notes/config-ideas.md".to_owned()));
        assert!(!found.contains(&"Downloads/unrelated.txt".to_owned()));
    }

    #[test]
    fn finds_non_ascii_names_and_keeps_kinds() {
        let scan = sample();
        assert_eq!(names(&scan, "报告"), ["Documents/报告 2026.pdf"]);
        let index = best(&scan, "src-tauri")[0];
        assert_eq!(scan.get(index), ("repos/heyo/src-tauri", true));
    }

    /// Timings on the real home directory: `cargo test --release measure -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn measure() {
        let started = Instant::now();
        let scan = scan(&home());
        println!("scan: {} entries, {} MB, {:?}", scan.len(), (scan.text.len() + scan.entries.len() * 8) >> 20, started.elapsed());
        for query in ["tauri conf", "readme", "报告", "screenshot png", "x"] {
            let started = Instant::now();
            let found = best(&scan, query);
            println!("{query:?}: {} results in {:?}; first: {:?}", found.len(), started.elapsed(), found.first().map(|i| scan.get(*i).0));
        }
    }
}
