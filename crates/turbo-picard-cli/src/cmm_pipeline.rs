use crossbeam_channel::{Receiver, Sender, bounded};
use rust_htslib::bam::{self, Read};
use std::any::Any;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CmmPipelineError {
    #[error("{0}")]
    Handler(String),
    #[error("{0}")]
    Bam(String),
    #[error("{0}")]
    Channel(String),
    #[error("CMM batch handler panicked: {0}")]
    HandlerPanicked(String),
    #[error("CMM consumer panicked: {0}")]
    ConsumerPanicked(String),
    #[error("CMM worker failed")]
    WorkerFailed,
    #[error("CMM worker pool is shutting down")]
    ShuttingDown,
    #[error("CMM worker pool has no handlers")]
    NoHandlers,
    #[error("CMM reader thread panicked")]
    ReaderPanicked,
    #[error("BAM reader thread panicked")]
    BamReaderPanicked,
}

impl From<CmmPipelineError> for String {
    fn from(error: CmmPipelineError) -> Self {
        error.to_string()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per collector family is constructed field-by-field by the command runners"
)]
pub struct CmmPlan {
    pub alignment: bool,
    pub insert_size: bool,
    pub base_distribution: bool,
    pub quality_distribution: bool,
    pub mean_quality: bool,
    pub quality_yield: bool,
    pub gc_bias: bool,
    pub wgs: bool,
}

impl CmmPlan {
    #[cfg(test)]
    pub const ALL: Self = Self {
        alignment: true,
        insert_size: true,
        base_distribution: true,
        quality_distribution: true,
        mean_quality: true,
        quality_yield: true,
        gc_bias: true,
        wgs: true,
    };

    #[inline]
    fn needs_quality_scan(self) -> bool {
        self.base_distribution || self.quality_distribution || self.mean_quality
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one gate per collector family is read field-by-field by the batch handlers"
)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualityReadFilter {
    pub aligned_reads_only: bool,
    pub pf_reads_only: bool,
}

impl QualityReadFilter {
    pub const fn new(aligned_reads_only: bool, pf_reads_only: bool) -> Self {
        Self {
            aligned_reads_only,
            pf_reads_only,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualityYieldScope {
    pub include_secondary: bool,
    pub include_supplemental: bool,
}

impl QualityYieldScope {
    pub const fn new(include_secondary: bool, include_supplemental: bool) -> Self {
        Self {
            include_secondary,
            include_supplemental,
        }
    }
}

impl CmmRecordGates {
    #[cfg(test)]
    #[inline]
    pub fn for_record(
        record: &bam::Record,
        filter: QualityReadFilter,
        yield_scope: QualityYieldScope,
    ) -> Self {
        Self::for_plan(record, CmmPlan::ALL, filter, yield_scope)
    }

    #[inline]
    pub fn for_plan(
        record: &bam::Record,
        plan: CmmPlan,
        filter: QualityReadFilter,
        yield_scope: QualityYieldScope,
    ) -> Self {
        let secondary = record.is_secondary();
        let supplementary = record.is_supplementary();
        let unmapped = record.is_unmapped();
        let captures_quality = plan.needs_quality_scan()
            && !super::skip_quality_metric_record(
                record,
                filter.aligned_reads_only,
                filter.pf_reads_only,
            );
        Self {
            alignment: plan.alignment && !secondary && !supplementary,
            insert_size: plan.insert_size
                && record.is_paired()
                && !unmapped
                && !record.is_mate_unmapped()
                && !secondary
                && !supplementary
                && record.is_last_in_template()
                && record.insert_size() != 0,
            base_distribution: plan.base_distribution && captures_quality,
            quality_distribution: plan.quality_distribution && captures_quality,
            mean_quality: plan.mean_quality && captures_quality,
            quality_yield: plan.quality_yield
                && (!secondary || yield_scope.include_secondary)
                && (!supplementary || yield_scope.include_supplemental),
            gc_bias: plan.gc_bias && !secondary && !supplementary && !unmapped,
            wgs: plan.wgs && !unmapped && !secondary && !supplementary,
        }
    }

    #[inline]
    fn has_any(self) -> bool {
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

pub const BATCH_POOL_DEPTH: usize = 16;

pub struct CmmBatchRecord {
    pub record: bam::Record,
    pub gates: CmmRecordGates,
}

pub type CmmBatchHandler = Box<dyn Fn(&[CmmBatchRecord]) -> Result<(), String> + Send>;

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
    completion_rx: Receiver<Vec<CmmBatchRecord>>,
    completion_job_tx: Option<Sender<CompletionJob>>,
    completion_handle: Option<JoinHandle<()>>,
    error_tx: Sender<CmmPipelineError>,
    error_rx: Receiver<CmmPipelineError>,
    poison: Arc<AtomicBool>,
    plan: CmmPlan,
}

impl CmmWorkerPool {
    #[cfg(test)]
    pub fn new(handlers: Vec<CmmBatchHandler>) -> Self {
        Self::with_plan(handlers, CmmPlan::ALL)
    }

    pub fn with_plan(handlers: Vec<CmmBatchHandler>, plan: CmmPlan) -> Self {
        let worker_count = handlers.len();
        let (batch_return, batch_pool) = bounded(BATCH_POOL_DEPTH);
        let (completion_tx, completion_rx) = crossbeam_channel::unbounded::<Vec<CmmBatchRecord>>();
        let (error_tx, error_rx) = bounded(1);
        let inflight = Arc::new(AtomicUsize::new(0));
        let poison = Arc::new(AtomicBool::new(false));
        let mut work_txs = Vec::with_capacity(worker_count);
        let mut handles = Vec::with_capacity(worker_count);
        let (completion_job_tx, completion_job_rx) = bounded(BATCH_POOL_DEPTH);

        for handler in handlers {
            let (work_tx, work_rx) = bounded::<WorkerJob>(BATCH_POOL_DEPTH);
            let error_tx = error_tx.clone();
            let poison = Arc::clone(&poison);
            work_txs.push(work_tx);
            handles.push(thread::spawn(move || {
                while let Ok(WorkerJob { batch, ack }) = work_rx.recv() {
                    let result =
                        panic::catch_unwind(AssertUnwindSafe(|| handler(batch.as_slice())));
                    match result {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            poison.store(true, Ordering::Release);
                            let _ = error_tx.try_send(CmmPipelineError::Handler(error));
                        }
                        Err(error) => {
                            poison.store(true, Ordering::Release);
                            let _ = error_tx.try_send(CmmPipelineError::HandlerPanicked(
                                panic_message(&*error),
                            ));
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
            completion_rx,
            completion_job_tx: Some(completion_job_tx),
            completion_handle: Some(completion_handle),
            error_tx,
            error_rx,
            poison,
            plan,
        }
    }

    fn take_batch_vec(&self) -> Vec<CmmBatchRecord> {
        self.batch_pool
            .try_recv()
            .unwrap_or_else(|_| Vec::with_capacity(super::CMM_BATCH_SIZE))
    }

    fn wait_for_capacity(&self) -> Result<(), CmmPipelineError> {
        while self.inflight.load(Ordering::Acquire) >= BATCH_POOL_DEPTH {
            self.drain_one_completion()?;
        }
        Ok(())
    }

    fn drain_one_completion(&self) -> Result<(), CmmPipelineError> {
        let mut records = self
            .completion_rx
            .recv()
            .map_err(|error| CmmPipelineError::Channel(error.to_string()))?;
        records.clear();
        let _ = self.batch_return.try_send(records);
        self.inflight.fetch_sub(1, Ordering::Release);
        Ok(())
    }

    fn dispatch_batch_async(&self, records: Vec<CmmBatchRecord>) -> Result<(), CmmPipelineError> {
        if self.poison.load(Ordering::Acquire) {
            return Err(CmmPipelineError::WorkerFailed);
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
                    CmmPipelineError::Channel(error.to_string())
                })?;
        }
        drop(ack_tx);
        let completion_job_tx = self
            .completion_job_tx
            .as_ref()
            .ok_or(CmmPipelineError::ShuttingDown)?;
        completion_job_tx
            .send(CompletionJob {
                batch: shared,
                ack: ack_rx,
            })
            .map_err(|error| {
                self.poison.store(true, Ordering::Release);
                CmmPipelineError::Channel(error.to_string())
            })?;

        self.inflight.fetch_add(1, Ordering::Release);

        Ok(())
    }

    fn drain_all_completions(&self) -> Result<(), CmmPipelineError> {
        while self.inflight.load(Ordering::Acquire) > 0 {
            self.drain_one_completion()?;
        }
        Ok(())
    }

    #[expect(
        clippy::fn_params_excessive_bools,
        reason = "the positional signature is shared with the command runners that call it"
    )]
    pub fn run_parallel_bam_pass(
        self,
        mut reader: bam::Reader,
        stop_after: u32,
        aligned_reads_only: bool,
        pf_reads_only: bool,
        include_secondary_quality_yield: bool,
        include_supplemental_quality_yield: bool,
    ) -> Result<(), CmmPipelineError> {
        if self.worker_count == 0 {
            return Err(CmmPipelineError::NoHandlers);
        }

        let pool = Arc::new(self);
        let worker_pool = Arc::clone(&pool);
        let worker_error_rx = pool.error_rx.clone();
        let (error_tx, error_rx) = bounded::<CmmPipelineError>(1);
        let poison = Arc::clone(&pool.poison);
        let filter = QualityReadFilter::new(aligned_reads_only, pf_reads_only);
        let yield_scope = QualityYieldScope::new(
            include_secondary_quality_yield,
            include_supplemental_quality_yield,
        );

        let reader_handle = thread::spawn({
            let poison = Arc::clone(&poison);
            let worker_error_tx = pool.error_tx.clone();
            move || {
                let result = (|| -> Result<(), CmmPipelineError> {
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
                        let record =
                            record.map_err(|error| CmmPipelineError::Bam(error.to_string()))?;
                        let gates = CmmRecordGates::for_plan(
                            &record,
                            worker_pool.plan,
                            filter,
                            yield_scope,
                        );
                        if gates.has_any() {
                            batch.push(CmmBatchRecord { record, gates });
                        }
                        if batch.len() >= super::CMM_BATCH_SIZE {
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
            .map_err(|_| CmmPipelineError::ReaderPanicked)?;
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

pub fn pipeline_bam_records<F>(
    mut reader: bam::Reader,
    stop_after: u32,
    channel_depth: usize,
    mut consumer: F,
) -> Result<(), CmmPipelineError>
where
    F: FnMut(bam::Record) -> Result<(), String>,
{
    let (record_tx, record_rx) = bounded::<Result<bam::Record, CmmPipelineError>>(channel_depth);
    let (error_tx, error_rx) = bounded::<CmmPipelineError>(1);
    let (stop_tx, stop_rx) = bounded::<()>(1);

    let mut consumer_error: Option<CmmPipelineError> = None;

    let reader_handle = thread::spawn(move || {
        let result = (|| -> Result<(), CmmPipelineError> {
            let mut seen = 0usize;
            for record in reader.records() {
                if stop_after > 0 {
                    if seen >= stop_after as usize {
                        break;
                    }
                    seen += 1;
                }
                let record = record.map_err(|error| CmmPipelineError::Bam(error.to_string()))?;
                crossbeam_channel::select! {
                    recv(stop_rx) -> _ => return Ok(()),
                    send(record_tx, Ok(record)) -> result => {
                        result.map_err(|error| CmmPipelineError::Channel(error.to_string()))?;
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
                consumer_error = Some(CmmPipelineError::Handler(error));
                break;
            }
            Err(panic) => {
                consumer_error = Some(CmmPipelineError::ConsumerPanicked(panic_message(&*panic)));
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
        .map_err(|_| CmmPipelineError::BamReaderPanicked)?;
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
    use std::fmt::Write as _;
    use std::fs;
    use tempfile::tempdir;

    fn record_with_flags(flags: u16) -> bam::Record {
        let mut record = bam::Record::new();
        record.set(b"read", None, b"ACGT", b"FFFF");
        record.set_flags(flags);
        record
    }

    #[test]
    fn cmm_plan_keeps_all_gates_identical_to_the_unplanned_path() {
        for flags in [
            0_u16,
            0x1 | 0x2 | 0x40,
            0x4,
            0x200,
            0x100 | 0x800,
            0x1 | 0x8 | 0x80,
        ] {
            let record = record_with_flags(flags);
            assert_eq!(
                CmmRecordGates::for_plan(
                    &record,
                    CmmPlan::ALL,
                    QualityReadFilter::new(true, true),
                    QualityYieldScope::new(false, false)
                ),
                CmmRecordGates::for_record(
                    &record,
                    QualityReadFilter::new(true, true),
                    QualityYieldScope::new(false, false)
                ),
                "flags={flags:#x}"
            );
        }
    }

    #[test]
    fn cmm_plan_skips_gate_families_no_collector_reads() {
        let paired = record_with_flags(0x1 | 0x2 | 0x40);
        let alignment_only = CmmRecordGates::for_plan(
            &paired,
            CmmPlan {
                alignment: true,
                ..CmmPlan::default()
            },
            QualityReadFilter::new(true, true),
            QualityYieldScope::new(false, false),
        );
        assert!(alignment_only.alignment);
        assert!(!alignment_only.insert_size);
        assert!(!alignment_only.base_distribution);
        assert!(!alignment_only.quality_distribution);
        assert!(!alignment_only.mean_quality);
        assert!(!alignment_only.quality_yield);
        assert!(!alignment_only.gc_bias);
        assert!(!alignment_only.wgs);

        let unplanned = CmmRecordGates::for_plan(
            &paired,
            CmmPlan::default(),
            QualityReadFilter::new(true, true),
            QualityYieldScope::new(false, false),
        );
        assert!(!unplanned.has_any());
    }

    #[test]
    fn cmm_record_gates_filter_quality_metrics_with_read_filters() {
        let aligned_pf_accepted = record_with_flags(0);
        let gates = CmmRecordGates::for_record(
            &aligned_pf_accepted,
            QualityReadFilter::new(true, true),
            QualityYieldScope::new(false, false),
        );
        assert!(gates.base_distribution);
        assert!(gates.quality_distribution);
        assert!(gates.mean_quality);
        assert!(gates.quality_yield);

        let unmapped = record_with_flags(4);
        let unmapped_gates = CmmRecordGates::for_record(
            &unmapped,
            QualityReadFilter::new(true, true),
            QualityYieldScope::new(false, false),
        );
        assert!(!unmapped_gates.base_distribution);
        assert!(!unmapped_gates.quality_distribution);
        assert!(!unmapped_gates.mean_quality);
        assert!(!unmapped_gates.gc_bias);
        assert!(!unmapped_gates.wgs);

        let qc_failed = record_with_flags(0x200);
        let qc_failed_pf_gates = CmmRecordGates::for_record(
            &qc_failed,
            QualityReadFilter::new(true, true),
            QualityYieldScope::new(false, false),
        );
        assert!(!qc_failed_pf_gates.base_distribution);
        assert!(!qc_failed_pf_gates.quality_distribution);
        assert!(!qc_failed_pf_gates.mean_quality);

        let secondary = record_with_flags(0x100);
        let secondary_gates = CmmRecordGates::for_record(
            &secondary,
            QualityReadFilter::new(false, false),
            QualityYieldScope::new(false, false),
        );
        assert!(!secondary_gates.quality_yield);
        assert!(!secondary_gates.alignment);

        let secondary_included_yield = CmmRecordGates::for_record(
            &secondary,
            QualityReadFilter::new(false, false),
            QualityYieldScope::new(true, false),
        );
        assert!(secondary_included_yield.quality_yield);

        let secondary_excluded_yield = CmmRecordGates::for_record(
            &secondary,
            QualityReadFilter::new(false, false),
            QualityYieldScope::new(false, false),
        );
        assert!(!secondary_excluded_yield.quality_yield);
    }

    #[test]
    fn cmm_record_gates_filter_supplementary_reads_for_quality_yield() {
        let supplemental = record_with_flags(0x800);
        let supplemental_gates = CmmRecordGates::for_record(
            &supplemental,
            QualityReadFilter::new(false, false),
            QualityYieldScope::new(false, true),
        );
        assert!(supplemental_gates.quality_yield);

        let supplemental_without_flag = CmmRecordGates::for_record(
            &supplemental,
            QualityReadFilter::new(false, false),
            QualityYieldScope::new(false, false),
        );
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

        let pool = CmmWorkerPool::new(vec![Box::new(|_| Err("handler failure".to_string()))]);

        let err = pool
            .run_parallel_bam_pass(reader, 0, false, false, false, false)
            .unwrap_err();
        assert_eq!(err.to_string(), "handler failure");
    }

    #[test]
    fn cmm_worker_pool_forwards_handler_panics_as_run_errors() {
        let tempdir = tempdir().expect("tempdir exists");
        let input = tempdir.path().join("input.sam");
        write_basic_sam(&input, "mapped\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n");
        let reader = bam::Reader::from_path(&input).expect("sam input opens");

        let pool = CmmWorkerPool::new(vec![Box::new(|_| {
            panic!("simulated handler panic");
        })]);

        let err = pool
            .run_parallel_bam_pass(reader, 0, false, false, false, false)
            .unwrap_err();
        assert!(
            err.to_string().contains("CMM batch handler panicked"),
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
        let pool = CmmWorkerPool::new(vec![Box::new(move |batch| {
            observed_ref.fetch_add(batch.len(), Ordering::Relaxed);
            Ok(())
        })]);

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
        assert_eq!(error.to_string(), "consumer failed");
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
            writeln!(
                lines,
                "mapped_{index}\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF"
            )
            .unwrap();
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
        assert_eq!(error.to_string(), "consumer failed");
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
        assert!(error.to_string().contains("CMM consumer panicked"));
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
            writeln!(
                lines,
                "mapped_{index}\t0\tchr1\t{}\t60\t4M\t*\t0\t0\tACGT\tFFFF",
                index + 1
            )
            .unwrap();
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
        let pool = CmmWorkerPool::new(vec![]);
        let tempdir = tempdir().expect("tempdir exists");
        let input = tempdir.path().join("empty.sam");
        fs::write(
            &input,
            "@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chr1\tLN:1000\n",
        )
        .expect("empty fixture is written");
        let reader = bam::Reader::from_path(&input).expect("sam input opens");

        let err = pool
            .run_parallel_bam_pass(reader, 0, false, false, false, false)
            .expect_err("expected no-handler error");
        assert_eq!(err, CmmPipelineError::NoHandlers);
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
