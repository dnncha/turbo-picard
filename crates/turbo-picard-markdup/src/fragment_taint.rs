//! A bounded Bloom filter over the external fragment keys.
//!
//! The fragment stage only changes a duplicate decision when a pair record
//! shares its fragment key with an unpaired candidate: such a group marks
//! every one of its non-pair members, while a group that holds only pair
//! records marks nothing. This filter answers "does an unpaired candidate
//! use this fragment key?" with false positives only. A false positive adds
//! a pair record to a group that holds only pair records, and that group
//! marks no record either way, so the decision is unchanged. A false
//! negative cannot happen, so every group that holds an unpaired candidate
//! keeps all of its pair members.
//!
//! Three hash functions over 33 554 432 bits (1 << 19 words of 64 bits,
//! 4 MiB) give a false positive rate of about (1 - e^(-3k/m))^3: roughly
//! 1.7 percent at 10 000 distinct keys, 15 percent at 100 000, and 63
//! percent at 1 000 000. The word array is allocated on the first insert,
//! so an input without unpaired candidates allocates nothing.

use std::hash::Hasher;

const TAINT_WORDS: usize = 1 << 19;
const TAINT_MASK: usize = TAINT_WORDS * 64 - 1;

pub(super) struct FragmentTaint {
    words: Vec<u64>,
}

impl FragmentTaint {
    pub(super) fn new() -> Self {
        Self { words: Vec::new() }
    }

    fn positions(key: &[u8]) -> [usize; 3] {
        let mut hasher = rustc_hash::FxHasher::default();
        hasher.write(key);
        let hash = hasher.finish();
        let step = (hash.rotate_left(32) | 1) as usize;
        let first = hash as usize;
        [
            first & TAINT_MASK,
            first.wrapping_add(step) & TAINT_MASK,
            first.wrapping_add(step.wrapping_mul(2)) & TAINT_MASK,
        ]
    }

    pub(super) fn insert(&mut self, key: &[u8]) {
        if self.words.is_empty() {
            self.words.resize(TAINT_WORDS, 0);
        }
        for bit in Self::positions(key) {
            self.words[bit / 64] |= 1_u64 << (bit % 64);
        }
    }

    pub(super) fn contains(&self, key: &[u8]) -> bool {
        if self.words.is_empty() {
            return false;
        }
        Self::positions(key)
            .iter()
            .all(|bit| self.words[bit / 64] & (1_u64 << (bit % 64)) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taint_filter_reports_every_inserted_key() {
        let mut taint = FragmentTaint::new();
        let keys: Vec<Vec<u8>> = (0..4096)
            .map(|index| format!("fragment-key-{index}").into_bytes())
            .collect();
        for key in &keys {
            taint.insert(key);
        }
        for key in &keys {
            assert!(taint.contains(key), "an inserted key was reported absent");
        }
    }

    #[test]
    fn taint_filter_allocates_nothing_before_the_first_insert() {
        let taint = FragmentTaint::new();
        assert!(taint.words.is_empty());
        assert!(!taint.contains(b"any-key"));
        assert!(taint.words.is_empty());
    }
}
