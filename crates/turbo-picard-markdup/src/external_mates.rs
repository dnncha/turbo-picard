use crate::fragment_taint::bloom_positions;
use crate::{
    ExternalPlanError, ExternalPlanRecord, ExternalSorter, FragmentStage, HashMap,
    decode_external_plan_record, duplicate_candidate_is_pair, encode_external_pair,
    encode_external_plan_record, external_pair_key,
};

const MAX_PENDING_MATES: usize = 4096;
const MAX_PENDING_BUFFER_BYTES: usize = 4 * 1024 * 1024;
const SPILL_FILTER_WORDS: usize = 1 << 17;

pub(super) struct ExternalMateCache {
    pending: HashMap<Vec<u8>, ExternalPlanRecord>,
    buffer_bytes: usize,
    spilled: Vec<u64>,
    max_pending: usize,
    max_buffer_bytes: usize,
}

impl ExternalMateCache {
    pub(super) fn new() -> Self {
        Self {
            pending: HashMap::default(),
            buffer_bytes: 0,
            spilled: Vec::new(),
            max_pending: MAX_PENDING_MATES,
            max_buffer_bytes: MAX_PENDING_BUFFER_BYTES,
        }
    }

    fn filter_bits(&self, key: &[u8]) -> [usize; 3] {
        bloom_positions(key, self.spilled.len() * 64 - 1)
    }

    fn was_spilled(&self, key: &[u8]) -> bool {
        if self.spilled.is_empty() {
            return false;
        }
        self.filter_bits(key)
            .iter()
            .all(|bit| self.spilled[bit / 64] & (1_u64 << (bit % 64)) != 0)
    }

    fn mark_spilled(&mut self, key: &[u8]) {
        if self.spilled.is_empty() {
            self.spilled.resize(SPILL_FILTER_WORDS, 0);
        }
        for bit in self.filter_bits(key) {
            self.spilled[bit / 64] |= 1_u64 << (bit % 64);
        }
    }

    pub(super) fn push(
        &mut self,
        record: ExternalPlanRecord,
        qnames: &mut ExternalSorter,
        pairs: &mut ExternalSorter,
    ) -> Result<(), ExternalPlanError> {
        if self.was_spilled(&record.template_key) {
            if let Some(first) = self.pending.remove(record.template_key.as_slice()) {
                self.buffer_bytes -= record_buffer_bytes(&first);
                spill_record(first, qnames)?;
            }
            return spill_record(record, qnames);
        }
        if let Some(first) = self.pending.remove(record.template_key.as_slice()) {
            self.buffer_bytes -= record_buffer_bytes(&first);
            return push_pair(&first, &record, pairs);
        }
        let bytes = record_buffer_bytes(&record);
        if bytes > self.max_buffer_bytes {
            self.mark_spilled(&record.template_key);
            return spill_record(record, qnames);
        }
        if self.pending.len() == self.max_pending
            || self.buffer_bytes.saturating_add(bytes) > self.max_buffer_bytes
        {
            self.flush(qnames)?;
            if self.was_spilled(&record.template_key) {
                return spill_record(record, qnames);
            }
        }
        self.buffer_bytes += bytes;
        self.pending.insert(record.template_key.clone(), record);
        Ok(())
    }

    pub(super) fn flush(&mut self, qnames: &mut ExternalSorter) -> Result<(), ExternalPlanError> {
        for (key, record) in std::mem::take(&mut self.pending) {
            self.mark_spilled(&key);
            qnames.push(key, encode_external_plan_record(&record, false))?;
        }
        self.buffer_bytes = 0;
        Ok(())
    }
}

fn record_buffer_bytes(record: &ExternalPlanRecord) -> usize {
    [
        record.template_key.len(),
        record.template_key.capacity(),
        record.qname.capacity(),
        record.read_group.as_ref().map_or(0, Vec::capacity),
        record.barcode.primary.as_ref().map_or(0, Vec::capacity),
        record.barcode.read_one.as_ref().map_or(0, Vec::capacity),
        record.barcode.read_two.as_ref().map_or(0, Vec::capacity),
    ]
    .into_iter()
    .fold(0, usize::saturating_add)
}

fn spill_record(
    record: ExternalPlanRecord,
    qnames: &mut ExternalSorter,
) -> Result<(), ExternalPlanError> {
    let payload = encode_external_plan_record(&record, false);
    qnames
        .push(record.template_key, payload)
        .map_err(ExternalPlanError::Sort)
}

pub(super) fn push_pair(
    first: &ExternalPlanRecord,
    second: &ExternalPlanRecord,
    pairs: &mut ExternalSorter,
) -> Result<(), ExternalPlanError> {
    if !matches!(
        (first.flags & 0xc0, second.flags & 0xc0),
        (0x40, 0x80) | (0x80, 0x40)
    ) {
        return Err(ExternalPlanError::MatesNotComplementary);
    }
    let key = external_pair_key(first, second);
    pairs
        .push(key, encode_external_pair(first, second))
        .map_err(ExternalPlanError::Sort)
}

pub(super) fn finish_spilled(
    qnames: ExternalSorter,
    pairs: &mut ExternalSorter,
    mut fragments: Option<&mut FragmentStage>,
) -> Result<(), ExternalPlanError> {
    let mut pending_pair = None::<ExternalPlanRecord>;
    qnames.finish_into(|item| {
        if let Some(pending) =
            pending_pair.take_if(|pending| pending.template_key.as_slice() != item.key.as_slice())
            && let Some(stage) = fragments.as_deref_mut()
        {
            stage.push_required(&pending)?;
        }
        let plan = decode_external_plan_record(&item.key, &item.payload, false)?;
        if !duplicate_candidate_is_pair(plan.flags) {
            return Ok(());
        }
        if let Some(first) = pending_pair.take() {
            if first.template_key == plan.template_key {
                push_pair(&first, &plan, pairs)?;
            } else {
                pending_pair = Some(plan);
            }
        } else {
            pending_pair = Some(plan);
        }
        Ok(())
    })?;
    if let (Some(plan), Some(stage)) = (pending_pair, fragments) {
        stage.push_required(&plan)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ExternalBarcodeValues, bounded_plan, decode_borrowed_external_pair, external_fragment_key,
        external_sorter, push_external_pair_fragments,
    };
    use tempfile::tempdir;
    use turbo_picard_core::external_sort::ExternalSortConfig;

    type JoinedPair = (Vec<u8>, u64, Vec<u8>);

    fn plan(ordinal: u64, name: &[u8], group: Option<&[u8]>, flags: u16) -> ExternalPlanRecord {
        ExternalPlanRecord {
            ordinal,
            library_id: 0,
            read_group: group.map(<[u8]>::to_vec),
            template_key: bounded_plan::template_key(group, name),
            flags,
            reference_id: 0,
            position: ordinal.cast_signed() * 17,
            mate_reference_id: 0,
            mate_position: ordinal.cast_signed() * 17 + 31,
            template_length: 350,
            unclipped_position: ordinal.cast_signed() * 17 - 3,
            quality_score: ordinal % 41,
            qname: name.to_vec(),
            barcode: ExternalBarcodeValues {
                primary: Some(vec![u8::try_from(ordinal % 256).unwrap(); 4]),
                read_one: None,
                read_two: Some(Vec::new()),
            },
        }
    }

    fn join(
        records: &[ExternalPlanRecord],
        cache: Option<(usize, usize)>,
        spill: bool,
        saturate: bool,
    ) -> Result<Vec<JoinedPair>, ExternalPlanError> {
        let tmp = tempdir().unwrap();
        let mut config = ExternalSortConfig::new(tmp.path());
        config.max_records_in_ram = if spill { 1 } else { 10_000 };
        config.prefix = "qname".into();
        let mut qnames = ExternalSorter::new(config.clone()).unwrap();
        config.prefix = "pairs".into();
        let mut pairs = ExternalSorter::new(config).unwrap();
        if let Some((max_pending, max_bytes)) = cache {
            let mut cache = ExternalMateCache::new();
            cache.max_pending = max_pending;
            cache.max_buffer_bytes = max_bytes;
            if saturate {
                cache.spilled = vec![u64::MAX; SPILL_FILTER_WORDS];
            }
            for record in records {
                cache.push(record.clone(), &mut qnames, &mut pairs)?;
                assert!(cache.pending.len() <= max_pending);
                assert!(cache.buffer_bytes <= max_bytes);
                assert_eq!(
                    cache.buffer_bytes,
                    cache.pending.values().map(record_buffer_bytes).sum()
                );
            }
            cache.flush(&mut qnames)?;
        } else {
            for record in records {
                spill_record(record.clone(), &mut qnames)?;
            }
        }
        finish_spilled(qnames, &mut pairs, None)?;
        let mut result = Vec::new();
        pairs.finish_into(|item| {
            for record in decode_borrowed_external_pair(&item.key, &item.payload)? {
                let record = record.into_owned();
                result.push((
                    item.key.clone(),
                    record.ordinal,
                    encode_external_plan_record(&record, true),
                ));
            }
            Ok(())
        })?;
        result.sort();
        Ok(result)
    }

    #[test]
    fn bounded_cache_matches_stable_qname_join_after_eviction_and_reused_names() {
        let mut records = Vec::new();
        let mut occurrences = HashMap::<(Vec<u8>, Option<Vec<u8>>), usize>::default();
        for ordinal in 0..257 {
            let identity = (ordinal * 13 + ordinal / 7) % 19;
            let name = if identity == 18 {
                vec![b'x'; 4096]
            } else {
                format!("read-{identity}").into_bytes()
            };
            let group = match identity % 3 {
                0 => None,
                1 => Some(Vec::new()),
                _ => Some(b"rg1".to_vec()),
            };
            let count = occurrences
                .entry((name.clone(), group.clone()))
                .or_default();
            let flags = if *count % 2 == 0 { 99 } else { 147 };
            *count += 1;
            records.push(plan(ordinal, &name, group.as_deref(), flags));
        }
        for (ordinal, group, flag) in [
            (257, None, 99),
            (258, Some(b"".as_slice()), 99),
            (259, None, 147),
            (260, Some(b"".as_slice()), 147),
        ] {
            records.push(plan(ordinal, b"shared", group, flag));
        }
        for spill in [false, true] {
            let expected = join(&records, None, spill, false).unwrap();
            for count in [1, 2, 7, 4096] {
                for bytes in [128, 4096, MAX_PENDING_BUFFER_BYTES] {
                    assert_eq!(
                        join(&records, Some((count, bytes)), spill, false).unwrap(),
                        expected,
                        "count={count} bytes={bytes} spill={spill}"
                    );
                }
            }
            assert_eq!(
                join(&records, Some((7, 4096)), spill, true).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn randomized_bounded_cache_matches_the_exact_join() {
        fn next(state: &mut u64) -> u64 {
            *state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            *state
        }
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        for iteration in 0..24 {
            let template_count = 2 + (next(&mut state) % 6) as usize;
            let record_count = 4 + (next(&mut state) % 40) as usize;
            let mut records = Vec::with_capacity(record_count);
            let mut occurrences = HashMap::<(Vec<u8>, Option<Vec<u8>>), usize>::default();
            for ordinal in 0..record_count {
                let name =
                    format!("read-{}", next(&mut state) % template_count as u64).into_bytes();
                let group = match next(&mut state) % 3 {
                    0 => None,
                    1 => Some(Vec::new()),
                    _ => Some(b"rg1".to_vec()),
                };
                let count = occurrences
                    .entry((name.clone(), group.clone()))
                    .or_default();
                let flags = if *count % 2 == 0 { 99 } else { 147 };
                *count += 1;
                records.push(plan(ordinal as u64, &name, group.as_deref(), flags));
            }
            let expected = join(&records, None, true, false).unwrap();
            for max_pending in [1, 2, 3, 7] {
                for max_bytes in [64, 512, 4096, MAX_PENDING_BUFFER_BYTES] {
                    assert_eq!(
                        join(&records, Some((max_pending, max_bytes)), true, false).unwrap(),
                        expected,
                        "iteration={iteration} count={max_pending} bytes={max_bytes}"
                    );
                }
            }
            assert_eq!(
                join(&records, Some((2, 128)), true, true).unwrap(),
                expected,
                "iteration={iteration} saturated filter"
            );
        }
    }

    #[test]
    fn spill_filter_collision_flushes_an_already_cached_first_mate() {
        let tmp = tempdir().unwrap();
        let mut qnames = external_sorter(tmp.path(), "qname").unwrap();
        let mut pairs = external_sorter(tmp.path(), "pairs").unwrap();
        let mut cache = ExternalMateCache::new();
        cache
            .push(plan(0, b"shared", None, 99), &mut qnames, &mut pairs)
            .unwrap();
        cache.spilled = vec![u64::MAX; SPILL_FILTER_WORDS];
        cache
            .push(plan(1, b"shared", None, 147), &mut qnames, &mut pairs)
            .unwrap();
        assert!(cache.pending.is_empty());
        assert_eq!(cache.buffer_bytes, 0);
        finish_spilled(qnames, &mut pairs, None).unwrap();
        let mut count = 0;
        pairs
            .finish_into(|_| {
                count += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn oversized_reserved_buffers_do_not_enter_the_mate_cache() {
        let tmp = tempdir().unwrap();
        let mut qnames = external_sorter(tmp.path(), "qname").unwrap();
        let mut pairs = external_sorter(tmp.path(), "pairs").unwrap();
        let mut cache = ExternalMateCache::new();
        cache.max_buffer_bytes = 128;
        let mut record = plan(0, b"short", None, 99);
        record.qname.reserve(4096);
        cache.push(record, &mut qnames, &mut pairs).unwrap();
        assert!(cache.pending.is_empty());
        assert_eq!(cache.buffer_bytes, 0);
    }

    #[test]
    fn cache_and_spill_join_reject_noncomplementary_primary_mates() {
        let records = [plan(0, b"invalid", None, 99), plan(1, b"invalid", None, 99)];
        let expected = join(&records, None, false, false).unwrap_err().to_string();
        for saturate in [false, true] {
            assert_eq!(
                join(&records, Some((1, 4096)), true, saturate)
                    .unwrap_err()
                    .to_string(),
                expected
            );
        }
    }

    #[test]
    fn deferred_fragments_retain_completed_pairs_and_both_kinds_of_orphan() {
        let records = [
            plan(0, b"a-pair", None, 99),
            plan(1, b"a-pair", None, 147),
            plan(2, b"b-orphan", Some(b"rg1"), 99),
            plan(3, b"c-pair", Some(b"rg1"), 99),
            plan(4, b"d-last-orphan", None, 147),
            plan(5, b"c-pair", Some(b"rg1"), 147),
        ];
        let mut expected: Vec<_> = records
            .iter()
            .map(|record| {
                (
                    external_fragment_key(record),
                    record.ordinal,
                    encode_external_plan_record(record, true),
                )
            })
            .collect();
        expected.sort();
        for spill in [false, true] {
            for max_pending in [1, MAX_PENDING_MATES] {
                let tmp = tempdir().unwrap();
                let mut config = ExternalSortConfig::new(tmp.path());
                config.max_records_in_ram = if spill { 1 } else { 10_000 };
                let mut qnames = ExternalSorter::new(config.clone()).unwrap();
                let mut pairs = ExternalSorter::new(config.clone()).unwrap();
                let mut fragments = FragmentStage::new(ExternalSorter::new(config).unwrap());
                let mut cache = ExternalMateCache::new();
                cache.max_pending = max_pending;
                for record in &records {
                    cache.push(record.clone(), &mut qnames, &mut pairs).unwrap();
                }
                cache.flush(&mut qnames).unwrap();
                finish_spilled(qnames, &mut pairs, Some(&mut fragments)).unwrap();
                {
                    let sorter = fragments.sorter.as_mut().expect("fragment sorter");
                    pairs
                        .finish_into(|item| push_external_pair_fragments(&item, sorter))
                        .unwrap();
                }
                let mut actual = Vec::new();
                fragments
                    .take_sorter()
                    .expect("fragment sorter")
                    .finish_into(|item| {
                        let plan = decode_external_plan_record(&item.key, &item.payload, true)?;
                        actual.push((item.key, plan.ordinal, item.payload));
                        Ok(())
                    })
                    .unwrap();
                actual.sort();
                assert_eq!(actual, expected, "spill={spill}, cache={max_pending}");
            }
        }
    }
}
