use crate::merge_tree::MergeTree;
use crate::sort_error::SortError;
use crate::temp_runs::OwnedRuns;
use std::cmp::Ordering;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

static SORTER_ID: AtomicU64 = AtomicU64::new(0);

const DEFAULT_MAX_RECORDS: usize = 500_000;
const DEFAULT_MAX_BYTES: usize = 256 * 1024 * 1024;
const DEFAULT_MERGE_FAN_IN: usize = 32;
const PARALLEL_SORT_THRESHOLD: usize = 16_384;

#[derive(Debug, Clone)]
pub struct ExternalSortConfig {
    pub tmp_dir: PathBuf,
    pub max_records_in_ram: usize,
    pub max_bytes_in_ram: usize,
    pub merge_fan_in: usize,
    pub threads: usize,
    pub prefix: String,
    pub background_spill: bool,
}

impl ExternalSortConfig {
    pub fn new(tmp_dir: impl Into<PathBuf>) -> Self {
        Self {
            tmp_dir: tmp_dir.into(),
            max_records_in_ram: DEFAULT_MAX_RECORDS,
            max_bytes_in_ram: DEFAULT_MAX_BYTES,
            merge_fan_in: DEFAULT_MERGE_FAN_IN,
            threads: 1,
            prefix: "turbo-picard-sort".to_string(),
            background_spill: false,
        }
    }
}

const SORT_IO_BUFFER_BYTES: usize = 1 << 18;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExternalSortMetrics {
    pub spills: usize,
    pub max_resident_records: usize,
    pub max_estimated_bytes: usize,
    pub run_count: usize,
    pub bytes_written: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortItem {
    pub key: Vec<u8>,
    pub ordinal: u64,
    pub payload: Vec<u8>,
}

impl SortItem {
    fn new(key: Vec<u8>, ordinal: u64, payload: Vec<u8>) -> Self {
        Self {
            key,
            ordinal,
            payload,
        }
    }

    #[cfg(test)]
    fn estimated_bytes(&self) -> usize {
        size_of::<Self>()
            .saturating_add(self.key.capacity())
            .saturating_add(self.payload.capacity())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArenaEntry {
    key_offset: usize,
    key_len: u32,
    payload_len: u32,
    ordinal: u64,
}

impl ArenaEntry {
    fn key<'a>(&self, arena: &'a [u8]) -> &'a [u8] {
        let start = self.key_offset;
        let end = start + self.key_len as usize;
        &arena[start..end]
    }

    fn payload<'a>(&self, arena: &'a [u8]) -> &'a [u8] {
        let start = self.key_offset + self.key_len as usize;
        let end = start + self.payload_len as usize;
        &arena[start..end]
    }
}

pub struct ExternalSorter {
    config: ExternalSortConfig,
    arena: Vec<u8>,
    entries: Vec<ArenaEntry>,
    already_sorted: bool,
    runs: Vec<PathBuf>,
    owned_runs: OwnedRuns,
    next_ordinal: u64,
    metrics: ExternalSortMetrics,
    instance_id: u64,
    next_run_index: u64,
    pending_spill: Option<std::thread::JoinHandle<Result<FinishedSpill, SortError>>>,
}

struct FinishedSpill {
    bytes: u64,
    entries: Vec<ArenaEntry>,
    arena: Vec<u8>,
}

impl Drop for ExternalSorter {
    fn drop(&mut self) {
        if let Some(handle) = self.pending_spill.take() {
            let _ = handle.join();
        }
    }
}

impl ExternalSorter {
    pub fn new(config: ExternalSortConfig) -> Result<Self, SortError> {
        fs::create_dir_all(&config.tmp_dir)?;
        Ok(Self {
            config,
            arena: Vec::new(),
            entries: Vec::new(),
            already_sorted: true,
            runs: Vec::new(),
            owned_runs: OwnedRuns::default(),
            next_ordinal: 0,
            metrics: ExternalSortMetrics::default(),
            instance_id: SORTER_ID.fetch_add(1, AtomicOrdering::Relaxed),
            next_run_index: 0,
            pending_spill: None,
        })
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "preserves existing owned-Vec API; prefer push_slice to avoid allocations"
    )]
    pub fn push(&mut self, key: Vec<u8>, payload: Vec<u8>) -> Result<(), SortError> {
        self.push_slice(&key, &payload)
    }

    pub fn push_slice(&mut self, key: &[u8], payload: &[u8]) -> Result<(), SortError> {
        let key_len = u32::try_from(key.len()).map_err(|_| SortError::PayloadTooLarge)?;
        let payload_len = u32::try_from(payload.len()).map_err(|_| SortError::PayloadTooLarge)?;
        let item_data_bytes = key.len().saturating_add(payload.len());
        let budget = self.config.max_bytes_in_ram.max(1);
        let entry_size = size_of::<ArenaEntry>();
        let required_arena_len = self.arena.len().saturating_add(item_data_bytes);

        if self.entries.len() < self.entries.capacity()
            && required_arena_len <= self.arena.capacity()
        {
            if self.already_sorted
                && let Some(last) = self.entries.last()
                && last.key(&self.arena) > key
            {
                self.already_sorted = false;
            }
            let key_offset = self.arena.len();
            self.arena.extend_from_slice(key);
            self.arena.extend_from_slice(payload);
            let ordinal = self.next_ordinal;
            self.next_ordinal = self.next_ordinal.wrapping_add(1);
            self.entries.push(ArenaEntry {
                key_offset,
                key_len,
                payload_len,
                ordinal,
            });
            let estimated_bytes = self
                .arena
                .capacity()
                .saturating_add(self.entries.capacity().saturating_mul(entry_size));
            self.metrics.max_resident_records =
                self.metrics.max_resident_records.max(self.entries.len());
            self.metrics.max_estimated_bytes =
                self.metrics.max_estimated_bytes.max(estimated_bytes);
            if self.entries.len() >= self.config.max_records_in_ram.max(1)
                || estimated_bytes >= budget
            {
                self.spill_current_run()?;
                if self
                    .arena
                    .capacity()
                    .saturating_add(self.entries.capacity().saturating_mul(entry_size))
                    > budget
                {
                    self.arena = Vec::new();
                    self.entries = Vec::new();
                }
            }
            return Ok(());
        }

        let next_entries_cap = if self.entries.len() == self.entries.capacity() {
            self.entries
                .capacity()
                .saturating_mul(2)
                .max(4)
                .min(self.config.max_records_in_ram.max(1))
                .min(budget.saturating_sub(required_arena_len) / entry_size)
                .max(self.entries.len().saturating_add(1))
        } else {
            self.entries.capacity()
        };
        let next_arena_cap = if required_arena_len > self.arena.capacity() {
            self.arena
                .capacity()
                .saturating_mul(2)
                .max(64)
                .min(budget.saturating_sub(next_entries_cap.saturating_mul(entry_size)))
                .max(required_arena_len)
        } else {
            self.arena.capacity()
        };
        let projected_bytes =
            next_arena_cap.saturating_add(next_entries_cap.saturating_mul(entry_size));
        if !self.entries.is_empty() && projected_bytes > budget {
            self.spill_current_run()?;
        }

        let entries_bytes = self.entries.capacity().saturating_mul(entry_size);
        let arena_bytes = self.arena.capacity();
        if self.entries.is_empty()
            && (arena_bytes.saturating_add(entries_bytes) > budget
                || item_data_bytes.saturating_add(entry_size) > budget)
        {
            self.arena = Vec::new();
            self.entries = Vec::new();
        }

        let required_arena_len = self.arena.len().saturating_add(item_data_bytes);
        let target_entries_cap = if self.entries.len() == self.entries.capacity() {
            let remaining_for_entries = budget.saturating_sub(required_arena_len);
            self.entries
                .capacity()
                .saturating_mul(2)
                .max(4)
                .min(self.config.max_records_in_ram.max(1))
                .min(remaining_for_entries / entry_size)
                .max(self.entries.len().saturating_add(1))
        } else {
            self.entries.capacity()
        };
        if target_entries_cap > self.entries.capacity() {
            self.entries
                .try_reserve_exact(target_entries_cap - self.entries.len())?;
        }
        let entries_cap_bytes = self.entries.capacity().saturating_mul(entry_size);
        if required_arena_len > self.arena.capacity() {
            let target_arena_cap = self
                .arena
                .capacity()
                .saturating_mul(2)
                .max(64)
                .min(budget.saturating_sub(entries_cap_bytes))
                .max(required_arena_len);
            self.arena
                .try_reserve_exact(target_arena_cap - self.arena.len())?;
        }

        if self.already_sorted
            && let Some(last) = self.entries.last()
            && last.key(&self.arena) > key
        {
            self.already_sorted = false;
        }

        let key_offset = self.arena.len();
        self.arena.extend_from_slice(key);
        self.arena.extend_from_slice(payload);
        let ordinal = self.next_ordinal;
        self.next_ordinal = self.next_ordinal.wrapping_add(1);
        self.entries.push(ArenaEntry {
            key_offset,
            key_len,
            payload_len,
            ordinal,
        });

        let estimated_bytes = self
            .arena
            .capacity()
            .saturating_add(self.entries.capacity().saturating_mul(entry_size));
        self.metrics.max_resident_records =
            self.metrics.max_resident_records.max(self.entries.len());
        self.metrics.max_estimated_bytes = self.metrics.max_estimated_bytes.max(estimated_bytes);
        if self.entries.len() >= self.config.max_records_in_ram.max(1) || estimated_bytes >= budget
        {
            self.spill_current_run()?;
            if self
                .arena
                .capacity()
                .saturating_add(self.entries.capacity().saturating_mul(entry_size))
                > budget
            {
                self.arena = Vec::new();
                self.entries = Vec::new();
            }
        }
        Ok(())
    }

    pub fn finish_slices(
        mut self,
        mut emit: impl FnMut(&[u8], &[u8]) -> Result<(), SortError>,
    ) -> Result<ExternalSortMetrics, SortError> {
        if self.runs.is_empty() {
            if !self.already_sorted {
                sort_entries(&mut self.entries, &self.arena, self.config.threads);
            }
            self.metrics.run_count = usize::from(!self.entries.is_empty());
            for entry in &self.entries {
                emit(entry.key(&self.arena), entry.payload(&self.arena))?;
            }
            return Ok(self.metrics);
        }

        self.spill_current_run()?;
        self.wait_for_background_spill()?;
        self.arena = Vec::new();
        self.entries = Vec::new();
        let mut runs = std::mem::take(&mut self.runs);
        while runs.len() > self.config.merge_fan_in.max(2) {
            runs = self.merge_pass(&runs)?;
        }
        let merge_result = merge_runs_parts(&runs, |item| emit(&item.key, &item.payload));
        for run in runs {
            let _ = self.owned_runs.remove(&run);
        }
        merge_result?;
        Ok(self.metrics)
    }

    pub fn finish_into(
        mut self,
        mut emit: impl FnMut(SortItem) -> Result<(), SortError>,
    ) -> Result<ExternalSortMetrics, SortError> {
        if self.runs.is_empty() {
            if !self.already_sorted {
                sort_entries(&mut self.entries, &self.arena, self.config.threads);
            }
            self.metrics.run_count = usize::from(!self.entries.is_empty());
            for entry in &self.entries {
                emit(SortItem::new(
                    entry.key(&self.arena).to_vec(),
                    entry.ordinal,
                    entry.payload(&self.arena).to_vec(),
                ))?;
            }
            return Ok(self.metrics);
        }

        self.spill_current_run()?;
        self.wait_for_background_spill()?;
        self.arena = Vec::new();
        self.entries = Vec::new();
        let mut runs = std::mem::take(&mut self.runs);
        while runs.len() > self.config.merge_fan_in.max(2) {
            runs = self.merge_pass(&runs)?;
        }
        let merge_result = merge_runs_parts(&runs, |item| {
            emit(SortItem::new(
                item.key.clone(),
                item.ordinal,
                item.payload.clone(),
            ))
        });
        for run in runs {
            let _ = self.owned_runs.remove(&run);
        }
        merge_result?;
        Ok(self.metrics)
    }

    pub fn metrics(&self) -> ExternalSortMetrics {
        self.metrics
    }

    fn wait_for_background_spill(&mut self) -> Result<(), SortError> {
        let Some(handle) = self.pending_spill.take() else {
            return Ok(());
        };
        let finished = handle
            .join()
            .map_err(|_| io::Error::other("external sort spill thread panicked"))??;
        self.metrics.bytes_written = self.metrics.bytes_written.saturating_add(finished.bytes);
        self.metrics.spills += 1;
        self.metrics.run_count += 1;
        if self.entries.capacity() == 0 && self.arena.capacity() == 0 {
            self.entries = finished.entries;
            self.arena = finished.arena;
        }
        Ok(())
    }

    fn spill_current_run_in_background(&mut self) -> Result<(), SortError> {
        let path = self.create_run_path()?;
        self.wait_for_background_spill()?;
        let mut entries = std::mem::take(&mut self.entries);
        let mut arena = std::mem::take(&mut self.arena);
        let needs_sort = !self.already_sorted;
        let threads = self.config.threads;
        self.already_sorted = true;
        self.runs.push(path.clone());
        self.pending_spill = Some(std::thread::spawn(move || {
            if needs_sort {
                sort_entries(&mut entries, &arena, threads);
            }
            let bytes = write_run(&path, &entries, &arena)?;
            entries.clear();
            arena.clear();
            Ok(FinishedSpill {
                bytes,
                entries,
                arena,
            })
        }));
        Ok(())
    }

    fn spill_current_run(&mut self) -> Result<(), SortError> {
        if self.entries.is_empty() {
            return Ok(());
        }
        if self.config.background_spill {
            return self.spill_current_run_in_background();
        }
        if !self.already_sorted {
            sort_entries(&mut self.entries, &self.arena, self.config.threads);
        }
        let path = self.create_run_path()?;
        let bytes = write_run(&path, &self.entries, &self.arena)?;
        self.metrics.bytes_written = self.metrics.bytes_written.saturating_add(bytes);
        self.metrics.spills += 1;
        self.metrics.run_count += 1;
        self.runs.push(path);
        self.entries.clear();
        self.arena.clear();
        self.already_sorted = true;
        Ok(())
    }

    fn merge_pass(&mut self, runs: &[PathBuf]) -> Result<Vec<PathBuf>, SortError> {
        let fan_in = self.config.merge_fan_in.max(2);
        let mut merged = Vec::new();
        for chunk in runs.chunks(fan_in) {
            if chunk.len() == 1 {
                merged.push(chunk[0].clone());
                continue;
            }
            let output = self.create_run_path()?;
            let bytes = merge_runs_to_file(chunk, &output)?;
            self.metrics.bytes_written = self.metrics.bytes_written.saturating_add(bytes);
            self.metrics.run_count += 1;
            merged.push(output);
            for run in chunk {
                self.owned_runs.remove(run)?;
            }
        }
        Ok(merged)
    }

    fn create_run_path(&mut self) -> Result<PathBuf, SortError> {
        loop {
            let path = self.config.tmp_dir.join(format!(
                "{}-{}-{}-{}.run",
                self.config.prefix,
                process::id(),
                self.instance_id,
                self.next_run_index
            ));
            self.next_run_index += 1;
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => {
                    self.owned_runs.register(&path);
                    return Ok(path);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}

pub(crate) fn reserve_run_slot<T>(
    items: &mut Vec<T>,
    buffer_bytes: usize,
    max_bytes: usize,
    max_records: usize,
) -> Result<(), SortError> {
    let item_size = size_of::<T>().max(1);
    let budget = max_bytes.max(1);
    if items.is_empty()
        && buffer_bytes.saturating_add(items.capacity().saturating_mul(item_size)) > budget
    {
        *items = Vec::new();
    }
    if items.len() == items.capacity() {
        let required = items.len().saturating_add(1);
        let capacity = items
            .capacity()
            .saturating_mul(2)
            .max(4)
            .min(max_records.max(1))
            .min(budget.saturating_sub(buffer_bytes) / item_size)
            .max(required);
        items.try_reserve_exact(capacity - items.len())?;
    }
    Ok(())
}

fn compare_entries(left: &ArenaEntry, right: &ArenaEntry, arena: &[u8]) -> Ordering {
    left.key(arena)
        .cmp(right.key(arena))
        .then_with(|| left.ordinal.cmp(&right.ordinal))
}

fn sort_entries(entries: &mut Vec<ArenaEntry>, arena: &[u8], threads: usize) {
    let worker_count = if threads > 1 && entries.len() >= PARALLEL_SORT_THRESHOLD {
        threads
            .min(entries.len() / (PARALLEL_SORT_THRESHOLD / 2))
            .max(1)
    } else {
        1
    };
    if worker_count <= 1 {
        entries.sort_unstable_by(|left, right| compare_entries(left, right, arena));
        return;
    }

    let chunk_len = entries.len().div_ceil(worker_count);
    std::thread::scope(|scope| {
        for chunk in entries.chunks_mut(chunk_len) {
            scope.spawn(move || {
                chunk.sort_unstable_by(|left, right| compare_entries(left, right, arena));
            });
        }
    });

    let slices: Vec<&[ArenaEntry]> = entries.chunks(chunk_len).collect();
    let mut offsets = vec![0_usize; slices.len()];
    let mut heads: Vec<Option<ArenaEntry>> = slices.iter().map(|s| s.first().copied()).collect();
    let mut tree = MergeTree::new(&heads, |left, right| compare_entries(left, right, arena));
    let mut merged = Vec::with_capacity(entries.len());
    while let Some(chunk_index) = tree.winner() {
        if let Some(entry) = heads[chunk_index].take() {
            merged.push(entry);
        }
        offsets[chunk_index] += 1;
        heads[chunk_index] = slices[chunk_index].get(offsets[chunk_index]).copied();
        tree.update(chunk_index, &heads, |left, right| {
            compare_entries(left, right, arena)
        });
    }
    drop(slices);
    *entries = merged;
}

fn compare_items(left: &SortItem, right: &SortItem) -> Ordering {
    left.key
        .cmp(&right.key)
        .then_with(|| left.ordinal.cmp(&right.ordinal))
}

fn write_run(path: &Path, entries: &[ArenaEntry], arena: &[u8]) -> Result<u64, SortError> {
    let mut writer = BufWriter::with_capacity(SORT_IO_BUFFER_BYTES, File::create(path)?);
    let mut bytes = 0_u64;
    for entry in entries {
        bytes += write_item_parts(
            &mut writer,
            entry.key(arena),
            entry.ordinal,
            entry.payload(arena),
        )?;
    }
    writer.flush()?;
    Ok(bytes)
}

fn write_item_parts(
    writer: &mut impl Write,
    key: &[u8],
    ordinal: u64,
    payload: &[u8],
) -> io::Result<u64> {
    let key_len = u32::try_from(key.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "sort key too large"))?;
    let payload_len = u64::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "sort payload too large"))?;
    let mut header = [0_u8; 20];
    header[..4].copy_from_slice(&key_len.to_le_bytes());
    header[4..12].copy_from_slice(&payload_len.to_le_bytes());
    header[12..20].copy_from_slice(&ordinal.to_le_bytes());
    writer.write_all(&header)?;
    writer.write_all(key)?;
    writer.write_all(payload)?;
    Ok(20 + u64::from(key_len) + payload_len)
}

#[cfg(test)]
fn write_item(writer: &mut impl Write, item: &SortItem) -> io::Result<u64> {
    write_item_parts(writer, &item.key, item.ordinal, &item.payload)
}

fn read_item_into(reader: &mut impl Read, item: &mut SortItem) -> Result<bool, SortError> {
    let mut header = [0_u8; 20];
    match reader.read_exact(&mut header[..1]) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(false),
        Err(error) => return Err(error.into()),
    }
    reader.read_exact(&mut header[1..])?;
    let key_len = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let payload_len = usize::try_from(u64::from_le_bytes([
        header[4], header[5], header[6], header[7], header[8], header[9], header[10], header[11],
    ]))
    .map_err(|_| SortError::PayloadTooLarge)?;
    item.ordinal = u64::from_le_bytes([
        header[12], header[13], header[14], header[15], header[16], header[17], header[18],
        header[19],
    ]);
    item.key.resize(key_len, 0);
    item.payload.resize(payload_len, 0);
    reader.read_exact(&mut item.key)?;
    reader.read_exact(&mut item.payload)?;
    Ok(true)
}

#[cfg(test)]
fn read_item(reader: &mut impl Read) -> Result<Option<SortItem>, SortError> {
    let mut item = SortItem::new(Vec::new(), 0, Vec::new());
    if read_item_into(reader, &mut item)? {
        Ok(Some(item))
    } else {
        Ok(None)
    }
}

fn merge_runs_to_file(runs: &[PathBuf], output: &Path) -> Result<u64, SortError> {
    let mut writer = BufWriter::with_capacity(SORT_IO_BUFFER_BYTES, File::create(output)?);
    let mut bytes = 0_u64;
    merge_runs_parts(runs, |item| {
        bytes += write_item_parts(&mut writer, &item.key, item.ordinal, &item.payload)?;
        Ok(())
    })?;
    writer.flush()?;
    Ok(bytes)
}

fn merge_runs_parts(
    runs: &[PathBuf],
    mut emit: impl FnMut(&SortItem) -> Result<(), SortError>,
) -> Result<(), SortError> {
    let mut readers = Vec::with_capacity(runs.len());
    let mut heads = Vec::with_capacity(runs.len());
    for path in runs {
        let mut reader = BufReader::with_capacity(SORT_IO_BUFFER_BYTES, File::open(path)?);
        let mut head = SortItem::new(Vec::new(), 0, Vec::new());
        if read_item_into(&mut reader, &mut head)? {
            heads.push(Some(head));
        } else {
            heads.push(None);
        }
        readers.push(reader);
    }
    let mut tree = MergeTree::new(&heads, compare_items);
    while let Some(reader_index) = tree.winner() {
        let mut keep = true;
        if let Some(item) = heads[reader_index].as_mut() {
            emit(item)?;
            keep = read_item_into(&mut readers[reader_index], item)?;
        }
        if !keep {
            heads[reader_index] = None;
        }
        tree.update(reader_index, &heads, compare_items);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("turbo-picard-{name}-{}", process::id()))
    }

    #[test]
    fn run_budget_counts_vector_capacity_at_growth_boundaries() {
        let dir = test_dir("sort-vector-budget");
        let budget = 5 * (size_of::<ArenaEntry>() + 2);
        let mut config = ExternalSortConfig::new(&dir);
        config.max_bytes_in_ram = budget;
        let mut sorter = ExternalSorter::new(config).unwrap();
        for index in 0..40_u8 {
            sorter.push(vec![1], vec![index]).unwrap();
            let allocated =
                sorter.arena.capacity() + sorter.entries.capacity() * size_of::<ArenaEntry>();
            assert!(
                allocated <= budget,
                "allocated {allocated} with budget {budget}"
            );
            assert!(sorter.metrics().max_estimated_bytes <= budget);
        }
        let mut payloads = Vec::new();
        sorter
            .finish_into(|item| {
                payloads.push(item.payload[0]);
                Ok(())
            })
            .unwrap();
        assert_eq!(payloads, (0..40_u8).collect::<Vec<_>>());
        assert!(fs::read_dir(&dir).unwrap().next().is_none());
        fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn oversized_items_are_isolated_and_release_retained_metadata() {
        let dir = test_dir("sort-oversized-isolation");
        let mut config = ExternalSortConfig::new(&dir);
        config.max_bytes_in_ram = 1024;
        let mut sorter = ExternalSorter::new(config).unwrap();
        for index in 0..4_u8 {
            sorter.push(vec![1], vec![index]).unwrap();
        }
        sorter.push(vec![0], vec![0_u8; 4096]).unwrap();
        assert_eq!(sorter.metrics().max_resident_records, 4);
        assert_eq!(
            sorter.metrics().max_estimated_bytes,
            4097 + size_of::<ArenaEntry>()
        );
        assert_eq!(sorter.metrics().spills, 2);
        assert_eq!(sorter.entries, [] as [ArenaEntry; 0]);
        assert_eq!(sorter.arena.capacity(), 0);
        let mut ordinals = Vec::new();
        sorter
            .finish_into(|item| {
                ordinals.push(item.ordinal);
                Ok(())
            })
            .unwrap();
        assert_eq!(ordinals, [4, 0, 1, 2, 3]);
        assert!(fs::read_dir(&dir).unwrap().next().is_none());
        fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn streams_stable_order_after_multiple_spills() {
        let dir = test_dir("external-sort");
        let _ = fs::remove_dir_all(&dir);
        let mut config = ExternalSortConfig::new(&dir);
        config.max_records_in_ram = 2;
        config.max_bytes_in_ram = usize::MAX;
        let mut sorter = ExternalSorter::new(config).unwrap();
        for (key, payload) in [
            (b"b".as_slice(), b"zero".as_slice()),
            (b"a", b"one"),
            (b"b", b"two"),
            (b"a", b"three"),
        ] {
            sorter.push(key.to_vec(), payload.to_vec()).unwrap();
        }
        let mut output = Vec::new();
        let metrics = sorter
            .finish_into(|item| {
                output.push(String::from_utf8(item.payload).unwrap());
                Ok(())
            })
            .unwrap();
        assert_eq!(output, ["one", "three", "zero", "two"]);
        assert!(metrics.spills >= 2);
        assert!(fs::read_dir(&dir).unwrap().next().is_none());
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn truncated_run_is_an_error_at_every_byte_boundary() {
        let item = SortItem::new(b"key".to_vec(), 7, b"payload".to_vec());
        let mut bytes = Vec::new();
        write_item(&mut bytes, &item).unwrap();
        assert!(read_item(&mut &b""[..]).unwrap().is_none());
        for end in 1..bytes.len() {
            assert!(
                read_item(&mut &bytes[..end]).is_err(),
                "accepted truncation at {end}"
            );
        }
        assert_eq!(read_item(&mut bytes.as_slice()).unwrap(), Some(item));
    }

    #[test]
    fn intermediate_merge_failure_cleans_all_owned_runs() {
        let dir = test_dir("sort-failed-merge");
        let mut config = ExternalSortConfig::new(&dir);
        config.max_records_in_ram = 1;
        config.merge_fan_in = 2;
        let mut sorter = ExternalSorter::new(config).unwrap();
        for i in 0..5_u8 {
            sorter.push(vec![i], vec![i]).unwrap();
        }
        fs::write(&sorter.runs[0], b"\0\0").unwrap();
        assert!(sorter.finish_into(|_| Ok(())).is_err());
        assert!(fs::read_dir(&dir).unwrap().next().is_none());
        fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn memory_estimate_counts_reserved_buffers_and_metadata() {
        let item = SortItem::new(Vec::with_capacity(1024), 0, Vec::with_capacity(2048));
        assert_eq!(item.estimated_bytes(), size_of::<SortItem>() + 3072);
    }

    #[test]
    fn background_spills_sort_identically_to_foreground_spills() {
        let mut state = 0x243f_6a88_85a3_08d3_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for (case, limit) in [1_usize, 2, 3, 17, 64, 1000].into_iter().enumerate() {
            let items = (0..600)
                .map(|_| {
                    let key = vec![u8::try_from(next() % 9).unwrap_or(0); 1 + case % 3];
                    let payload = next().to_le_bytes().to_vec();
                    (key, payload)
                })
                .collect::<Vec<_>>();
            let run = |background: bool| {
                let dir = test_dir(&format!("sort-background-{case}-{background}"));
                let mut config = ExternalSortConfig::new(&dir);
                config.max_records_in_ram = limit;
                config.merge_fan_in = 3;
                config.background_spill = background;
                let mut sorter = ExternalSorter::new(config).unwrap();
                for (key, payload) in &items {
                    sorter.push_slice(key, payload).unwrap();
                }
                let mut observed = Vec::new();
                let metrics = sorter
                    .finish_into(|item| {
                        observed.push((item.key, item.ordinal, item.payload));
                        Ok(())
                    })
                    .unwrap();
                assert!(fs::read_dir(&dir).unwrap().next().is_none());
                fs::remove_dir(&dir).unwrap();
                (observed, metrics.spills, metrics.bytes_written)
            };
            assert_eq!(run(true), run(false), "limit {limit}");
        }
    }

    #[test]
    fn singleton_runs_are_not_rewritten_during_merge_passes() {
        let dir = test_dir("sort-singleton-carry");
        let mut config = ExternalSortConfig::new(&dir);
        config.max_records_in_ram = 1;
        config.merge_fan_in = 2;
        let mut sorter = ExternalSorter::new(config).unwrap();
        for value in (0..5_u8).rev() {
            sorter.push(vec![value], vec![value]).unwrap();
        }
        let mut observed = Vec::new();
        let metrics = sorter
            .finish_into(|item| {
                observed.push(item.payload[0]);
                Ok(())
            })
            .unwrap();
        assert_eq!(observed, [0, 1, 2, 3, 4]);
        assert_eq!(metrics.spills, 5);
        assert_eq!(metrics.run_count, 8);
        assert_eq!(metrics.bytes_written, 13 * 22);
        assert!(fs::read_dir(&dir).unwrap().next().is_none());
        fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn equal_keys_remain_stable_with_and_without_spilling() {
        for limit in [1, 2, 7, 100] {
            let dir = test_dir(&format!("sort-equal-keys-{limit}"));
            let mut config = ExternalSortConfig::new(&dir);
            config.max_records_in_ram = limit;
            config.merge_fan_in = 3;
            let mut sorter = ExternalSorter::new(config).unwrap();
            for value in 0..40_u8 {
                sorter.push(vec![1], vec![value]).unwrap();
            }
            let mut observed = Vec::new();
            sorter
                .finish_into(|item| {
                    observed.push(item.payload[0]);
                    Ok(())
                })
                .unwrap();
            assert_eq!(observed, (0..40_u8).collect::<Vec<_>>());
            assert!(fs::read_dir(&dir).unwrap().next().is_none());
            fs::remove_dir(&dir).unwrap();
        }
    }

    #[test]
    fn sink_failure_cleans_merged_and_carried_runs() {
        let dir = test_dir("sort-sink-failure");
        let mut config = ExternalSortConfig::new(&dir);
        config.max_records_in_ram = 1;
        config.merge_fan_in = 2;
        let mut sorter = ExternalSorter::new(config).unwrap();
        for value in 0..5_u8 {
            sorter.push(vec![value], vec![value]).unwrap();
        }
        let error = sorter
            .finish_into(|_| Err(SortError::Sink("sink failed".to_string())))
            .unwrap_err();
        assert_eq!(error.to_string(), "sink failed");
        assert!(fs::read_dir(&dir).unwrap().next().is_none());
        fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn parallel_arena_sort_and_finish_slices_match_single_threaded_order() {
        for threads in [1, 4] {
            let dir = test_dir(&format!("sort-parallel-slices-{threads}"));
            let _ = fs::remove_dir_all(&dir);
            let mut config = ExternalSortConfig::new(&dir);
            config.threads = threads;
            let mut sorter = ExternalSorter::new(config).unwrap();
            let mut state = 1_u64;
            let mut expected = Vec::with_capacity(20_000);
            for ordinal in 0..20_000_u64 {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                let key = (state % 500).to_be_bytes();
                let payload = ordinal.to_le_bytes();
                sorter.push_slice(&key, &payload).unwrap();
                expected.push((key, ordinal, payload));
            }
            expected.sort_unstable();
            let mut observed = Vec::with_capacity(20_000);
            sorter
                .finish_slices(|key, payload| {
                    observed.push((key.to_vec(), payload.to_vec()));
                    Ok(())
                })
                .unwrap();
            assert_eq!(observed.len(), expected.len());
            for ((obs_k, obs_p), (exp_k, _, exp_p)) in observed.iter().zip(&expected) {
                assert_eq!(obs_k.as_slice(), exp_k);
                assert_eq!(obs_p.as_slice(), exp_p);
            }
            fs::remove_dir(&dir).unwrap();
        }
    }
}
