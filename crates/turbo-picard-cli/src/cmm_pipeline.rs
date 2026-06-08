use crossbeam_channel::{bounded, Receiver, Sender};
use rust_htslib::bam::{self, Read};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
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
        Self {
            alignment: !secondary && !supplementary,
            insert_size: record.is_paired()
                && !unmapped
                && !record.is_mate_unmapped()
                && !secondary
                && !supplementary
                && record.is_last_in_template()
                && record.insert_size() != 0,
            base_distribution: !super::skip_quality_metric_record(record, aligned_reads_only, pf_reads_only),
            quality_distribution: !super::skip_quality_metric_record(record, aligned_reads_only, pf_reads_only),
            mean_quality: !super::skip_quality_metric_record(record, aligned_reads_only, pf_reads_only),
            quality_yield: (!secondary || include_secondary_yield)
                && (!supplementary || include_supplemental_yield),
            gc_bias: !secondary && !supplementary && !unmapped,
            wgs: !unmapped && !secondary && !supplementary,
        }
    }
}

pub const BATCH_POOL_DEPTH: usize = 16;

pub struct CmmBatchRecord {
    pub record: bam::Record,
    pub gates: CmmRecordGates,
}

pub type CmmBatchHandler = Box<dyn Fn(&[CmmBatchRecord]) + Send>;

struct WorkerJob {
    batch: Arc<Vec<CmmBatchRecord>>,
    ack: Sender<()>,
}

pub struct CmmWorkerPool {
    work_txs: Vec<Sender<WorkerJob>>,
    shutdown_txs: Vec<Sender<()>>,
    handles: Vec<JoinHandle<()>>,
    worker_count: usize,
    batch_pool: Receiver<Vec<CmmBatchRecord>>,
    batch_return: Sender<Vec<CmmBatchRecord>>,
    inflight: Arc<AtomicUsize>,
    completion_rx: Receiver<Vec<CmmBatchRecord>>,
    completion_tx: Sender<Vec<CmmBatchRecord>>,
}

impl CmmWorkerPool {
    pub fn new(handlers: Vec<CmmBatchHandler>) -> Self {
        let worker_count = handlers.len();
        let (batch_return, batch_pool) = bounded(BATCH_POOL_DEPTH);
        let (completion_tx, completion_rx) = bounded(BATCH_POOL_DEPTH);
        let inflight = Arc::new(AtomicUsize::new(0));
        let mut work_txs = Vec::with_capacity(worker_count);
        let mut shutdown_txs = Vec::with_capacity(worker_count);
        let mut handles = Vec::with_capacity(worker_count);

        for handler in handlers {
            let (work_tx, work_rx) = bounded::<WorkerJob>(BATCH_POOL_DEPTH);
            let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
            work_txs.push(work_tx);
            shutdown_txs.push(shutdown_tx);
            handles.push(thread::spawn(move || {
                loop {
                    crossbeam_channel::select! {
                        recv(shutdown_rx) -> _ => break,
                        recv(work_rx) -> job => {
                            let Ok(WorkerJob { batch, ack }) = job else {
                                break;
                            };
                            handler(batch.as_slice());
                            let _ = ack.send(());
                        }
                    }
                }
            }));
        }

        Self {
            work_txs,
            shutdown_txs,
            handles,
            worker_count,
            batch_pool,
            batch_return,
            inflight,
            completion_rx,
            completion_tx,
        }
    }

    fn take_batch_vec(&self) -> Vec<CmmBatchRecord> {
        self.batch_pool
            .try_recv()
            .unwrap_or_else(|_| Vec::with_capacity(super::CMM_BATCH_SIZE))
    }

    fn wait_for_capacity(&self) -> Result<(), String> {
        while self.inflight.load(Ordering::Acquire) >= BATCH_POOL_DEPTH {
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
        if records.is_empty() {
            return Ok(());
        }
        self.wait_for_capacity()?;
        self.inflight.fetch_add(1, Ordering::Release);

        let shared = Arc::new(records);
        let (ack_tx, ack_rx) = bounded(self.worker_count);
        for work_tx in &self.work_txs {
            work_tx
                .send(WorkerJob {
                    batch: Arc::clone(&shared),
                    ack: ack_tx.clone(),
                })
                .map_err(|error| error.to_string())?;
        }
        drop(ack_tx);

        let completion_tx = self.completion_tx.clone();
        let worker_count = self.worker_count;
        thread::spawn(move || {
            for _ in 0..worker_count {
                let _ = ack_rx.recv();
            }
            let records = match Arc::try_unwrap(shared) {
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
        });

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
    ) -> Result<(), String> {
        let pool = Arc::new(self);
        let worker_pool = Arc::clone(&pool);
        let (error_tx, error_rx) = bounded::<String>(1);
        let poison = Arc::new(AtomicBool::new(false));

        let reader_handle = thread::spawn({
            let poison = Arc::clone(&poison);
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
                        let gates = CmmRecordGates::for_record(&record, false, false, true, true);
                        batch.push(CmmBatchRecord { record, gates });
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
                    let _ = error_tx.send(error);
                }
            }
        });

        reader_handle
            .join()
            .map_err(|_| "CMM reader thread panicked".to_string())?;
        if let Ok(error) = error_rx.try_recv() {
            return Err(error);
        }
        drop(pool);
        Ok(())
    }
}

impl Drop for CmmWorkerPool {
    fn drop(&mut self) {
        for shutdown_tx in self.shutdown_txs.drain(..) {
            let _ = shutdown_tx.send(());
        }
        for handle in self.handles.drain(..) {
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
                record_tx
                    .send(record.map_err(|error| error.to_string()))
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = error_tx.send(error);
        }
    });

    while let Ok(record) = record_rx.recv() {
        consumer(record?)?;
        if let Ok(error) = error_rx.try_recv() {
            return Err(error);
        }
    }

    reader_handle
        .join()
        .map_err(|_| "BAM reader thread panicked".to_string())?;
    if let Ok(error) = error_rx.try_recv() {
        return Err(error);
    }
    Ok(())
}
