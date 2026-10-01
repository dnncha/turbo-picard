//! Bounded early mate joining for the external duplicate plan.
//!
//! Complete pairs use the same key and original ordinals as the QNAME sort.
//! Evicted identities must never re-enter the cache: doing so could pair the
//! second and third occurrences of a reused QNAME instead of the first and
//! second. The spill filter has false positives only; these send additional
//! records through the exact external join and never affect duplicate decisions.

use super::*;
use std::hash::Hasher;

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
        let mut hasher = rustc_hash::FxHasher::default();
        hasher.write(key);
        let hash = hasher.finish();
        let step = (hash.rotate_left(32) | 1) as usize;
        let mask = self.spilled.len() * 64 - 1;
        let first = hash as usize;
        [
            first & mask,
            first.wrapping_add(step) & mask,
            first.wrapping_add(step.wrapping_mul(2)) & mask,
        ]
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
    ) -> Result<(), String> {
        if self.was_spilled(&record.template_key) {
            // A filter collision can affect an identity already resident in
            // the cache. Spill its earlier occurrence before the incoming one.
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

    pub(super) fn flush(&mut self, qnames: &mut ExternalSorter) -> Result<(), String> {
        // Drop the map's reserved capacity as well as the records on eviction.
        for (key, record) in std::mem::take(&mut self.pending) {
            self.mark_spilled(&key);
            qnames.push(key, encode_external_plan_record(&record, false))?;
        }
        self.buffer_bytes = 0;
        Ok(())
    }
}

fn record_buffer_bytes(record: &ExternalPlanRecord) -> usize {
    // Metadata is bounded separately by MAX_PENDING_MATES. Count the extra
    // map key and every owned record buffer, including reserved capacity.
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

fn spill_record(record: ExternalPlanRecord, qnames: &mut ExternalSorter) -> Result<(), String> {
    let payload = encode_external_plan_record(&record, false);
    qnames.push(record.template_key, payload)
}

pub(super) fn push_pair(
    first: &ExternalPlanRecord,
    second: &ExternalPlanRecord,
    pairs: &mut ExternalSorter,
) -> Result<(), String> {
    if !matches!(
        (first.flags & 0xc0, second.flags & 0xc0),
        (0x40, 0x80) | (0x80, 0x40)
    ) {
        return Err(
            "external MarkDuplicates mates must have complementary first/second flags".to_string(),
        );
    }
    let key = external_pair_key(first, second);
    pairs.push(key.clone(), encode_external_plan_record(first, true))?;
    pairs.push(key, encode_external_plan_record(second, true))
}

// This is the original stable QNAME join, shared by the fallback and the
// differential oracle in tests.
pub(super) fn finish_spilled(
    qnames: ExternalSorter,
    pairs: &mut ExternalSorter,
) -> Result<(), String> {
    let mut pending_pair = None::<ExternalPlanRecord>;
    qnames
        .finish_into(|item| {
            if pending_pair
                .as_ref()
                .is_some_and(|pending| pending.template_key.as_slice() != item.key.as_slice())
            {
                pending_pair = None;
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
        })
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(ordinal: u64, name: &[u8], group: Option<&[u8]>, flags: u16) -> ExternalPlanRecord {
        ExternalPlanRecord {
            ordinal,
            library_id: 0,
            read_group: group.map(<[u8]>::to_vec),
            template_key: bounded_plan::template_key(group, name),
            flags,
            reference_id: 0,
            position: ordinal as i64 * 17,
            mate_reference_id: 0,
            mate_position: ordinal as i64 * 17 + 31,
            template_length: 350,
            unclipped_position: ordinal as i64 * 17 - 3,
            quality_score: ordinal % 41,
            qname: name.to_vec(),
            barcode: ExternalBarcodeValues {
                primary: Some(vec![ordinal as u8; 4]),
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
    ) -> Result<Vec<(Vec<u8>, u64, Vec<u8>)>, String> {
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
        finish_spilled(qnames, &mut pairs)?;
        let mut result = Vec::new();
        pairs.finish_into(|item| {
            let record = decode_external_plan_record(&item.key, &item.payload, true)?;
            result.push((item.key, record.ordinal, item.payload));
            Ok(())
        })?;
        // Completion order can differ; original input ordinals remain the
        // selection/tie-break contract used by duplicate-group processing.
        result.sort();
        Ok(result)
    }

    #[test]
    fn bounded_cache_matches_stable_qname_join_after_eviction_and_reused_names() {
        let mut records = Vec::new();
        let mut occurrences = HashMap::<(Vec<u8>, Option<Vec<u8>>), usize>::default();
        // Interleave templates, reuse names, and leave orphan primary mates.
        // Missing/empty RG, long identity keys and mate barcodes are retained.
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
        // Same QNAME with distinct read groups is a distinct identity.
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
    fn spill_filter_collision_flushes_an_already_cached_first_mate() {
        let tmp = tempdir().unwrap();
        let mut qnames = external_sorter(tmp.path(), "qname").unwrap();
        let mut pairs = external_sorter(tmp.path(), "pairs").unwrap();
        let mut cache = ExternalMateCache::new();
        cache
            .push(plan(0, b"shared", None, 99), &mut qnames, &mut pairs)
            .unwrap();
        // Simulate unrelated spills setting every filter bit for this resident
        // identity. This must defer both mates rather than lose the first.
        cache.spilled = vec![u64::MAX; SPILL_FILTER_WORDS];
        cache
            .push(plan(1, b"shared", None, 147), &mut qnames, &mut pairs)
            .unwrap();
        assert!(cache.pending.is_empty());
        assert_eq!(cache.buffer_bytes, 0);
        finish_spilled(qnames, &mut pairs).unwrap();
        let mut count = 0;
        pairs
            .finish_into(|_| {
                count += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(count, 2);
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
        let expected = join(&records, None, false, false).unwrap_err();
        for saturate in [false, true] {
            assert_eq!(
                join(&records, Some((1, 4096)), true, saturate).unwrap_err(),
                expected
            );
        }
    }
}
