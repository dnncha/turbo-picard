//! Bounded-memory external sorting for alignment records.
//!
//! Runs are ordinary BAM files.  That avoids lossy record serialisation and
//! permits the final k-way merge to write records directly to the destination.

use crate::temp_runs::OwnedRuns;
use rust_htslib::bam::{self, Read};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

static SORTER_ID: AtomicU64 = AtomicU64::new(0);
const DEFAULT_MAX_RECORDS: usize = 500_000;
const DEFAULT_MERGE_FAN_IN: usize = 32;

pub type RecordCompare = fn(&bam::Record, &bam::Record) -> Ordering;

#[derive(Debug, Clone)]
pub struct BamExternalSortConfig {
    pub tmp_dir: PathBuf,
    pub max_records_in_ram: usize,
    pub merge_fan_in: usize,
    pub prefix: String,
}

impl BamExternalSortConfig {
    pub fn new(tmp_dir: impl Into<PathBuf>) -> Self {
        Self {
            tmp_dir: tmp_dir.into(),
            max_records_in_ram: DEFAULT_MAX_RECORDS,
            merge_fan_in: DEFAULT_MERGE_FAN_IN,
            prefix: "turbo-picard-bam-sort".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BamExternalSortMetrics {
    pub spills: usize,
    pub max_resident_records: usize,
    pub run_count: usize,
}

/// Alignment sorter which only holds the current run and one record per open
/// run in memory.  It owns its temporary files until completion.
pub struct BamExternalSorter {
    config: BamExternalSortConfig,
    header: bam::Header,
    records: Vec<bam::Record>,
    runs: Vec<PathBuf>,
    owned_runs: OwnedRuns,
    metrics: BamExternalSortMetrics,
    instance_id: u64,
    next_run_index: u64,
}

impl BamExternalSorter {
    pub fn new(header: bam::Header, config: BamExternalSortConfig) -> Result<Self, String> {
        fs::create_dir_all(&config.tmp_dir).map_err(|error| error.to_string())?;
        Ok(Self {
            config,
            header,
            records: Vec::new(),
            runs: Vec::new(),
            owned_runs: OwnedRuns::default(),
            metrics: BamExternalSortMetrics::default(),
            instance_id: SORTER_ID.fetch_add(1, AtomicOrdering::Relaxed),
            next_run_index: 0,
        })
    }

    pub fn push(&mut self, record: bam::Record, compare: RecordCompare) -> Result<(), String> {
        self.records.push(record);
        self.metrics.max_resident_records =
            self.metrics.max_resident_records.max(self.records.len());
        if self.records.len() >= self.config.max_records_in_ram.max(1) {
            self.spill_current_run(compare)?;
        }
        Ok(())
    }

    pub fn finish_into(
        mut self,
        compare: RecordCompare,
        mut emit: impl FnMut(bam::Record) -> Result<(), String>,
    ) -> Result<BamExternalSortMetrics, String> {
        if self.runs.is_empty() {
            self.records.sort_unstable_by(compare);
            self.metrics.run_count = usize::from(!self.records.is_empty());
            for record in self.records.drain(..) {
                emit(record)?;
            }
            return Ok(self.metrics);
        }

        self.spill_current_run(compare)?;
        let mut runs = std::mem::take(&mut self.runs);
        while runs.len() > self.config.merge_fan_in.max(2) {
            runs = self.merge_pass(runs, compare)?;
        }
        let result = merge_runs(&runs, compare, emit);
        for run in runs {
            let _ = self.owned_runs.remove(&run);
        }
        result?;
        Ok(self.metrics)
    }

    fn spill_current_run(&mut self, compare: RecordCompare) -> Result<(), String> {
        if self.records.is_empty() {
            return Ok(());
        }
        self.records.sort_unstable_by(compare);
        let path = self.create_run_path()?;
        write_run(&path, &self.header, &self.records)?;
        self.metrics.spills += 1;
        self.metrics.run_count += 1;
        self.runs.push(path);
        self.records.clear();
        Ok(())
    }

    fn merge_pass(
        &mut self,
        runs: Vec<PathBuf>,
        compare: RecordCompare,
    ) -> Result<Vec<PathBuf>, String> {
        let fan_in = self.config.merge_fan_in.max(2);
        let mut merged = Vec::new();
        for chunk in runs.chunks(fan_in) {
            let output = self.create_run_path()?;
            merge_runs_to_file(chunk, &output, &self.header, compare)?;
            self.metrics.run_count += 1;
            merged.push(output);
            for run in chunk {
                self.owned_runs.remove(run)?;
            }
        }
        Ok(merged)
    }

    fn create_run_path(&mut self) -> Result<PathBuf, String> {
        loop {
            let path = self.config.tmp_dir.join(format!(
                "{}-{}-{}-{}.bam",
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
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
    }
}

fn write_run(path: &Path, header: &bam::Header, records: &[bam::Record]) -> Result<(), String> {
    let mut writer = bam::Writer::from_path(path, header, bam::Format::Bam)
        .map_err(|error| error.to_string())?;
    for record in records {
        writer.write(record).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn merge_runs_to_file(
    runs: &[PathBuf],
    output: &Path,
    header: &bam::Header,
    compare: RecordCompare,
) -> Result<(), String> {
    let mut writer = bam::Writer::from_path(output, header, bam::Format::Bam)
        .map_err(|error| error.to_string())?;
    merge_runs(runs, compare, |record| {
        writer.write(&record).map_err(|error| error.to_string())
    })
}

fn merge_runs(
    runs: &[PathBuf],
    compare: RecordCompare,
    mut emit: impl FnMut(bam::Record) -> Result<(), String>,
) -> Result<(), String> {
    let mut readers = Vec::with_capacity(runs.len());
    let mut heap = BinaryHeap::with_capacity(runs.len());
    for path in runs {
        let mut reader = bam::Reader::from_path(path).map_err(|error| error.to_string())?;
        let reader_index = readers.len();
        if let Some(record) = next_record(&mut reader)? {
            heap.push(HeapRecord {
                record,
                reader_index,
                compare,
            });
        }
        readers.push(reader);
    }
    // One resident record per run, O(log k) rather than O(k) comparisons per
    // emitted record. Reader index preserves the former cross-run tie order.
    while let Some(entry) = heap.pop() {
        let reader_index = entry.reader_index;
        emit(entry.record)?;
        if let Some(record) = next_record(&mut readers[reader_index])? {
            heap.push(HeapRecord {
                record,
                reader_index,
                compare,
            });
        }
    }
    Ok(())
}

struct HeapRecord {
    record: bam::Record,
    reader_index: usize,
    compare: RecordCompare,
}

impl Ord for HeapRecord {
    fn cmp(&self, other: &Self) -> Ordering {
        // All entries in a merge use the same comparator. Reverse it for
        // BinaryHeap's max-heap, so the smallest record is emitted first.
        (self.compare)(&other.record, &self.record)
            .then_with(|| other.reader_index.cmp(&self.reader_index))
    }
}

impl PartialOrd for HeapRecord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for HeapRecord {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for HeapRecord {}

fn next_record(reader: &mut bam::Reader) -> Result<Option<bam::Record>, String> {
    let mut record = bam::Record::new();
    match reader.read(&mut record) {
        Some(Ok(())) => Ok(Some(record)),
        Some(Err(error)) => Err(error.to_string()),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qname_compare(left: &bam::Record, right: &bam::Record) -> Ordering {
        left.qname().cmp(right.qname())
    }

    #[test]
    fn spills_and_merges_bam_records() {
        let dir = std::env::temp_dir().join(format!("turbo-picard-bam-sort-{}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut config = BamExternalSortConfig::new(&dir);
        config.max_records_in_ram = 2;
        let mut sorter = BamExternalSorter::new(bam::Header::new(), config).unwrap();
        for qname in [b"b".as_slice(), b"a", b"d", b"c"] {
            let mut record = bam::Record::new();
            record.set(qname, None, b"A", b"F");
            sorter.push(record, qname_compare).unwrap();
        }
        let mut names = Vec::new();
        let metrics = sorter
            .finish_into(qname_compare, |record| {
                names.push(String::from_utf8(record.qname().to_vec()).unwrap());
                Ok(())
            })
            .unwrap();
        assert_eq!(names, ["a", "b", "c", "d"]);
        assert!(metrics.spills >= 2);
        assert!(fs::read_dir(&dir).unwrap().next().is_none());
        let _ = fs::remove_dir(&dir);
    }
    #[test]
    fn heap_merge_preserves_cross_run_tie_order() {
        let dir = std::env::temp_dir().join(format!("turbo-bam-ties-{}", process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut owner = OwnedRuns::default();
        let mut paths = Vec::new();
        for index in 0..4_u8 {
            let path = dir.join(format!("{index}.bam"));
            owner.register(&path);
            let mut record = bam::Record::new();
            record.set(b"same", None, b"A", &[30]);
            record.set_mapq(index);
            write_run(&path, &bam::Header::new(), &[record.clone(), record]).unwrap();
            paths.push(path);
        }
        let mut observed = Vec::new();
        merge_runs(&paths, qname_compare, |record| {
            observed.push(record.mapq());
            Ok(())
        })
        .unwrap();
        assert_eq!(observed, [0, 0, 1, 1, 2, 2, 3, 3]);
        drop(owner);
        fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn intermediate_bam_merge_failure_cleans_all_runs() {
        let dir = std::env::temp_dir().join(format!("turbo-bam-failure-{}", process::id()));
        let mut config = BamExternalSortConfig::new(&dir);
        config.max_records_in_ram = 1;
        config.merge_fan_in = 2;
        let mut sorter = BamExternalSorter::new(bam::Header::new(), config).unwrap();
        for name in [b"e", b"d", b"c", b"b", b"a"] {
            let mut record = bam::Record::new();
            record.set(name, None, b"A", &[30]);
            sorter.push(record, qname_compare).unwrap();
        }
        fs::write(&sorter.runs[0], b"not a BAM").unwrap();
        assert!(sorter.finish_into(qname_compare, |_| Ok(())).is_err());
        assert!(fs::read_dir(&dir).unwrap().next().is_none());
        fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn heap_selection_matches_linear_oracle_with_fewer_comparisons() {
        use std::sync::atomic::{AtomicUsize, Ordering as CountOrdering};
        static COMPARISONS: AtomicUsize = AtomicUsize::new(0);
        fn counted(left: &bam::Record, right: &bam::Record) -> Ordering {
            COMPARISONS.fetch_add(1, CountOrdering::Relaxed);
            qname_compare(left, right)
        }
        fn entry(value: usize, reader_index: usize) -> HeapRecord {
            let mut record = bam::Record::new();
            record.set(format!("r{value:08}").as_bytes(), None, b"A", &[30]);
            HeapRecord {
                record,
                reader_index,
                compare: counted,
            }
        }
        const RUNS: usize = 32;
        const RECORDS: usize = 4096;
        let mut current: Vec<_> = (0..RUNS).map(|i| Some(entry(i, i))).collect();
        let mut expected = Vec::new();
        COMPARISONS.store(0, CountOrdering::Relaxed);
        loop {
            let index = current
                .iter()
                .enumerate()
                .filter_map(|(i, row)| row.as_ref().map(|row| (i, row)))
                .min_by(|(li, l), (ri, r)| counted(&l.record, &r.record).then_with(|| li.cmp(ri)))
                .map(|(i, _)| i);
            let Some(index) = index else {
                break;
            };
            let row = current[index].take().unwrap();
            let name = String::from_utf8(row.record.qname().to_vec()).unwrap();
            let value: usize = name[1..].parse().unwrap();
            expected.push(name);
            if value + RUNS < RECORDS {
                current[index] = Some(entry(value + RUNS, index));
            }
        }
        let linear_comparisons = COMPARISONS.load(CountOrdering::Relaxed);
        COMPARISONS.store(0, CountOrdering::Relaxed);
        let mut heap = BinaryHeap::new();
        for i in 0..RUNS {
            heap.push(entry(i, i));
        }
        let mut observed = Vec::new();
        while let Some(row) = heap.pop() {
            let name = String::from_utf8(row.record.qname().to_vec()).unwrap();
            let value: usize = name[1..].parse().unwrap();
            observed.push(name);
            if value + RUNS < RECORDS {
                heap.push(entry(value + RUNS, row.reader_index));
            }
        }
        let heap_comparisons = COMPARISONS.load(CountOrdering::Relaxed);
        assert_eq!(observed, expected);
        assert_eq!(observed.len(), RECORDS);
        assert!(
            heap_comparisons * 2 < linear_comparisons,
            "heap={heap_comparisons}, linear={linear_comparisons}"
        );
    }
}
