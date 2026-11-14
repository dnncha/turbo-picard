use super::wgs_metrics::WgsCollector;
use rust_htslib::bam::{self, Read};
use rust_htslib::htslib;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use turbo_picard_core::bgzf_threads::{HtsThreadRole, bgzf_threads_for};
use turbo_picard_core::hts_io;

const REGION_SPAN: usize = 4_000_000;

#[derive(Debug, Clone, Copy)]
struct Region {
    tid: u32,
    start: usize,
    end: usize,
}

pub(crate) fn collect_by_region(
    input: &str,
    reference: &str,
    summary: &mut WgsCollector,
) -> Result<bool, String> {
    let workers = bgzf_threads_for(HtsThreadRole::RegionWorker).unwrap_or(1);
    let region_span = std::env::var("TURBO_PICARD_WGS_REGION_SPAN")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|span| *span > 0)
        .unwrap_or(REGION_SPAN);
    collect_by_region_with(input, reference, summary, workers, region_span)
}

fn collect_by_region_with(
    input: &str,
    reference: &str,
    summary: &mut WgsCollector,
    workers: usize,
    region_span: usize,
) -> Result<bool, String> {
    if workers < 2 {
        return Ok(false);
    }
    let Some(index) = current_index_for(input) else {
        return Ok(false);
    };
    let Ok(probe) = open_indexed(input, &index, reference) else {
        return Ok(false);
    };
    let header = probe.header().clone();
    drop(probe);
    let target_names = header
        .target_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name).to_string())
        .collect::<Vec<_>>();
    let regions = regions_for(&header, region_span);
    let next_region = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let worker_summaries = (0..workers.min(regions.len().max(1)))
        .map(|_| summary.region_worker())
        .collect::<Vec<_>>();

    let outcomes = thread::scope(|scope| {
        let handles = worker_summaries
            .into_iter()
            .map(|mut worker| {
                let regions = regions.as_slice();
                let target_names = target_names.as_slice();
                let next_region = &next_region;
                let failed = &failed;
                let index = index.as_path();
                scope.spawn(move || {
                    let outcome = run_worker(
                        input,
                        index,
                        reference,
                        regions,
                        target_names,
                        next_region,
                        failed,
                        &mut worker,
                    );
                    if outcome.is_err() {
                        failed.store(true, Ordering::Relaxed);
                    }
                    outcome.map(|()| worker)
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| {
                handle.join().unwrap_or_else(|_| {
                    Err((
                        usize::MAX,
                        "CollectWgsMetrics region worker panicked".to_string(),
                    ))
                })
            })
            .collect::<Vec<_>>()
    });

    let mut first_failure: Option<(usize, String)> = None;
    let mut finished = Vec::new();
    for outcome in outcomes {
        match outcome {
            Ok(worker) => finished.push(worker),
            Err(failure) => {
                if first_failure
                    .as_ref()
                    .is_none_or(|current| failure.0 < current.0)
                {
                    first_failure = Some(failure);
                }
            }
        }
    }
    if let Some((_, message)) = first_failure {
        return Err(message);
    }
    if finished.iter().any(WgsCollector::accumulation_cap_reached) {
        return Ok(false);
    }
    for worker in finished {
        summary.absorb(worker);
    }
    Ok(true)
}

#[expect(
    clippy::too_many_arguments,
    reason = "one borrowed handle per piece of shared worker state"
)]
fn run_worker(
    input: &str,
    index: &Path,
    reference: &str,
    regions: &[Region],
    target_names: &[String],
    next_region: &AtomicUsize,
    failed: &AtomicBool,
    worker: &mut WgsCollector,
) -> Result<(), (usize, String)> {
    let mut reader =
        open_indexed(input, index, reference).map_err(|error| (0, error.to_string()))?;
    let mut record = bam::Record::new();
    loop {
        let position = next_region.fetch_add(1, Ordering::Relaxed);
        if failed.load(Ordering::Relaxed) {
            return Ok(());
        }
        let Some(region) = regions.get(position) else {
            return Ok(());
        };
        worker.begin_region(region.start, region.end);
        reader
            .fetch((
                region.tid,
                i64::try_from(region.start).unwrap_or(i64::MAX),
                i64::try_from(region.end).unwrap_or(i64::MAX),
            ))
            .map_err(|error| (position, error.to_string()))?;
        while let Some(result) = reader.read(&mut record) {
            result.map_err(|error| (position, error.to_string()))?;
            worker
                .observe(&record, target_names)
                .map_err(|error| (position, error))?;
        }
        if worker.accumulation_cap_reached() {
            failed.store(true, Ordering::Relaxed);
            return Ok(());
        }
    }
}

fn regions_for(header: &bam::HeaderView, region_span: usize) -> Vec<Region> {
    let mut regions = Vec::new();
    for tid in 0..header.target_count() {
        let length = header
            .target_len(tid)
            .map_or(0, |length| usize::try_from(length).unwrap_or(usize::MAX));
        let mut start = 0;
        while start < length {
            let end = start.saturating_add(region_span).min(length);
            regions.push(Region { tid, start, end });
            start = end;
        }
    }
    regions
}

fn open_indexed(
    input: &str,
    index: &Path,
    reference: &str,
) -> Result<bam::IndexedReader, hts_io::HtsIoError> {
    let reference = hts_io::resolve_reference_sequence(input, Some(reference))?;
    let mut reader = bam::IndexedReader::from_path_and_index(Path::new(input), index)?;
    if let Some(reference) = reference.as_deref() {
        reader.set_reference(reference)?;
    }
    if hts_io::path_format(input) == Some(bam::Format::Cram) {
        reader.set_cram_options(htslib::hts_fmt_option_CRAM_OPT_DECODE_MD, 0)?;
    }
    Ok(reader)
}

fn current_index_for(input: &str) -> Option<PathBuf> {
    let input_path = Path::new(input);
    let input_modified = input_path.metadata().ok()?.modified().ok()?;
    let suffixes: &[&str] = if hts_io::path_format(input) == Some(bam::Format::Cram) {
        &["crai"]
    } else {
        &["bai", "csi"]
    };
    suffixes
        .iter()
        .flat_map(|suffix| {
            [
                PathBuf::from(format!("{input}.{suffix}")),
                input_path.with_extension(suffix),
            ]
        })
        .find(|candidate| {
            candidate
                .metadata()
                .and_then(|metadata| metadata.modified())
                .is_ok_and(|index_modified| index_modified >= input_modified)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wgs_metrics::WgsOptions;
    use crate::wgs_metrics::tests::{
        FIXTURE_CONTIGS, fixture_reads, fixture_reference_text, fixture_target_names,
    };

    fn options(coverage_cap: u32, locus_accumulation_cap: u32) -> WgsOptions {
        WgsOptions {
            minimum_mapping_quality: 20,
            minimum_base_quality: 20,
            coverage_cap,
            locus_accumulation_cap,
            count_unpaired: false,
            exclude_supplementary: false,
        }
    }

    fn write_fixture(dir: &Path, seed: u64, depth_scale: usize) -> (String, String) {
        let mut header = bam::Header::new();
        let mut reference_text = Vec::new();
        for ((name, length), bases) in FIXTURE_CONTIGS.iter().zip(fixture_reference_text(seed)) {
            let mut line = bam::header::HeaderRecord::new(b"SQ");
            line.push_tag(b"SN", name);
            line.push_tag(b"LN", length);
            header.push_record(&line);
            reference_text.extend_from_slice(format!(">{name}\n").as_bytes());
            reference_text.extend_from_slice(&bases);
            reference_text.push(b'\n');
        }
        let reference = dir.join(format!("ref{seed}.fa"));
        std::fs::write(&reference, reference_text).expect("reference fixture is writable");
        rust_htslib::faidx::Reader::from_path(&reference).expect("reference fixture is indexable");
        let bam_path = dir.join(format!("fixture{seed}.bam"));
        {
            let mut writer = bam::Writer::from_path(&bam_path, &header, bam::Format::Bam)
                .expect("BAM fixture is writable");
            for read in fixture_reads(seed, depth_scale) {
                writer
                    .write(&read.to_record())
                    .expect("fixture record is writable");
            }
        }
        bam::index::build(&bam_path, None, bam::index::Type::Bai, 1)
            .expect("BAM fixture is indexable");
        (
            bam_path.to_string_lossy().to_string(),
            reference.to_string_lossy().to_string(),
        )
    }

    fn one_pass_text(input: &str, reference: &str, options: WgsOptions) -> String {
        let mut collector =
            WgsCollector::for_run(reference, None, options, None).expect("reference loads");
        let mut reader = bam::Reader::from_path(input).expect("fixture opens");
        let target_names = fixture_target_names();
        let mut record = bam::Record::new();
        while let Some(result) = reader.read(&mut record) {
            result.expect("fixture record decodes");
            collector
                .observe(&record, &target_names)
                .expect("fixture record is accepted");
        }
        collector.finish();
        collector.to_picard_text(10_000, true)
    }

    #[test]
    fn regional_collection_matches_one_pass_for_every_region_size() {
        let dir = tempfile::tempdir().expect("tempdir exists");
        for (seed, depth_scale, coverage_cap) in [(7, 6, 250), (11, 40, 12), (23, 15, 3)] {
            let (input, reference) = write_fixture(dir.path(), seed, depth_scale);
            let options = options(coverage_cap, 100_000);
            let expected = one_pass_text(&input, &reference, options);
            assert!(expected.contains("picard.analysis.WgsMetrics"));
            for region_span in [1, 7, 64, 333, 1_000, 5_000, 1_000_000] {
                for workers in [2, 5] {
                    let mut collector = WgsCollector::for_run(&reference, None, options, None)
                        .expect("reference loads");
                    assert!(
                        collect_by_region_with(
                            &input,
                            &reference,
                            &mut collector,
                            workers,
                            region_span
                        )
                        .expect("regional collection succeeds")
                    );
                    collector.finish();
                    assert_eq!(
                        collector.to_picard_text(10_000, true),
                        expected,
                        "seed {seed} span {region_span} workers {workers}"
                    );
                }
            }
        }
    }

    #[test]
    fn regional_collection_declines_when_one_pass_is_required() {
        let dir = tempfile::tempdir().expect("tempdir exists");
        let (input, reference) = write_fixture(dir.path(), 2, 40);
        let collect = |options: WgsOptions, workers: usize| {
            let mut collector =
                WgsCollector::for_run(&reference, None, options, None).expect("reference loads");
            collect_by_region_with(&input, &reference, &mut collector, workers, 1_000)
                .expect("regional collection reports its decision")
        };
        assert!(collect(options(250, 100_000), 4));
        assert!(!collect(options(250, 100_000), 1));
        assert!(!collect(options(2, 3), 4));
        std::fs::remove_file(format!("{input}.bai")).expect("index fixture is removable");
        assert!(!collect(options(250, 100_000), 4));
    }
}
