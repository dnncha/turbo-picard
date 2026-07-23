//! Bounded-memory stable external sorting over precomputed binary keys.
//!
//! The final merge calls the supplied sink directly.  In particular, this is
//! deliberately not an iterator backed by a `Vec`: callers can sort files
//! much larger than available memory without rebuilding the result in RAM.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

static SORTER_ID: AtomicU64 = AtomicU64::new(0);

const DEFAULT_MAX_RECORDS: usize = 500_000;
const DEFAULT_MAX_BYTES: usize = 256 * 1024 * 1024;
const DEFAULT_MERGE_FAN_IN: usize = 32;

#[derive(Debug, Clone)]
pub struct ExternalSortConfig {
    pub tmp_dir: PathBuf,
    pub max_records_in_ram: usize,
    pub max_bytes_in_ram: usize,
    pub merge_fan_in: usize,
    pub prefix: String,
}

impl ExternalSortConfig {
    pub fn new(tmp_dir: impl Into<PathBuf>) -> Self {
        Self {
            tmp_dir: tmp_dir.into(),
            max_records_in_ram: DEFAULT_MAX_RECORDS,
            max_bytes_in_ram: DEFAULT_MAX_BYTES,
            merge_fan_in: DEFAULT_MERGE_FAN_IN,
            prefix: "turbo-picard-sort".to_string(),
        }
    }
}

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

    fn estimated_bytes(&self) -> usize {
        self.key.len() + self.payload.len() + 24
    }
}

/// A stable file-backed sorter.  `finish_into` is the only completion API so
/// a caller cannot accidentally turn a disk-backed sort into a full-RAM one.
pub struct ExternalSorter {
    config: ExternalSortConfig,
    items: Vec<SortItem>,
    runs: Vec<PathBuf>,
    next_ordinal: u64,
    resident_bytes: usize,
    metrics: ExternalSortMetrics,
    instance_id: u64,
    next_run_index: u64,
}

impl ExternalSorter {
    pub fn new(config: ExternalSortConfig) -> Result<Self, String> {
        fs::create_dir_all(&config.tmp_dir).map_err(|error| error.to_string())?;
        Ok(Self {
            config,
            items: Vec::new(),
            runs: Vec::new(),
            next_ordinal: 0,
            resident_bytes: 0,
            metrics: ExternalSortMetrics::default(),
            instance_id: SORTER_ID.fetch_add(1, AtomicOrdering::Relaxed),
            next_run_index: 0,
        })
    }

    pub fn push(&mut self, key: Vec<u8>, payload: Vec<u8>) -> Result<(), String> {
        let item = SortItem::new(key, self.next_ordinal, payload);
        self.next_ordinal = self.next_ordinal.wrapping_add(1);
        self.resident_bytes = self.resident_bytes.saturating_add(item.estimated_bytes());
        self.items.push(item);
        self.metrics.max_resident_records = self.metrics.max_resident_records.max(self.items.len());
        self.metrics.max_estimated_bytes =
            self.metrics.max_estimated_bytes.max(self.resident_bytes);
        if self.items.len() >= self.config.max_records_in_ram.max(1)
            || self.resident_bytes >= self.config.max_bytes_in_ram.max(1)
        {
            self.spill_current_run()?;
        }
        Ok(())
    }

    /// Complete the sort and deliver each item in order to `emit`.
    ///
    /// No output collection is allocated by this method.  Temporary files are
    /// deleted on success, failure, and drop.
    pub fn finish_into(
        mut self,
        mut emit: impl FnMut(SortItem) -> Result<(), String>,
    ) -> Result<ExternalSortMetrics, String> {
        if self.runs.is_empty() {
            sort_items(&mut self.items);
            self.metrics.run_count = usize::from(!self.items.is_empty());
            for item in self.items.drain(..) {
                emit(item)?;
            }
            return Ok(self.metrics);
        }

        self.spill_current_run()?;
        let mut runs = std::mem::take(&mut self.runs);
        while runs.len() > self.config.merge_fan_in.max(2) {
            runs = self.merge_pass(runs)?;
        }
        let merge_result = merge_runs(&runs, emit);
        for run in runs {
            let _ = remove_if_exists(&run);
        }
        merge_result?;
        Ok(self.metrics)
    }

    pub fn metrics(&self) -> ExternalSortMetrics {
        self.metrics
    }

    fn spill_current_run(&mut self) -> Result<(), String> {
        if self.items.is_empty() {
            return Ok(());
        }
        sort_items(&mut self.items);
        let path = self.create_run_path()?;
        let bytes = write_run(&path, &self.items)?;
        self.metrics.bytes_written = self.metrics.bytes_written.saturating_add(bytes);
        self.metrics.spills += 1;
        self.metrics.run_count += 1;
        self.runs.push(path);
        self.items.clear();
        self.resident_bytes = 0;
        Ok(())
    }

    fn merge_pass(&mut self, runs: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
        let fan_in = self.config.merge_fan_in.max(2);
        let mut merged = Vec::new();
        for chunk in runs.chunks(fan_in) {
            let output = self.create_run_path()?;
            let bytes = merge_runs_to_file(chunk, &output)?;
            self.metrics.bytes_written = self.metrics.bytes_written.saturating_add(bytes);
            self.metrics.run_count += 1;
            merged.push(output);
            for run in chunk {
                remove_if_exists(run)?;
            }
        }
        Ok(merged)
    }

    fn create_run_path(&mut self) -> Result<PathBuf, String> {
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
                Ok(_) => return Ok(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
    }
}

impl Drop for ExternalSorter {
    fn drop(&mut self) {
        for run in self.runs.drain(..) {
            let _ = remove_if_exists(&run);
        }
    }
}

fn sort_items(items: &mut [SortItem]) {
    items.sort_by(compare_items);
}

fn compare_items(left: &SortItem, right: &SortItem) -> Ordering {
    left.key
        .cmp(&right.key)
        .then_with(|| left.ordinal.cmp(&right.ordinal))
}

fn write_run(path: &Path, items: &[SortItem]) -> Result<u64, String> {
    let mut writer = BufWriter::new(File::create(path).map_err(|error| error.to_string())?);
    let mut bytes = 0_u64;
    for item in items {
        bytes += write_item(&mut writer, item).map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn write_item(writer: &mut impl Write, item: &SortItem) -> io::Result<u64> {
    let key_len = u32::try_from(item.key.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "sort key too large"))?;
    let payload_len = u64::try_from(item.payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "sort payload too large"))?;
    writer.write_all(&key_len.to_le_bytes())?;
    writer.write_all(&payload_len.to_le_bytes())?;
    writer.write_all(&item.ordinal.to_le_bytes())?;
    writer.write_all(&item.key)?;
    writer.write_all(&item.payload)?;
    Ok(20 + u64::from(key_len) + payload_len)
}

fn read_item(reader: &mut impl Read) -> Result<Option<SortItem>, String> {
    let mut key_len_bytes = [0_u8; 4];
    match reader.read_exact(&mut key_len_bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.to_string()),
    }
    let mut payload_len_bytes = [0_u8; 8];
    let mut ordinal_bytes = [0_u8; 8];
    reader
        .read_exact(&mut payload_len_bytes)
        .map_err(|error| error.to_string())?;
    reader
        .read_exact(&mut ordinal_bytes)
        .map_err(|error| error.to_string())?;
    let key_len = u32::from_le_bytes(key_len_bytes) as usize;
    let payload_len = usize::try_from(u64::from_le_bytes(payload_len_bytes))
        .map_err(|_| "sort payload too large for this platform".to_string())?;
    let mut key = vec![0_u8; key_len];
    let mut payload = vec![0_u8; payload_len];
    reader
        .read_exact(&mut key)
        .map_err(|error| error.to_string())?;
    reader
        .read_exact(&mut payload)
        .map_err(|error| error.to_string())?;
    Ok(Some(SortItem::new(
        key,
        u64::from_le_bytes(ordinal_bytes),
        payload,
    )))
}

fn merge_runs_to_file(runs: &[PathBuf], output: &Path) -> Result<u64, String> {
    let mut writer = BufWriter::new(File::create(output).map_err(|error| error.to_string())?);
    let mut bytes = 0_u64;
    merge_runs(runs, |item| {
        bytes += write_item(&mut writer, &item).map_err(|error| error.to_string())?;
        Ok(())
    })?;
    writer.flush().map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn merge_runs(
    runs: &[PathBuf],
    mut emit: impl FnMut(SortItem) -> Result<(), String>,
) -> Result<(), String> {
    let mut readers = Vec::with_capacity(runs.len());
    let mut heap = BinaryHeap::<HeapItem>::new();
    for path in runs {
        let mut reader = BufReader::new(File::open(path).map_err(|error| error.to_string())?);
        if let Some(item) = read_item(&mut reader)? {
            heap.push(HeapItem {
                item,
                reader_index: readers.len(),
            });
        }
        readers.push(reader);
    }
    while let Some(heap_item) = heap.pop() {
        let reader_index = heap_item.reader_index;
        emit(heap_item.item)?;
        if let Some(item) = read_item(&mut readers[reader_index])? {
            heap.push(HeapItem { item, reader_index });
        }
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
struct HeapItem {
    item: SortItem,
    reader_index: usize,
}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_items(&other.item, &self.item)
            .then_with(|| other.reader_index.cmp(&self.reader_index))
    }
}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn remove_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("turbo-picard-{name}-{}", process::id()))
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
}
