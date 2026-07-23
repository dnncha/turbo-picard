//! Bounded-memory external sorting for alignment records.
//!
//! Runs are ordinary BAM files.  That avoids lossy record serialisation and
//! permits the final k-way merge to write records directly to the destination.

use rust_htslib::bam::{self, Read};
use std::cmp::Ordering;
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
            let _ = remove_if_exists(&run);
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
                remove_if_exists(run)?;
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
                Ok(_) => return Ok(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
    }
}

impl Drop for BamExternalSorter {
    fn drop(&mut self) {
        for run in self.runs.drain(..) {
            let _ = remove_if_exists(&run);
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
    let mut current = Vec::with_capacity(runs.len());
    for path in runs {
        let mut reader = bam::Reader::from_path(path).map_err(|error| error.to_string())?;
        current.push(next_record(&mut reader)?);
        readers.push(reader);
    }
    loop {
        let Some(index) = current
            .iter()
            .enumerate()
            .filter_map(|(index, record)| record.as_ref().map(|record| (index, record)))
            .min_by(|(left_index, left), (right_index, right)| {
                compare(left, right).then_with(|| left_index.cmp(right_index))
            })
            .map(|(index, _)| index)
        else {
            return Ok(());
        };
        let record = current[index].take().expect("selected record exists");
        emit(record)?;
        current[index] = next_record(&mut readers[index])?;
    }
}

fn next_record(reader: &mut bam::Reader) -> Result<Option<bam::Record>, String> {
    let mut record = bam::Record::new();
    match reader.read(&mut record) {
        Some(Ok(())) => Ok(Some(record)),
        Some(Err(error)) => Err(error.to_string()),
        None => Ok(None),
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
}
