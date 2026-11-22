use crate::fragment_taint::bloom_positions;
use crate::{
    ExternalBarcodeValues, ExternalPlanError, ExternalSorter, FragmentStage, HashMap, LibraryId,
    MarkDuplicatesConfig, ReadGroupRegistry, append_compact_pair, compact_pair_key_into,
    decode_pending_mate_into, encode_pending_mate_into, external_barcode_values,
};
#[cfg(test)]
use crate::{
    ExternalPlanRecord, decode_external_plan_record, duplicate_candidate_is_pair,
    encode_external_pair, encode_external_plan_record, external_pair_key,
};
use rust_htslib::bam;

const MAX_PENDING_MATES: usize = 32_768;
const MAX_PENDING_BUFFER_BYTES: usize = 16 * 1024 * 1024;
const SPILL_FILTER_WORDS: usize = 1 << 17;
const DISTANT_MATE_WINDOW_BP: i64 = 20_000;

#[derive(Debug, Default)]
struct SpillFilter {
    words: Vec<u64>,
}

impl SpillFilter {
    fn filter_bits(&self, key: &[u8]) -> [usize; 3] {
        bloom_positions(key, self.words.len() * 64 - 1)
    }

    fn was_spilled(&self, key: &[u8]) -> bool {
        if self.words.is_empty() {
            return false;
        }
        self.filter_bits(key)
            .iter()
            .all(|bit| self.words[bit / 64] & (1_u64 << (bit % 64)) != 0)
    }

    fn mark_spilled(&mut self, key: &[u8]) {
        if self.words.is_empty() {
            self.words.resize(SPILL_FILTER_WORDS, 0);
        }
        for bit in self.filter_bits(key) {
            self.words[bit / 64] |= 1_u64 << (bit % 64);
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct PendingMate {
    pub ordinal: u64,
    pub library_id: LibraryId,
    pub rg_id: u32,
    pub flags: u16,
    pub reference_id: i32,
    pub position: i64,
    pub mate_reference_id: i32,
    pub unclipped_position: i64,
    pub quality_score: u64,
    pub barcode: ExternalBarcodeValues,
}

impl PendingMate {
    pub(super) fn empty() -> Self {
        Self {
            ordinal: 0,
            library_id: 0,
            rg_id: 0,
            flags: 0,
            reference_id: -1,
            position: -1,
            mate_reference_id: -1,
            unclipped_position: -1,
            quality_score: 0,
            barcode: ExternalBarcodeValues::empty(),
        }
    }
}

pub(super) struct CompactMateCache {
    pending: HashMap<Vec<u8>, PendingMate>,
    buffer_bytes: usize,
    spilled: SpillFilter,
    max_pending: usize,
    max_buffer_bytes: usize,
    key_scratch: Vec<u8>,
    pair_key_scratch: Vec<u8>,
    payload_scratch: Vec<u8>,
    barcodes_enabled: bool,
}

impl CompactMateCache {
    pub(super) fn new(barcodes_enabled: bool) -> Self {
        Self {
            pending: HashMap::default(),
            buffer_bytes: 0,
            spilled: SpillFilter::default(),
            max_pending: MAX_PENDING_MATES,
            max_buffer_bytes: MAX_PENDING_BUFFER_BYTES,
            key_scratch: Vec::with_capacity(64),
            pair_key_scratch: Vec::with_capacity(64),
            payload_scratch: Vec::with_capacity(128),
            barcodes_enabled,
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "hot-path mate cache avoids constructing an owned intermediate record"
    )]
    pub(super) fn push_record(
        &mut self,
        record: &bam::Record,
        ordinal: u64,
        library_id: LibraryId,
        rg_id: u32,
        unclipped_position: i64,
        quality_score: u64,
        config: &MarkDuplicatesConfig,
        qnames: &mut ExternalSorter,
        pairs: &mut ExternalSorter,
    ) -> Result<(), ExternalPlanError> {
        let qname = record.qname();
        self.key_scratch.clear();
        self.key_scratch.extend_from_slice(&rg_id.to_le_bytes());
        self.key_scratch.extend_from_slice(qname);

        let flags = record.flags();
        let reference_id = record.tid();
        let position = record.pos();
        let mate_reference_id = record.mtid();
        let barcode = if self.barcodes_enabled {
            external_barcode_values(record, config)
        } else {
            ExternalBarcodeValues::empty()
        };

        if self.spilled.was_spilled(&self.key_scratch) {
            if let Some((old_key, first)) = self.pending.remove_entry(self.key_scratch.as_slice()) {
                self.buffer_bytes -= pending_mate_bytes(&old_key, &first);
                encode_pending_mate_into(&mut self.payload_scratch, &first, self.barcodes_enabled);
                qnames.push_slice(&old_key, &self.payload_scratch)?;
            }
            let incoming = PendingMate {
                ordinal,
                library_id,
                rg_id,
                flags,
                reference_id,
                position,
                mate_reference_id,
                unclipped_position,
                quality_score,
                barcode,
            };
            encode_pending_mate_into(&mut self.payload_scratch, &incoming, self.barcodes_enabled);
            return qnames
                .push_slice(&self.key_scratch, &self.payload_scratch)
                .map_err(ExternalPlanError::Sort);
        }

        if let Some((old_key, first)) = self.pending.remove_entry(self.key_scratch.as_slice()) {
            self.buffer_bytes -= pending_mate_bytes(&old_key, &first);
            let second = PendingMate {
                ordinal,
                library_id,
                rg_id,
                flags,
                reference_id,
                position,
                mate_reference_id,
                unclipped_position,
                quality_score,
                barcode,
            };
            return push_compact_pair(
                &first,
                &second,
                qname,
                self.barcodes_enabled,
                pairs,
                &mut self.pair_key_scratch,
                &mut self.payload_scratch,
            );
        }

        let mate = PendingMate {
            ordinal,
            library_id,
            rg_id,
            flags,
            reference_id,
            position,
            mate_reference_id,
            unclipped_position,
            quality_score,
            barcode,
        };
        let bytes = pending_mate_bytes(&self.key_scratch, &mate);
        if bytes > self.max_buffer_bytes {
            self.spilled.mark_spilled(&self.key_scratch);
            encode_pending_mate_into(&mut self.payload_scratch, &mate, self.barcodes_enabled);
            return qnames
                .push_slice(&self.key_scratch, &self.payload_scratch)
                .map_err(ExternalPlanError::Sort);
        }
        if self.pending.len() >= self.max_pending
            || self.buffer_bytes.saturating_add(bytes) > self.max_buffer_bytes
        {
            self.evict_distant_or_flush(reference_id, position, qnames)?;
            if self.spilled.was_spilled(&self.key_scratch) {
                encode_pending_mate_into(&mut self.payload_scratch, &mate, self.barcodes_enabled);
                return qnames
                    .push_slice(&self.key_scratch, &self.payload_scratch)
                    .map_err(ExternalPlanError::Sort);
            }
        }
        self.buffer_bytes += bytes;
        self.pending.insert(self.key_scratch.clone(), mate);
        Ok(())
    }

    fn evict_distant_or_flush(
        &mut self,
        current_tid: i32,
        current_pos: i64,
        qnames: &mut ExternalSorter,
    ) -> Result<(), ExternalPlanError> {
        let before_len = self.pending.len();
        let mut kept = HashMap::default();
        let mut kept_bytes = 0_usize;
        for (key, mate) in std::mem::take(&mut self.pending) {
            let distant = mate.reference_id != current_tid
                || mate.mate_reference_id != current_tid
                || mate.position.saturating_add(DISTANT_MATE_WINDOW_BP) < current_pos;
            if distant {
                self.spilled.mark_spilled(&key);
                encode_pending_mate_into(&mut self.payload_scratch, &mate, self.barcodes_enabled);
                qnames.push_slice(&key, &self.payload_scratch)?;
            } else {
                kept_bytes += pending_mate_bytes(&key, &mate);
                kept.insert(key, mate);
            }
        }
        let evicted = before_len - kept.len();
        if evicted * 4 < before_len || kept_bytes * 4 >= self.max_buffer_bytes * 3 {
            for (key, mate) in kept {
                self.spilled.mark_spilled(&key);
                encode_pending_mate_into(&mut self.payload_scratch, &mate, self.barcodes_enabled);
                qnames.push_slice(&key, &self.payload_scratch)?;
            }
            self.buffer_bytes = 0;
        } else {
            self.pending = kept;
            self.buffer_bytes = kept_bytes;
        }
        Ok(())
    }

    pub(super) fn flush(&mut self, qnames: &mut ExternalSorter) -> Result<(), ExternalPlanError> {
        for (key, mate) in std::mem::take(&mut self.pending) {
            self.spilled.mark_spilled(&key);
            encode_pending_mate_into(&mut self.payload_scratch, &mate, self.barcodes_enabled);
            qnames.push_slice(&key, &self.payload_scratch)?;
        }
        self.buffer_bytes = 0;
        Ok(())
    }
}

fn pending_mate_bytes(key: &[u8], mate: &PendingMate) -> usize {
    size_of::<PendingMate>()
        .saturating_add(key.len())
        .saturating_add(mate.barcode.primary.as_ref().map_or(0, Vec::capacity))
        .saturating_add(mate.barcode.read_one.as_ref().map_or(0, Vec::capacity))
        .saturating_add(mate.barcode.read_two.as_ref().map_or(0, Vec::capacity))
}

fn push_compact_pair(
    first: &PendingMate,
    second: &PendingMate,
    qname: &[u8],
    barcodes_enabled: bool,
    pairs: &mut ExternalSorter,
    key_buf: &mut Vec<u8>,
    payload_buf: &mut Vec<u8>,
) -> Result<(), ExternalPlanError> {
    if !matches!(
        (first.flags & 0xc0, second.flags & 0xc0),
        (0x40, 0x80) | (0x80, 0x40)
    ) {
        return Err(ExternalPlanError::MatesNotComplementary);
    }
    let first_is_left = compact_pair_key_into(key_buf, first, second, barcodes_enabled);
    append_compact_pair(
        payload_buf,
        first,
        second,
        qname,
        first_is_left,
        barcodes_enabled,
    );
    pairs
        .push_slice(key_buf, payload_buf)
        .map_err(ExternalPlanError::Sort)
}

pub(super) fn finish_spilled_compact(
    qnames: ExternalSorter,
    pairs: &mut ExternalSorter,
    mut fragments: Option<&mut FragmentStage>,
    rg_registry: &ReadGroupRegistry,
    barcodes_enabled: bool,
) -> Result<(), ExternalPlanError> {
    let mut pending_key = Vec::<u8>::new();
    let mut pending_mate = None::<PendingMate>;
    let mut scratch_mate = PendingMate::empty();
    let mut pair_key_buf = Vec::with_capacity(64);
    let mut pair_payload_buf = Vec::with_capacity(128);

    qnames.finish_slices(|key, payload| {
        if pending_mate.is_some()
            && pending_key.as_slice() != key
            && let Some(pending) = pending_mate.take()
            && let Some(stage) = fragments.as_deref_mut()
            && pending_key.len() >= 4
        {
            let rg_id = u32::from_le_bytes([
                pending_key[0],
                pending_key[1],
                pending_key[2],
                pending_key[3],
            ]);
            stage.push_required_mate(&pending, &pending_key[4..], rg_registry.get(rg_id))?;
        }
        decode_pending_mate_into(payload, barcodes_enabled, &mut scratch_mate)?;
        if let Some(first) = pending_mate.take() {
            if pending_key.as_slice() == key {
                push_compact_pair(
                    &first,
                    &scratch_mate,
                    &key[4..],
                    barcodes_enabled,
                    pairs,
                    &mut pair_key_buf,
                    &mut pair_payload_buf,
                )?;
            } else {
                pending_key.clear();
                pending_key.extend_from_slice(key);
                pending_mate = Some(scratch_mate.clone());
            }
        } else {
            pending_key.clear();
            pending_key.extend_from_slice(key);
            pending_mate = Some(scratch_mate.clone());
        }
        Ok(())
    })?;

    if let (Some(pending), Some(stage)) = (pending_mate, fragments)
        && pending_key.len() >= 4
    {
        let rg_id = u32::from_le_bytes([
            pending_key[0],
            pending_key[1],
            pending_key[2],
            pending_key[3],
        ]);
        stage.push_required_mate(&pending, &pending_key[4..], rg_registry.get(rg_id))?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) struct ExternalMateCache {
    pending: HashMap<Vec<u8>, ExternalPlanRecord>,
    buffer_bytes: usize,
    spilled: Vec<u64>,
    max_pending: usize,
    max_buffer_bytes: usize,
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
fn spill_record(
    record: ExternalPlanRecord,
    qnames: &mut ExternalSorter,
) -> Result<(), ExternalPlanError> {
    let payload = encode_external_plan_record(&record, false);
    qnames
        .push(record.template_key, payload)
        .map_err(ExternalPlanError::Sort)
}

#[cfg(test)]
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

#[cfg(test)]
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

    #[test]
    fn compact_mate_cache_evicts_distant_mates_while_pairing_local_mates() {
        let make_rec = |qname: &[u8], flags: u16, tid: i32, pos: i64, mtid: i32, mpos: i64| {
            let mut rec = bam::Record::new();
            rec.set(qname, None, b"ACGT", b"FFFF");
            rec.set_flags(flags);
            rec.set_tid(tid);
            rec.set_pos(pos);
            rec.set_mtid(mtid);
            rec.set_mpos(mpos);
            rec
        };
        let config = MarkDuplicatesConfig {
            input: String::new(),
            inputs: Vec::new(),
            output: String::new(),
            metrics_file: String::new(),
            remove_duplicates: false,
            remove_sequencing_duplicates: false,
            assume_sorted: true,
            assume_sort_order: None,
            validation_stringency: None,
            quiet: true,
            create_index: false,
            create_md5_file: false,
            add_pg_tag_to_reads: false,
            tag_duplicate_set_members: false,
            duplicate_scoring_strategy: None,
            read_name_regex: None,
            tagging_policy: None,
            barcode_tag: None,
            read_one_barcode_tag: None,
            read_two_barcode_tag: None,
            clear_dt: false,
            optical_duplicate_pixel_distance: None,
            compression_level: None,
            reference_sequence: None,
            tmp_dir: None,
        };
        let tmp = tempdir().unwrap();
        let mut qnames = ExternalSorter::new(ExternalSortConfig::new(tmp.path())).unwrap();
        let mut pairs = ExternalSorter::new(ExternalSortConfig::new(tmp.path())).unwrap();
        let mut fragments =
            FragmentStage::new(ExternalSorter::new(ExternalSortConfig::new(tmp.path())).unwrap());
        let rg_registry = ReadGroupRegistry::default();
        let mut cache = CompactMateCache::new(false);
        cache.max_pending = 3;

        let r_dist_1 = make_rec(b"distant", 99, 0, 100, 0, 2_000_000);
        let r_local_a1 = make_rec(b"local_a", 99, 0, 1_900_100, 0, 1_900_200);
        let r_local_b1 = make_rec(b"local_b", 99, 0, 1_900_150, 0, 1_900_250);
        let r_trigger = make_rec(b"local_c", 99, 0, 1_900_180, 0, 1_900_280);
        let r_local_a2 = make_rec(b"local_a", 147, 0, 1_900_200, 0, 1_900_100);
        let r_local_b2 = make_rec(b"local_b", 147, 0, 1_900_250, 0, 1_900_150);
        let r_local_c2 = make_rec(b"local_c", 147, 0, 1_900_280, 0, 1_900_180);
        let r_dist_2 = make_rec(b"distant", 147, 0, 2_000_000, 0, 100);

        for (ord, rec) in [
            (0_u64, &r_dist_1),
            (1, &r_local_a1),
            (2, &r_local_b1),
            (3, &r_trigger),
            (4, &r_local_a2),
            (5, &r_local_b2),
            (6, &r_local_c2),
            (7, &r_dist_2),
        ] {
            cache
                .push_record(
                    rec,
                    ord,
                    0,
                    0,
                    rec.pos(),
                    100,
                    &config,
                    &mut qnames,
                    &mut pairs,
                )
                .unwrap();
        }
        cache.flush(&mut qnames).unwrap();
        finish_spilled_compact(
            qnames,
            &mut pairs,
            Some(&mut fragments),
            &rg_registry,
            false,
        )
        .unwrap();

        let mut pair_count = 0_usize;
        pairs
            .finish_slices(|_, _| {
                pair_count += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(pair_count, 4);
    }
}
