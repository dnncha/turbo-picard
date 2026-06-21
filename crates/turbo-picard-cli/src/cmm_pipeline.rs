use crossbeam_channel::{Receiver, Sender, bounded};
use rust_htslib::bam::{self, Read};
use std::any::Any;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};

/// Per-collector fast reject gates for a single BAM record.
///
/// Mirrors riker's `field_needs` union at the dispatch layer: skip collectors
/// that cannot observe this record before taking their mutex.
#[derive(Clone, Copy, Debug, Default)]
pub struct CmmRecordGates {
    pub alignment: bool,
    pub insert_size: bool,
    pub base_distribution: bool,
    pub quality_distribution: bool,
    pub mean_quality: bool,
    pub quality_yield: bool,
    pub gc_bias: bool,
    pub wgs: bool,
}

impl CmmRecordGates {
    #[inline]
    pub fn for_record(
        record: &bam::Record,
        aligned_reads_only: bool,
        pf_reads_only: bool,
        include_secondary_yield: bool,
        include_supplemental_yield: bool,
    ) -> Self {
        let secondary = record.is_secondary();
        let supplementary = record.is_supplementary();
        let unmapped = record.is_unmapped();
        let captures_quality =
            !super::skip_quality_metric_record(record, aligned_reads_only, pf_reads_only);
        Self {
            alignment: !secondary && !supplementary,
            insert_size: record.is_paired()
                && !unmapped
                && !record.is_mate_unmapped()
                && !secondary
                && !supplementary
                && record.is_last_in_template()
                && record.insert_size() != 0,
            base_distribution: captures_quality,
            quality_distribution: captures_quality,
            mean_quality: captures_quality,
            quality_yield: (!secondary || include_secondary_yield)
                && (!supplementary || include_supplemental_yield),
            gc_bias: !secondary && !supplementary && !unmapped,
            wgs: !unmapped && !secondary && !supplementary,
        }
    }

    #[inline]
    fn has_any(&self) -> bool {
        self.alignment
            || self.insert_size
            || self.base_distribution
            || self.quality_distribution
            || self.mean_quality
            || self.quality_yield
            || self.gc_bias
            || self.wgs
    }
}

pub const DEFAULT_BATCH_POOL_DEPTH: usize = 16;
pub const DEFAULT_BATCH_SIZE: usize = 512;

pub struct CmmBatchRecord {
    pub record: bam::Record,
    pub gates: CmmRecordGates,
}

pub type CmmBatchHandler = Box<dyn Fn(&[CmmBatchRecord]) -> Result<(), String> + Send>;
type CmmWorkerGroup = Vec<CmmBatchHandler>;

#[derive(Clone, Copy, Debug)]
struct CmmWorkerPoolOptions {
    worker_cap: usize,
    batch_size: usize,
    queue_depth: usize,
}

struct WorkerJob {
    batch: Arc<Vec<CmmBatchRecord>>,
    ack: Sender<()>,
}

struct CompletionJob {
    batch: Arc<Vec<CmmBatchRecord>>,
    ack: Receiver<()>,
}

pub struct CmmWorkerPool {
    work_txs: Vec<Sender<WorkerJob>>,
    handles: Vec<JoinHandle<()>>,
    worker_count: usize,
    batch_pool: Receiver<Vec<CmmBatchRecord>>,
    batch_return: Sender<Vec<CmmBatchRecord>>,
    inflight: Arc<AtomicUsize>,
    batch_size: usize,
    queue_depth: usize,
    completion_rx: Receiver<Vec<CmmBatchRecord>>,
    completion_job_tx: Option<Sender<CompletionJob>>,
    completion_handle: Option<JoinHandle<()>>,
    error_tx: Sender<String>,
    error_rx: Receiver<String>,
    poison: Arc<AtomicBool>,
}

impl CmmWorkerPool {
    pub fn new(handlers: Vec<CmmBatchHandler>, worker_cap: usize) -> Self {
        Self::with_options(
            handlers,
            CmmWorkerPoolOptions {
                worker_cap,
                batch_size: cmm_batch_size(),
                queue_depth: cmm_queue_depth(),
            },
        )
    }

    fn with_options(handlers: Vec<CmmBatchHandler>, options: CmmWorkerPoolOptions) -> Self {
        let worker_groups = worker_groups_for_handlers(handlers, options.worker_cap);
        let worker_count = worker_groups.len();
        let batch_size = options.batch_size.max(1);
        let queue_depth = options.queue_depth.max(1);
        let (batch_return, batch_pool) = bounded(queue_depth);
        let (completion_tx, completion_rx) = crossbeam_channel::unbounded::<Vec<CmmBatchRecord>>();
        let (error_tx, error_rx) = bounded(1);
        let inflight = Arc::new(AtomicUsize::new(0));
        let poison = Arc::new(AtomicBool::new(false));
        let mut work_txs = Vec::with_capacity(worker_count);
        let mut handles = Vec::with_capacity(worker_count);
        let (completion_job_tx, completion_job_rx) = bounded(queue_depth);

        for handlers in worker_groups {
            let (work_tx, work_rx) = bounded::<WorkerJob>(queue_depth);
            let error_tx = error_tx.clone();
            let poison = Arc::clone(&poison);
            work_txs.push(work_tx);
            handles.push(thread::spawn(move || {
                while let Ok(WorkerJob { batch, ack }) = work_rx.recv() {
                    for handler in &handlers {
                        let result =
                            panic::catch_unwind(AssertUnwindSafe(|| handler(batch.as_slice())));
                        match result {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                poison.store(true, Ordering::Release);
                                let _ = error_tx.try_send(error);
                                break;
                            }
                            Err(error) => {
                                poison.store(true, Ordering::Release);
                                let _ = error_tx.try_send(format!(
                                    "CMM batch handler panicked: {}",
                                    panic_message(&*error)
                                ));
                                break;
                            }
                        }
                    }
                    let _ = ack.send(());
                }
            }));
        }

        let completion_handle = thread::spawn({
            let completion_job_rx: Receiver<CompletionJob> = completion_job_rx;
            let completion_tx = completion_tx.clone();
            move || {
                while let Ok(job) = completion_job_rx.recv() {
                    for _ in 0..worker_count {
                        let _ = job.ack.recv();
                    }
                    let records = match Arc::try_unwrap(job.batch) {
                        Ok(mut records) => {
                            records.clear();
                            records
                        }
                        Err(shared) => {
                            let mut records = Vec::with_capacity(shared.len());
                            records.clear();
                            records
                        }
                    };
                    let _ = completion_tx.send(records);
                }
            }
        });

        Self {
            work_txs,
            handles,
            worker_count,
            batch_pool,
            batch_return,
            inflight,
            batch_size,
            queue_depth,
            completion_rx,
            completion_job_tx: Some(completion_job_tx),
            completion_handle: Some(completion_handle),
            error_tx,
            error_rx,
            poison,
        }
    }

    #[cfg(test)]
    fn worker_count(&self) -> usize {
        self.worker_count
    }

    fn take_batch_vec(&self) -> Vec<CmmBatchRecord> {
        self.batch_pool
            .try_recv()
            .unwrap_or_else(|_| Vec::with_capacity(self.batch_size))
    }

    fn wait_for_capacity(&self) -> Result<(), String> {
        while self.inflight.load(Ordering::Acquire) >= self.queue_depth {
            self.drain_one_completion()?;
        }
        Ok(())
    }

    fn drain_one_completion(&self) -> Result<(), String> {
        let mut records = self
            .completion_rx
            .recv()
            .map_err(|error| error.to_string())?;
        records.clear();
        let _ = self.batch_return.try_send(records);
        self.inflight.fetch_sub(1, Ordering::Release);
        Ok(())
    }

    fn dispatch_batch_async(&self, records: Vec<CmmBatchRecord>) -> Result<(), String> {
        if self.poison.load(Ordering::Acquire) {
            return Err("CMM worker failed".to_string());
        }
        if records.is_empty() {
            return Ok(());
        }
        self.wait_for_capacity()?;

        let shared = Arc::new(records);
        let (ack_tx, ack_rx) = bounded(self.worker_count);
        for work_tx in &self.work_txs {
            work_tx
                .send(WorkerJob {
                    batch: Arc::clone(&shared),
                    ack: ack_tx.clone(),
                })
                .map_err(|error| {
                    self.poison.store(true, Ordering::Release);
                    error.to_string()
                })?;
        }
        drop(ack_tx);
        let completion_job_tx = self
            .completion_job_tx
            .as_ref()
            .ok_or_else(|| "CMM worker pool is shutting down".to_string())?;
        completion_job_tx
            .send(CompletionJob {
                batch: shared,
                ack: ack_rx,
            })
            .map_err(|error| {
                self.poison.store(true, Ordering::Release);
                error.to_string()
            })?;

        self.inflight.fetch_add(1, Ordering::Release);

        Ok(())
    }

    fn drain_all_completions(&self) -> Result<(), String> {
        while self.inflight.load(Ordering::Acquire) > 0 {
            self.drain_one_completion()?;
        }
        Ok(())
    }

    pub fn run_parallel_bam_pass(
        self,
        mut reader: bam::Reader,
        stop_after: u32,
        aligned_reads_only: bool,
        pf_reads_only: bool,
        include_secondary_quality_yield: bool,
        include_supplemental_quality_yield: bool,
    ) -> Result<(), String> {
        if self.worker_count == 0 {
            return Err("CMM worker pool has no handlers".to_string());
        }

        let pool = Arc::new(self);
        let worker_pool = Arc::clone(&pool);
        let worker_error_rx = pool.error_rx.clone();
        let (error_tx, error_rx) = bounded::<String>(1);
        let poison = Arc::clone(&pool.poison);

        let reader_handle = thread::spawn({
            let poison = Arc::clone(&poison);
            let worker_error_tx = pool.error_tx.clone();
            move || {
                let result = (|| -> Result<(), String> {
                    let mut batch = worker_pool.take_batch_vec();
                    let mut seen = 0usize;
                    for record in reader.records() {
                        if poison.load(Ordering::Relaxed) {
                            break;
                        }
                        if stop_after > 0 {
                            if seen >= stop_after as usize {
                                break;
                            }
                            seen += 1;
                        }
                        let record = record.map_err(|error| error.to_string())?;
                        let gates = CmmRecordGates::for_record(
                            &record,
                            aligned_reads_only,
                            pf_reads_only,
                            include_secondary_quality_yield,
                            include_supplemental_quality_yield,
                        );
                        if gates.has_any() {
                            batch.push(CmmBatchRecord { record, gates });
                        }
                        if batch.len() >= worker_pool.batch_size {
                            let full_batch =
                                std::mem::replace(&mut batch, worker_pool.take_batch_vec());
                            worker_pool.dispatch_batch_async(full_batch)?;
                        }
                    }
                    if !batch.is_empty() && !poison.load(Ordering::Relaxed) {
                        worker_pool.dispatch_batch_async(batch)?;
                    }
                    worker_pool.drain_all_completions()
                })();
                if let Err(error) = result {
                    poison.store(true, Ordering::Relaxed);
                    let _ = error_tx.send(error.clone());
                    let _ = worker_error_tx.try_send(error);
                }
            }
        });

        reader_handle
            .join()
            .map_err(|_| "CMM reader thread panicked".to_string())?;
        if let Ok(error) = error_rx.try_recv() {
            return Err(error);
        }
        if let Ok(error) = worker_error_rx.try_recv() {
            return Err(error);
        }
        drop(pool);
        Ok(())
    }
}

fn panic_message(error: &dyn Any) -> String {
    if let Some(message) = error.downcast_ref::<&'static str>() {
        message.to_string()
    } else if let Some(message) = error.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn worker_groups_for_handlers(
    handlers: Vec<CmmBatchHandler>,
    worker_cap: usize,
) -> Vec<CmmWorkerGroup> {
    if handlers.is_empty() {
        return Vec::new();
    }
    let worker_count = worker_cap.max(1).min(handlers.len());
    let mut groups = (0..worker_count).map(|_| Vec::new()).collect::<Vec<_>>();
    for (index, handler) in handlers.into_iter().enumerate() {
        groups[index % worker_count].push(handler);
    }
    groups
}

fn cmm_batch_size() -> usize {
    positive_env_usize("TURBO_PICARD_CMM_BATCH_SIZE").unwrap_or(DEFAULT_BATCH_SIZE)
}

fn cmm_queue_depth() -> usize {
    positive_env_usize("TURBO_PICARD_CMM_QUEUE_DEPTH").unwrap_or(DEFAULT_BATCH_POOL_DEPTH)
}

fn positive_env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
}

impl Drop for CmmWorkerPool {
    fn drop(&mut self) {
        self.work_txs.clear();
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
        let completion_job_tx = self.completion_job_tx.take();
        drop(completion_job_tx);
        if let Some(handle) = self.completion_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Overlap BGZF decode (reader thread) with record processing (consumer thread).
pub fn pipeline_bam_records<F>(
    mut reader: bam::Reader,
    stop_after: u32,
    channel_depth: usize,
    mut consumer: F,
) -> Result<(), String>
where
    F: FnMut(bam::Record) -> Result<(), String>,
{
    let (record_tx, record_rx) = bounded::<Result<bam::Record, String>>(channel_depth);
    let (error_tx, error_rx) = bounded::<String>(1);
    let (stop_tx, stop_rx) = bounded::<()>(1);

    let mut consumer_error: Option<String> = None;

    let reader_handle = thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let mut seen = 0usize;
            for record in reader.records() {
                if stop_after > 0 {
                    if seen >= stop_after as usize {
                        break;
                    }
                    seen += 1;
                }
                let record = record.map_err(|error| error.to_string())?;
                crossbeam_channel::select! {
                    recv(stop_rx) -> _ => return Ok(()),
                    send(record_tx, Ok(record)) -> result => {
                        result.map_err(|error| error.to_string())?;
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = error_tx.send(error);
        }
    });

    while let Ok(record) = record_rx.recv() {
        let record = match record {
            Ok(record) => record,
            Err(error) => {
                consumer_error = Some(error);
                break;
            }
        };

        let consumer_result = panic::catch_unwind(AssertUnwindSafe(|| consumer(record)));
        match consumer_result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                consumer_error = Some(error);
                break;
            }
            Err(panic) => {
                consumer_error = Some(format!("CMM consumer panicked: {}", panic_message(&*panic)));
                break;
            }
        }
        if let Ok(error) = error_rx.try_recv() {
            if consumer_error.is_none() {
                consumer_error = Some(error);
            }
            break;
        }
    }

    let _ = stop_tx.send(());

    reader_handle
        .join()
        .map_err(|_| "BAM reader thread panicked".to_string())?;
    if let Some(error) = consumer_error {
        return Err(error);
    }
    if let Ok(error) = error_rx.try_recv() {
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn record_with_flags(flags: u16) -> bam::Record {
        let mut record = bam::Record::new();
        record.set(b"read", None, b"ACGT", b"FFFF");
        record.set_flags(flags);
        record
    }

    #[test]
    fn cmm_record_gates_filter_quality_metrics_with_read_filters() {
        let aligned_pf_accepted = record_with_flags(0);
        let gates = CmmRecordGates::for_record(&aligned_pf_accepted, true, true, false, false);
        assert!(gates.base_distribution);
        assert!(gates.quality_distribution);
        assert!(gates.mean_quality);
        assert!(gates.quality_yield);

        let unmapped = record_with_flags(4);
        let unmapped_gates = CmmRecordGates::for_record(&unmapped, true, true, false, false);
        assert!(!unmapped_gates.base_distribution);
        assert!(!unmapped_gates.quality_distribution);
        assert!(!unmapped_gates.mean_quality);
        assert!(!unmapped_gates.gc_bias);
        assert!(!unmapped_gates.wgs);

        let qc_failed = record_with_flags(0x200);
        let qc_failed_pf_gates = CmmRecordGates::for_record(&qc_failed, true, true, false, false);
        assert!(!qc_failed_pf_gates.base_distribution);
        assert!(!qc_failed_pf_gates.quality_distribution);
        assert!(!qc_failed_pf_gates.mean_quality);

        let secondary = record_with_flags(0x100);
        let secondary_gates = CmmRecordGates::for_record(&secondary, false, false, false, false);
        assert!(!secondary_gates.quality_yield);
        assert!(!secondary_gates.alignment);

        let secondary_included_yield =
            CmmRecordGates::for_record(&secondary, false, false, true, false);
        assert!(secondary_included_yield.quality_yield);

        let secondary_excluded_yield =
            CmmRecordGates::for_record(&secondary, false, false, false, false);
        assert!(!secondary_excluded_yield.quality_yield);
    }

    #[test]
    fn cmm_record_gates_filter_supplementary_reads_for_quality_yield() {
        let supplemental = record_with_flags(0x800);
        let supplemental_gates =
            CmmRecordGates::for_record(&supplemental, false, false, false, true);
        assert!(supplemental_gates.quality_yield);

        let supplemental_without_flag =
            CmmRecordGates::for_record(&supplemental, false, false, false, false);
        assert!(!supplemental_without_flag.quality_yield);
    }

    #[test]
    fn cmm_record_gates_has_any() {
        let empty = CmmRecordGates::default();
        assert!(!empty.has_any());

        let some = CmmRecordGates {
            alignment: true,
            ..CmmRecordGates::default()
        };
        assert!(some.has_any());
    }

    fn write_basic_sam(path: &std::path::Path, lines: &str) {
        let mut content = String::from("@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chr1\tLN:1000\n");
        content.push_str(lines);
        fs::write(path, content).expect("fixture SAM is written");
    }

    #[test]
    fn cmm_worker_pool_forwards_handler_errors_as_run_errors() {
        let tempdir = tempdir().expect("tempdir exists");
        let input = tempdir.path().join("input.sam");
        write_basic_sam(&input, "mapped\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n");
        let reader = bam::Reader::from_path(&input).expect("sam input opens");

        let pool = CmmWorkerPool::new(vec![Box::new(|_| Err("handler failure".to_string()))], 1);

        let err = pool
            .run_parallel_bam_pass(reader, 0, false, false, false, false)
            .unwrap_err();
        assert_eq!(err, "handler failure");
    }

    #[test]
    fn cmm_worker_pool_forwards_handler_panics_as_run_errors() {
        let tempdir = tempdir().expect("tempdir exists");
        let input = tempdir.path().join("input.sam");
        write_basic_sam(&input, "mapped\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n");
        let reader = bam::Reader::from_path(&input).expect("sam input opens");

        let pool = CmmWorkerPool::new(
            vec![Box::new(|_| {
                panic!("simulated handler panic");
            })],
            1,
        );

        let err = pool
            .run_parallel_bam_pass(reader, 0, false, false, false, false)
            .unwrap_err();
        assert!(
            err.contains("CMM batch handler panicked"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn cmm_run_parallel_bam_pass_skips_records_with_no_matching_gates() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let tempdir = tempdir().expect("tempdir exists");
        let input = tempdir.path().join("input.sam");
        write_basic_sam(
            &input,
            "mapped\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\nsecondary\t256\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        );
        let reader = bam::Reader::from_path(&input).expect("sam input opens");

        let observed = Arc::new(AtomicUsize::new(0));
        let observed_ref = Arc::clone(&observed);
        let pool = CmmWorkerPool::new(
            vec![Box::new(move |batch| {
                observed_ref.fetch_add(batch.len(), Ordering::Relaxed);
                Ok(())
            })],
            1,
        );

        pool.run_parallel_bam_pass(reader, 0, false, false, false, false)
            .expect("cmm parallel run succeeds");

        assert_eq!(observed.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn pipeline_bam_records_forwards_consumer_error_after_joining_reader() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let tempdir = tempdir().expect("tempdir exists");
        let input = tempdir.path().join("input.sam");
        write_basic_sam(
            &input,
            "mapped\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\nmapped2\t0\tchr1\t2\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
        );
        let reader = bam::Reader::from_path(&input).expect("sam input opens");

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_ref = Arc::clone(&calls);

        let result = pipeline_bam_records(reader, 0, 1, move |_record| {
            let call = calls_ref.fetch_add(1, Ordering::Relaxed) + 1;
            if call == 1 {
                return Err("consumer failed".to_string());
            }
            Ok(())
        });

        let error = result.expect_err("expected consumer error");
        assert_eq!(error, "consumer failed");
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn pipeline_bam_records_terminates_when_consumer_fails_with_small_channel() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let tempdir = tempdir().expect("tempdir exists");
        let input = tempdir.path().join("input.sam");
        let mut lines = String::new();
        for index in 0..500 {
            lines.push_str(&format!(
                "mapped_{index}\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n"
            ));
        }
        write_basic_sam(&input, &lines);

        let reader = bam::Reader::from_path(&input).expect("sam input opens");
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_ref = Arc::clone(&calls);

        let result = pipeline_bam_records(reader, 0, 1, move |_record| {
            let call = calls_ref.fetch_add(1, Ordering::Relaxed) + 1;
            if call == 1 {
                return Err("consumer failed".to_string());
            }
            Ok(())
        });

        let error = result.expect_err("expected consumer error");
        assert_eq!(error, "consumer failed");
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn pipeline_bam_records_forwards_consumer_panic_as_error() {
        use std::path::Path;

        let tempdir = tempdir().expect("tempdir exists");
        let input = tempdir.path().join("input.sam");
        let path = Path::new(&input);
        write_basic_sam(path, "mapped\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n");
        let reader = bam::Reader::from_path(path).expect("sam input opens");

        let result = pipeline_bam_records(reader, 0, 1, |_record| -> Result<(), String> {
            panic!("consumer panic");
        });

        let error = result.expect_err("expected consumer panic error");
        assert!(error.contains("CMM consumer panicked"));
    }

    #[test]
    fn pipeline_bam_records_obeys_stop_after_limit() {
        use std::path::Path;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let tempdir = tempdir().expect("tempdir exists");
        let input = tempdir.path().join("input.sam");
        let mut lines = String::new();
        for index in 0..10 {
            lines.push_str(&format!(
                "mapped_{index}\t0\tchr1\t{}\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
                index + 1
            ));
        }
        write_basic_sam(&input, &lines);
        let reader = bam::Reader::from_path(Path::new(&input)).expect("sam input opens");

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_ref = Arc::clone(&calls);

        pipeline_bam_records(reader, 3, 2, move |_record| {
            calls_ref.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
        .expect("pipeline stops cleanly");

        assert_eq!(calls.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn cmm_worker_pool_requires_at_least_one_handler() {
        let pool = CmmWorkerPool::new(vec![], 1);
        let tempdir = tempdir().expect("tempdir exists");
        let input = tempdir.path().join("empty.sam");
        std::fs::write(
            &input,
            "@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chr1\tLN:1000\n",
        )
        .expect("empty fixture is written");
        let reader = bam::Reader::from_path(&input).expect("sam input opens");

        let err = pool
            .run_parallel_bam_pass(reader, 0, false, false, false, false)
            .expect_err("expected no-handler error");
        assert_eq!(err, "CMM worker pool has no handlers");
    }

    #[test]
    fn cmm_worker_pool_caps_workers_below_handler_count() {
        fn ok_handler(_: &[CmmBatchRecord]) -> Result<(), String> {
            Ok(())
        }

        let handlers: Vec<CmmBatchHandler> = (0..5)
            .map(|_| Box::new(ok_handler) as CmmBatchHandler)
            .collect();
        let pool = CmmWorkerPool::new(handlers, 2);
        assert_eq!(pool.worker_count(), 2);
    }

    #[test]
    fn cmm_worker_pool_one_thread_cap_creates_one_worker() {
        fn ok_handler(_: &[CmmBatchRecord]) -> Result<(), String> {
            Ok(())
        }

        let handlers: Vec<CmmBatchHandler> = (0..4)
            .map(|_| Box::new(ok_handler) as CmmBatchHandler)
            .collect();
        let pool = CmmWorkerPool::new(handlers, 1);
        assert_eq!(pool.worker_count(), 1);
    }

    #[test]
    fn cmm_worker_pool_reuses_batch_vectors() {
        use std::collections::HashSet;
        use std::sync::{Arc, Mutex};

        let tempdir = tempdir().expect("tempdir exists");
        let input = tempdir.path().join("input.sam");
        let mut lines = String::new();
        for index in 0..12 {
            lines.push_str(&format!(
                "mapped_{index}\t0\tchr1\t{}\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
                index + 1
            ));
        }
        write_basic_sam(&input, &lines);
        let reader = bam::Reader::from_path(&input).expect("sam input opens");

        let seen_batches = Arc::new(Mutex::new(Vec::<usize>::new()));
        let seen_batches_ref = Arc::clone(&seen_batches);
        let pool = CmmWorkerPool::with_options(
            vec![Box::new(move |batch| {
                seen_batches_ref
                    .lock()
                    .expect("batch pointer lock")
                    .push(batch.as_ptr() as usize);
                Ok(())
            })],
            CmmWorkerPoolOptions {
                worker_cap: 1,
                batch_size: 1,
                queue_depth: 1,
            },
        );

        pool.run_parallel_bam_pass(reader, 0, false, false, false, false)
            .expect("cmm parallel run succeeds");

        let seen_batches = seen_batches.lock().expect("batch pointer lock");
        let unique_batches = seen_batches.iter().copied().collect::<HashSet<_>>();
        assert!(
            unique_batches.len() < seen_batches.len(),
            "expected at least one reused batch allocation, observed {seen_batches:?}"
        );
    }

    #[test]
    fn pipeline_bam_records_propagates_reader_record_error() {
        let tempdir = tempdir().expect("tempdir exists");
        let input = tempdir.path().join("invalid.sam");
        write_basic_sam(
            &input,
            "mapped\t0\tchr1\t1\t60\tINVALID_CIGAR\t*\t0\t0\tACGT\tFFFF\n",
        );
        let reader = bam::Reader::from_path(&input).expect("sam input opens");

        let result = pipeline_bam_records(reader, 0, 2, |_record| Ok(()));
        assert!(result.is_err());
    }
}
