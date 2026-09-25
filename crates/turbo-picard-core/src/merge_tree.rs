//! Tournament selection for a fixed set of sorted streams.
//!
//! Only run indices move through the tree; potentially large record wrappers
//! stay in their input slots. Replacing the winning head requires at most one
//! record comparison per level. Equal keys prefer the earlier input run, as
//! the previous heap-based merges did.

use std::cmp::Ordering;

const EMPTY: usize = usize::MAX;

pub(crate) struct MergeTree {
    leaf_count: usize,
    winners: Vec<usize>,
}

impl MergeTree {
    pub(crate) fn new<T>(heads: &[Option<T>], compare: impl Fn(&T, &T) -> Ordering) -> Self {
        let leaf_count = heads.len().max(1).next_power_of_two();
        let mut tree = Self {
            leaf_count,
            winners: vec![EMPTY; 2 * leaf_count],
        };
        for (index, head) in heads.iter().enumerate() {
            if head.is_some() {
                tree.winners[leaf_count + index] = index;
            }
        }
        for node in (1..leaf_count).rev() {
            tree.recompute(node, heads, &compare);
        }
        tree
    }

    pub(crate) fn winner(&self) -> Option<usize> {
        (self.winners[1] != EMPTY).then_some(self.winners[1])
    }

    /// Call after replacing or exhausting one head. All other heads must stay
    /// unchanged until their own update; their cached tournament results are
    /// still valid. The caller must not ask for a winner between taking a head
    /// and updating its slot.
    pub(crate) fn update<T>(
        &mut self,
        index: usize,
        heads: &[Option<T>],
        compare: impl Fn(&T, &T) -> Ordering,
    ) {
        let mut node = self.leaf_count + index;
        self.winners[node] = if heads[index].is_some() { index } else { EMPTY };
        node /= 2;
        while node > 0 {
            self.recompute(node, heads, &compare);
            node /= 2;
        }
    }

    fn recompute<T>(
        &mut self,
        node: usize,
        heads: &[Option<T>],
        compare: &impl Fn(&T, &T) -> Ordering,
    ) {
        let left = self.winners[2 * node];
        let right = self.winners[2 * node + 1];
        self.winners[node] = if left == EMPTY {
            right
        } else if right == EMPTY {
            left
        } else if compare(
            heads[left].as_ref().expect("active left merge head"),
            heads[right].as_ref().expect("active right merge head"),
        )
        .then_with(|| left.cmp(&right))
            != Ordering::Greater
        {
            left
        } else {
            right
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn empty_and_exhausted_streams_have_no_winner() {
        let empty: Vec<Option<u64>> = Vec::new();
        assert_eq!(MergeTree::new(&empty, u64::cmp).winner(), None);
        let mut heads = vec![None, Some(7_u64), None];
        let mut tree = MergeTree::new(&heads, u64::cmp);
        assert_eq!(tree.winner(), Some(1));
        heads[1] = None;
        tree.update(1, &heads, u64::cmp);
        assert_eq!(tree.winner(), None);
    }

    #[test]
    fn equal_keys_keep_earlier_run_order() {
        let mut heads = vec![Some(9_u64); 5];
        let mut tree = MergeTree::new(&heads, u64::cmp);
        for index in 0..heads.len() {
            assert_eq!(tree.winner(), Some(index));
            // A replacement with an equal key must still beat later runs.
            tree.update(index, &heads, u64::cmp);
            assert_eq!(tree.winner(), Some(index));
            heads[index] = None;
            tree.update(index, &heads, u64::cmp);
        }
        assert_eq!(tree.winner(), None);
    }

    #[test]
    fn replacement_recomputes_ancestors_even_when_local_winner_is_unchanged() {
        let mut heads = vec![Some(1_u64), Some(100), Some(3), Some(4)];
        let mut tree = MergeTree::new(&heads, u64::cmp);
        heads[0] = Some(50);
        tree.update(0, &heads, u64::cmp);
        assert_eq!(tree.winner(), Some(2));
    }

    #[test]
    fn randomized_merges_match_exact_stable_oracle() {
        for seed in 0..12_u64 {
            for run_count in 0..66 {
                let mut state = seed + 1;
                let mut runs = Vec::new();
                let mut expected = Vec::new();
                for run in 0..run_count {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let len = (state % 23) as usize;
                    let mut values = Vec::new();
                    for _ in 0..len {
                        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                        values.push(state % 11);
                    }
                    values.sort_unstable();
                    for (offset, value) in values.iter().enumerate() {
                        expected.push((*value, run, offset));
                    }
                    runs.push(values);
                }
                expected.sort_unstable();
                let mut offsets = vec![0; run_count];
                let mut heads: Vec<_> = runs.iter().map(|run| run.first().copied()).collect();
                let mut tree = MergeTree::new(&heads, u64::cmp);
                let mut observed = Vec::new();
                while let Some(run) = tree.winner() {
                    observed.push((heads[run].take().unwrap(), run, offsets[run]));
                    offsets[run] += 1;
                    heads[run] = runs[run].get(offsets[run]).copied();
                    tree.update(run, &heads, u64::cmp);
                }
                assert_eq!(observed, expected, "seed={seed}, runs={run_count}");
            }
        }
    }

    #[test]
    fn comparison_work_is_bounded_by_one_per_level() {
        const RUNS: usize = 32;
        const RECORDS: usize = 4096;
        let calls = Cell::new(0_usize);
        let compare = |left: &usize, right: &usize| {
            calls.set(calls.get() + 1);
            left.cmp(right)
        };
        let mut heads: Vec<_> = (0..RUNS).map(Some).collect();
        let mut tree = MergeTree::new(&heads, compare);
        assert_eq!(calls.get(), RUNS - 1);
        for expected in 0..RECORDS {
            let run = tree.winner().unwrap();
            let value = heads[run].take().unwrap();
            assert_eq!(value, expected);
            heads[run] = (value + RUNS < RECORDS).then_some(value + RUNS);
            let before = calls.get();
            tree.update(run, &heads, compare);
            assert!(calls.get() - before <= 5);
        }
        assert_eq!(tree.winner(), None);
        assert!(calls.get() <= RUNS - 1 + 5 * RECORDS);
    }
}
