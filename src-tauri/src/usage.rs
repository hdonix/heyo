//! How often and how recently root search items are used ("frecency").

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct Usage {
    count: u32,
    /// Milliseconds since the Unix epoch.
    last: u64,
}

#[derive(Default)]
pub struct UsageStore(Mutex<HashMap<String, Usage>>);

fn path() -> PathBuf {
    crate::paths::data().join("usage.json")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

impl UsageStore {
    pub fn load() -> Self {
        let map = fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        UsageStore(Mutex::new(map))
    }

    fn save(map: &HashMap<String, Usage>) {
        let path = path();
        let result = fs::create_dir_all(path.parent().unwrap())
            .and_then(|_| fs::write(&path, serde_json::to_vec(map).unwrap_or_default()));
        if let Err(e) = result {
            eprintln!("heyo: failed to save usage: {e}");
        }
    }

    pub fn record(&self, id: &str) {
        let mut map = self.0.lock().unwrap();
        let usage = map.entry(id.to_owned()).or_default();
        usage.count += 1;
        usage.last = now();
        Self::save(&map);
    }

    /// Whether the item was ever opened.
    pub fn used(&self, id: &str) -> bool {
        self.0.lock().unwrap().contains_key(id)
    }

    pub fn reset(&self, id: &str) {
        let mut map = self.0.lock().unwrap();
        if map.remove(id).is_some() {
            Self::save(&map);
        }
    }

    /// Frecency: use count, discounted by how long ago the last use was.
    /// Zero for items never used.
    pub fn score(&self, id: &str) -> f32 {
        let map = self.0.lock().unwrap();
        let Some(usage) = map.get(id) else { return 0.0 };
        frecency(usage.count, now().saturating_sub(usage.last) as f32 / DAY)
    }

    /// A store that remembers these uses (id, how often, how many days ago)
    /// and writes nothing.
    #[cfg(test)]
    pub fn with(uses: &[(&str, u32, f32)]) -> Self {
        let last = |days: f32| now() - (days * DAY) as u64;
        UsageStore(Mutex::new(uses.iter().map(|&(id, count, days)| (id.to_owned(), Usage { count, last: last(days) })).collect()))
    }
}

/// Milliseconds.
const DAY: f32 = 86_400_000.0;

/// What `count` uses, the last of them `days` ago, are worth: more for
/// more uses, though ever less for each, and less the longer ago.
fn frecency(count: u32, days: f32) -> f32 {
    let recency = match days {
        d if d < 1.0 => 1.0,
        d if d < 7.0 => 0.7,
        d if d < 30.0 => 0.4,
        _ => 0.2,
    };
    (count as f32).ln_1p() * recency
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_uses_count_for_more_but_each_for_less() {
        assert_eq!(frecency(0, 0.0), 0.0);
        assert!(frecency(2, 0.0) > frecency(1, 0.0));
        assert!(frecency(2, 0.0) - frecency(1, 0.0) > frecency(101, 0.0) - frecency(100, 0.0));
    }

    #[test]
    fn what_was_used_long_ago_fades() {
        let worth = |days| frecency(10, days);
        assert!(worth(0.5) > worth(3.0) && worth(3.0) > worth(20.0) && worth(20.0) > worth(90.0));
        // Fades, but is never forgotten.
        assert!(worth(1000.0) > 0.0);
        // A habit of long ago is worth less than something used once today.
        assert!(frecency(1, 0.0) > frecency(10, 90.0));
    }
}
