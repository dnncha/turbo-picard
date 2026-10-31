use std::hash::Hasher;

const TAINT_WORDS: usize = 1 << 19;
const TAINT_MASK: usize = TAINT_WORDS * 64 - 1;

fn low_usize_bits(value: u64) -> usize {
    let usize_bits = u64::try_from(usize::MAX).unwrap_or(u64::MAX);
    usize::try_from(value & usize_bits).unwrap_or_default()
}

pub(super) fn bloom_positions(key: &[u8], mask: usize) -> [usize; 3] {
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write(key);
    let hash = hasher.finish();
    let step = low_usize_bits(hash.rotate_left(32) | 1);
    let first = low_usize_bits(hash);
    [
        first & mask,
        first.wrapping_add(step) & mask,
        first.wrapping_add(step.wrapping_mul(2)) & mask,
    ]
}

pub(super) struct FragmentTaint {
    words: Vec<u64>,
}

impl FragmentTaint {
    pub(super) fn new() -> Self {
        Self { words: Vec::new() }
    }

    pub(super) fn insert(&mut self, key: &[u8]) {
        if self.words.is_empty() {
            self.words.resize(TAINT_WORDS, 0);
        }
        for bit in bloom_positions(key, TAINT_MASK) {
            self.words[bit / 64] |= 1_u64 << (bit % 64);
        }
    }

    pub(super) fn contains(&self, key: &[u8]) -> bool {
        if self.words.is_empty() {
            return false;
        }
        bloom_positions(key, TAINT_MASK)
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
