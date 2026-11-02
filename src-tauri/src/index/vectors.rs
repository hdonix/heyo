//! What texts mean, as vectors, and how the nearest to a query's are found.
//! Where they are kept is up to each store (`clipboard/vectors.rs`,
//! `items.rs`).
//!
//! A vector has length one, so the dot product of two is the cosine of the
//! angle between them. Finding the nearest compares the query with every
//! vector: at the sizes a clipboard history or a mailbox reaches that takes
//! a few milliseconds (see the `speed` test), is exact, and stays exact when
//! only the entries of one day or one application are wanted, where an
//! index built for speed would lose its way.
//!
//! The model's vectors say what language a text is in more loudly than what
//! it is about: to an English query every English entry is nearer than the
//! Chinese entry that answers it. So the vectors are compared from the
//! middle of their own language's vectors rather than from the origin
//! (`Index::settle`), which takes most of that out. On the evaluation cases
//! it is what makes a query find an entry in the other language at all.

use std::collections::HashSet;

/// The languages whose vectors are told apart: Chinese, and everything else.
const LANGUAGES: usize = 2;
/// A language with fewer entries than this has no middle of its own to
/// speak of, and shares the middle of all entries; a history with fewer has none.
const ENOUGH: usize = 16;
/// How far towards the middle of its language a vector is moved before it
/// is compared: all the way does a little worse on queries in the entry's
/// own language, on the evaluation cases, and no better across languages.
const PULL: f32 = 0.75;

/// Which of the two a text is, by how it begins.
pub fn language(text: &str) -> usize {
    usize::from(text.chars().take(256).any(|c| matches!(c as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff)))
}

/// The vectors of a history, in memory, one after another.
#[derive(Default)]
pub struct Index {
    /// How many numbers a vector has; 0 until the first is added.
    size: usize,
    ids: Vec<u64>,
    languages: Vec<u8>,
    data: Vec<f32>,
    /// The sum of the vectors of each language, and how many there are.
    sums: [Vec<f64>; LANGUAGES],
    counts: [usize; LANGUAGES],
    /// What each language's vectors are compared from, and for each vector
    /// one over its distance from there; made by `settle`.
    middles: [Vec<f32>; LANGUAGES],
    nearness: Vec<f32>,
    settled: bool,
}

/// The dot product. Eight sums side by side, which the compiler turns into
/// vector instructions; one running sum it may not reorder, and does not.
pub(crate) fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut sums = [0f32; 8];
    let (a_chunks, b_chunks) = (a.chunks_exact(8), b.chunks_exact(8));
    let rest: f32 = a_chunks.remainder().iter().zip(b_chunks.remainder()).map(|(x, y)| x * y).sum();
    for (x, y) in a_chunks.zip(b_chunks) {
        for lane in 0..8 {
            sums[lane] += x[lane] * y[lane];
        }
    }
    sums.iter().sum::<f32>() + rest
}

impl Index {
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Adds the vector of an entry in `language`. One of another size than
    /// the rest is not one of this model's, and is left out.
    pub fn push(&mut self, id: u64, language: usize, vector: &[f32]) {
        if vector.is_empty() || (self.size != 0 && vector.len() != self.size) {
            return;
        }
        if self.size == 0 {
            self.size = vector.len();
            self.sums = std::array::from_fn(|_| vec![0.0; vector.len()]);
        }
        let language = language.min(LANGUAGES - 1);
        self.ids.push(id);
        self.languages.push(language as u8);
        self.data.extend_from_slice(vector);
        for (sum, value) in self.sums[language].iter_mut().zip(vector) {
            *sum += f64::from(*value);
        }
        self.counts[language] += 1;
        self.settled = false;
    }

    /// Works out where each language's vectors are compared from, and how
    /// far each vector is from there. Once after any number of additions:
    /// it costs about what one search does.
    fn settle(&mut self) {
        if self.settled {
            return;
        }
        let total: usize = self.counts.iter().sum();
        let all: Vec<f64> = (0..self.size).map(|i| self.sums.iter().map(|sum| sum[i]).sum()).collect();
        self.middles = std::array::from_fn(|language| {
            let (sum, count) = match (self.counts[language] >= ENOUGH, total >= ENOUGH) {
                (true, _) => (&self.sums[language], self.counts[language]),
                (false, true) => (&all, total),
                (false, false) => return vec![0.0; self.size],
            };
            sum.iter().map(|value| PULL * (*value / count as f64) as f32).collect()
        });
        let lengths: Vec<f32> = self.middles.iter().map(|middle| dot(middle, middle)).collect();
        self.nearness = self
            .data
            .chunks_exact(self.size.max(1))
            .zip(&self.languages)
            .map(|(vector, language)| {
                let middle = &self.middles[*language as usize];
                // |v - m|² for a v of length one.
                let squared = 1.0 - 2.0 * dot(vector, middle) + lengths[*language as usize];
                1.0 / squared.max(1e-12).sqrt()
            })
            .collect();
        self.settled = true;
    }

    /// The `count` entries nearest to `query`, a text in `language`, and how
    /// near each is (1 is the same direction), nearest first; of the
    /// `allowed` ones only, when that is given.
    pub fn nearest(&mut self, query: &[f32], language: usize, allowed: Option<&HashSet<u64>>, count: usize) -> Vec<(u64, f32)> {
        if query.len() != self.size || count == 0 {
            return Vec::new();
        }
        self.settle();
        // The query from the middle of its own language, at length one.
        let middle = &self.middles[language.min(LANGUAGES - 1)];
        let moved: Vec<f32> = query.iter().zip(middle).map(|(value, middle)| value - middle).collect();
        let length = dot(&moved, &moved).sqrt().max(1e-6);
        let query: Vec<f32> = moved.iter().map(|value| value / length).collect();
        let towards: Vec<f32> = self.middles.iter().map(|middle| dot(&query, middle)).collect();

        let mut best: Vec<(u64, f32)> = Vec::with_capacity(count + 1);
        for (row, (id, vector)) in self.ids.iter().zip(self.data.chunks_exact(self.size)).enumerate() {
            if allowed.is_some_and(|allowed| !allowed.contains(id)) {
                continue;
            }
            // The cosine between the query and this vector, both seen from the middle.
            let score = (dot(&query, vector) - towards[self.languages[row] as usize]) * self.nearness[row];
            if best.len() == count && score <= best[count - 1].1 {
                continue;
            }
            let at = best.partition_point(|(_, other)| *other >= score);
            best.insert(at, (*id, score));
            best.truncate(count);
        }
        best
    }
}

pub(crate) fn blob(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|value| value.to_le_bytes()).collect()
}

pub(crate) fn floats(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4).map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A vector of length one pointing mostly along `axis`.
    pub(crate) fn unit(axis: usize, size: usize) -> Vec<f32> {
        let mut vector = vec![0.1f32; size];
        vector[axis % size] = 1.0;
        let norm = dot(&vector, &vector).sqrt();
        vector.iter().map(|value| value / norm).collect()
    }

    #[test]
    fn the_dot_product_of_any_length() {
        for size in [0, 1, 7, 8, 9, 384] {
            let a: Vec<f32> = (0..size).map(|i| i as f32 * 0.5).collect();
            let b: Vec<f32> = (0..size).map(|i| 1.0 - i as f32 * 0.25).collect();
            let plain: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
            assert!((dot(&a, &b) - plain).abs() <= plain.abs() * 1e-5 + 1e-5, "{size}");
        }
    }

    #[test]
    fn the_nearest_come_first() {
        let mut index = Index::default();
        for id in 1..=20u64 {
            index.push(id, 0, &unit(id as usize, 32));
        }
        // Not this model's: another size.
        index.push(99, 0, &unit(1, 16));
        index.push(98, 0, &[]);
        assert_eq!(index.len(), 20);
        let found = index.nearest(&unit(7, 32), 0, None, 3);
        assert_eq!(found[0].0, 7);
        assert!((found[0].1 - 1.0).abs() < 1e-5);
        assert!(found[0].1 > found[1].1 && found[1].1 >= found[2].1);
        assert_eq!(found.len(), 3);
        // Among the allowed only, and that search is exact too.
        let allowed: HashSet<u64> = [3, 4].into();
        let found = index.nearest(&unit(4, 32), 0, Some(&allowed), 5);
        assert_eq!(found.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [4, 3]);
        // A query of another size is not a question this index can answer.
        assert!(index.nearest(&unit(1, 8), 0, None, 3).is_empty());
        assert!(Index::default().nearest(&unit(1, 32), 0, None, 3).is_empty());
    }

    /// A vector that says its language loudly (the first two numbers) and
    /// its subject quietly, as the model's do.
    fn accented(language: usize, subject: usize) -> Vec<f32> {
        let mut vector = vec![0.0f32; 40];
        vector[language] = 1.0;
        vector[2 + subject] = 0.4;
        let norm = dot(&vector, &vector).sqrt();
        vector.iter().map(|value| value / norm).collect()
    }

    #[test]
    fn an_entry_in_the_other_language_is_found_by_its_subject() {
        let mut index = Index::default();
        for subject in 0..20usize {
            index.push(subject as u64, 0, &accented(0, subject));
            index.push(100 + subject as u64, 1, &accented(1, subject));
        }
        // As the vectors stand, every entry in the query's language is nearer
        // than the entry on its subject in the other one.
        let query = accented(0, 5);
        let other = dot(&query, &accented(1, 5));
        assert!((0..20).all(|subject| dot(&query, &accented(0, subject)) > other));
        // Seen from the middle of each language, the subject is what is left.
        let found: Vec<u64> = index.nearest(&query, 0, None, 2).into_iter().map(|(id, _)| id).collect();
        assert_eq!(found, [5, 105]);
        let found: Vec<u64> = index.nearest(&accented(1, 7), 1, None, 2).into_iter().map(|(id, _)| id).collect();
        assert_eq!(found, [107, 7]);
    }

    #[test]
    fn what_language_a_text_is_in() {
        assert_eq!((language("meeting notes"), language("会议纪要"), language("Wayland 支持"), language("")), (0, 1, 1, 0));
        // By how it begins: a long English text that quotes a Chinese word far in is English.
        assert_eq!(language(&format!("{} 发票", "word ".repeat(100))), 0);
    }

    /// `cargo test --release index::vectors::tests::speed -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn speed() {
        let size = 384;
        // Cheap numbers that are not all alike.
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut random = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 40) as f32 / (1u64 << 24) as f32 - 0.5
        };
        for entries in [5_000usize, 50_000] {
            let mut index = Index::default();
            for id in 0..entries {
                let vector: Vec<f32> = (0..size).map(|_| random()).collect();
                let norm = dot(&vector, &vector).sqrt();
                index.push(id as u64, id % 3 / 2, &vector.iter().map(|value| value / norm).collect::<Vec<_>>());
            }
            let query = index.data[..size].to_vec();
            let start = std::time::Instant::now();
            index.settle();
            println!("{entries:>6} entries: settled in {:.2} ms", start.elapsed().as_secs_f64() * 1000.0);
            let some: HashSet<u64> = (0..entries as u64).filter(|id| id % 10 == 0).collect();
            for (what, allowed) in [("all", None), ("a tenth", Some(&some))] {
                let mut times: Vec<f64> = (0..200)
                    .map(|_| {
                        let start = std::time::Instant::now();
                        std::hint::black_box(index.nearest(std::hint::black_box(&query), 0, allowed, 30));
                        start.elapsed().as_secs_f64() * 1000.0
                    })
                    .collect();
                times.sort_by(f64::total_cmp);
                println!("{entries:>6} entries, {what:<8} p50 {:6.2} ms   p95 {:6.2} ms   ({} MB of vectors)", times[100], times[190], entries * size * 4 / 1_000_000);
            }
        }
    }
}
