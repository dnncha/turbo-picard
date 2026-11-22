use super::wgs_metrics::{WgsCollector, WgsCounts};
use rust_htslib::bam::{self, Read};
use rust_htslib::htslib;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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
    let shared = SharedProgress {
        next_region: AtomicUsize::new(0),
        failed: AtomicBool::new(false),
        region_capped: (0..regions.len()).map(|_| AtomicBool::new(false)).collect(),
        region_counts: Mutex::new((0..regions.len()).map(|_| None).collect()),
    };
    let worker_summaries = (0..workers.min(regions.len().max(1)))
        .map(|_| summary.region_worker())
        .collect::<Vec<_>>();

    let outcomes = thread::scope(|scope| {
        let handles = worker_summaries
            .into_iter()
            .map(|mut worker| {
                let regions = regions.as_slice();
                let target_names = target_names.as_slice();
                let shared = &shared;
                let index = index.as_path();
                scope.spawn(move || {
                    let outcome = run_worker(
                        input,
                        index,
                        reference,
                        regions,
                        target_names,
                        shared,
                        &mut worker,
                    );
                    if outcome.is_err() {
                        shared.failed.store(true, Ordering::Relaxed);
                    }
                    outcome
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

    if let Some((_, message)) = outcomes
        .into_iter()
        .filter_map(Result::err)
        .min_by_key(|failure| failure.0)
    {
        return Err(message);
    }
    let mut region_counts = shared
        .region_counts
        .into_inner()
        .map_err(|_| "CollectWgsMetrics region totals lock poisoned".to_string())?;
    let capped_contigs = capped_contig_ranges(&regions, &shared.region_capped);
    if !capped_contigs.is_empty() {
        let cluster_outcomes = reprocess_capped_contigs(
            input,
            &index,
            reference,
            summary,
            &regions,
            &target_names,
            &shared.region_capped,
            &capped_contigs,
            workers,
        )?;
        for outcome in cluster_outcomes {
            for range in outcome.cleared {
                for slot in &mut region_counts[range] {
                    *slot = None;
                }
            }
            for count in outcome.counts {
                summary.absorb_counts(&count);
            }
            if outcome.any_capped {
                summary.note_accumulation_cap_reached();
            }
        }
    }
    for counts in region_counts.into_iter().flatten() {
        summary.absorb_counts(&counts);
    }
    Ok(true)
}

struct SharedProgress {
    next_region: AtomicUsize,
    failed: AtomicBool,
    region_capped: Vec<AtomicBool>,
    region_counts: Mutex<Vec<Option<WgsCounts>>>,
}

fn capped_contig_ranges(
    regions: &[Region],
    region_capped: &[AtomicBool],
) -> Vec<(u32, Range<usize>)> {
    let mut contigs = Vec::new();
    let mut start = 0;
    while start < regions.len() {
        let tid = regions[start].tid;
        let mut end = start + 1;
        while end < regions.len() && regions[end].tid == tid {
            end += 1;
        }
        if (start..end).any(|idx| region_capped[idx].load(Ordering::Relaxed)) {
            contigs.push((tid, start..end));
        }
        start = end;
    }
    contigs
}

struct CappedContigOutcome {
    cleared: Vec<Range<usize>>,
    counts: Vec<WgsCounts>,
    any_capped: bool,
}

#[expect(
    clippy::too_many_arguments,
    reason = "threaded cluster reprocessing borrows shared slices cleanly"
)]
fn reprocess_capped_contigs(
    input: &str,
    index: &Path,
    reference: &str,
    summary: &WgsCollector,
    regions: &[Region],
    target_names: &[String],
    region_capped: &[AtomicBool],
    capped_contigs: &[(u32, Range<usize>)],
    workers: usize,
) -> Result<Vec<CappedContigOutcome>, String> {
    let next_contig = AtomicUsize::new(0);
    let cluster_threads = workers.min(capped_contigs.len()).max(1);
    let results = thread::scope(|scope| {
        let handles = (0..cluster_threads)
            .map(|_| {
                let next_contig = &next_contig;
                let mut worker = summary.cluster_worker();
                scope.spawn(move || -> Result<CappedContigOutcome, String> {
                    let mut reader =
                        open_indexed(input, index, reference).map_err(|e| e.to_string())?;
                    if capped_contigs.len() == 1
                        && let Some(threads) = bgzf_threads_for(HtsThreadRole::Reader)
                    {
                        reader.set_threads(threads).map_err(|e| e.to_string())?;
                    }
                    let mut record = bam::Record::new();
                    let mut cleared = Vec::new();
                    let mut counts = Vec::new();
                    let mut any_capped = false;
                    loop {
                        let pos = next_contig.fetch_add(1, Ordering::Relaxed);
                        let Some((tid, contig_range)) = capped_contigs.get(pos) else {
                            return Ok(CappedContigOutcome {
                                cleared,
                                counts,
                                any_capped,
                            });
                        };
                        let local_regions = &regions[contig_range.clone()];
                        let local_capped = &region_capped[contig_range.clone()];
                        let mut local_idx = 0;
                        while local_idx < local_regions.len() {
                            if !local_capped[local_idx].load(Ordering::Relaxed) {
                                local_idx += 1;
                                continue;
                            }
                            let local_start = local_idx;
                            let mut local_end = local_idx;
                            while local_end + 1 < local_regions.len()
                                && local_capped[local_end + 1].load(Ordering::Relaxed)
                            {
                                local_end += 1;
                            }
                            let cluster_counts = reprocess_cluster(
                                &mut reader,
                                &mut record,
                                &mut worker,
                                *tid,
                                target_names,
                                local_regions,
                                local_capped,
                                local_start,
                                &mut local_end,
                                &mut any_capped,
                            )?;
                            cleared.push(
                                (contig_range.start + local_start)
                                    ..(contig_range.start + local_end + 1),
                            );
                            counts.push(cluster_counts);
                            local_idx = local_end + 1;
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| {
                handle.join().unwrap_or_else(|_| {
                    Err("CollectWgsMetrics capped-cluster worker panicked".to_string())
                })
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    Ok(results)
}

#[expect(
    clippy::too_many_arguments,
    reason = "cluster reprocessing operates directly on borrowed slice state"
)]
fn reprocess_cluster(
    reader: &mut bam::IndexedReader,
    record: &mut bam::Record,
    worker: &mut WgsCollector,
    tid: u32,
    target_names: &[String],
    local_regions: &[Region],
    local_capped: &[AtomicBool],
    local_start: usize,
    local_end: &mut usize,
    any_capped: &mut bool,
) -> Result<WgsCounts, String> {
    loop {
        let fetch_start = local_regions[local_start].start;
        let fetch_end = local_regions[*local_end].end;
        worker.begin_region(fetch_start, fetch_end);
        reader
            .fetch((
                tid,
                i64::try_from(fetch_start).unwrap_or(i64::MAX),
                i64::try_from(fetch_end).unwrap_or(i64::MAX),
            ))
            .map_err(|error| error.to_string())?;
        while let Some(result) = reader.read(record) {
            result.map_err(|error| error.to_string())?;
            worker.observe(record, target_names)?;
        }
        let max_dropped = worker.max_dropped_end();
        let cap_active =
            *local_end + 1 < local_regions.len() && worker.accumulation_cap_active_at(fetch_end);
        let cluster_capped = worker.take_accumulation_cap_reached();
        let counts = worker.drain_counts();
        if *local_end + 1 < local_regions.len() && (max_dropped > fetch_end || cap_active) {
            *local_end += 1;
            while *local_end + 1 < local_regions.len()
                && local_regions[*local_end + 1].start < max_dropped
            {
                *local_end += 1;
            }
            while *local_end + 1 < local_regions.len()
                && local_capped[*local_end + 1].load(Ordering::Relaxed)
            {
                *local_end += 1;
            }
            continue;
        }
        *any_capped |= cluster_capped;
        return Ok(counts);
    }
}

fn run_worker(
    input: &str,
    index: &Path,
    reference: &str,
    regions: &[Region],
    target_names: &[String],
    shared: &SharedProgress,
    worker: &mut WgsCollector,
) -> Result<(), (usize, String)> {
    let mut reader =
        open_indexed(input, index, reference).map_err(|error| (0, error.to_string()))?;
    let mut record = bam::Record::new();
    loop {
        let position = shared.next_region.fetch_add(1, Ordering::Relaxed);
        if shared.failed.load(Ordering::Relaxed) {
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
        let counts = worker.drain_counts();
        if worker.take_accumulation_cap_reached() {
            shared.region_capped[position].store(true, Ordering::Relaxed);
            continue;
        }
        shared.region_counts.lock().map_err(|_| {
            (
                position,
                "CollectWgsMetrics region totals lock poisoned".to_string(),
            )
        })?[position] = Some(counts);
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
    fn contigs_that_reach_the_accumulation_cap_are_redone_in_one_pass() {
        let dir = tempfile::tempdir().expect("tempdir exists");
        let (input, reference) = write_fixture(dir.path(), 2, 40);
        for capped in [options(2, 3), options(4, 9), options(250, 30)] {
            let expected = one_pass_text(&input, &reference, capped);
            for region_span in [50, 700, 100_000] {
                let mut collector =
                    WgsCollector::for_run(&reference, None, capped, None).expect("reference loads");
                assert!(
                    collect_by_region_with(&input, &reference, &mut collector, 4, region_span)
                        .expect("regional collection succeeds")
                );
                collector.finish();
                assert_eq!(
                    collector.to_picard_text(10_000, true),
                    expected,
                    "options {capped:?} span {region_span}"
                );
            }
        }
    }

    #[test]
    fn regional_collection_declines_without_workers_or_a_current_index() {
        let dir = tempfile::tempdir().expect("tempdir exists");
        let (input, reference) = write_fixture(dir.path(), 2, 6);
        let collect = |workers: usize| {
            let mut collector =
                WgsCollector::for_run(&reference, None, options(250, 100_000), None)
                    .expect("reference loads");
            collect_by_region_with(&input, &reference, &mut collector, workers, 1_000)
                .expect("regional collection reports its decision")
        };
        assert!(collect(4));
        assert!(!collect(1));
        std::fs::remove_file(format!("{input}.bai")).expect("index fixture is removable");
        assert!(!collect(4));
    }

    #[test]
    fn localized_capped_cluster_extends_across_region_boundary_matching_one_pass() {
        let dir = tempfile::tempdir().expect("tempdir exists");
        let mut header = bam::Header::new();
        let mut sq = bam::header::HeaderRecord::new(b"SQ");
        sq.push_tag(b"SN", "chr1");
        sq.push_tag(b"LN", 400);
        header.push_record(&sq);
        let reference = dir.path().join("ref.fa");
        std::fs::write(&reference, format!(">chr1\n{}\n", "A".repeat(400)))
            .expect("reference fixture is writable");
        rust_htslib::faidx::Reader::from_path(&reference).expect("reference fixture is indexable");
        let bam_path = dir.path().join("hotspot.bam");
        {
            let mut writer = bam::Writer::from_path(&bam_path, &header, bam::Format::Bam)
                .expect("BAM fixture is writable");
            let mut push_read = |name: &[u8], pos: i64, len: usize| {
                let mut rec = bam::Record::new();
                rec.set(
                    name,
                    Some(&bam::record::CigarString(vec![bam::record::Cigar::Match(
                        u32::try_from(len).expect("len fits u32"),
                    )])),
                    &vec![b'A'; len],
                    &vec![30_u8; len],
                );
                rec.set_tid(0);
                rec.set_pos(pos);
                rec.set_mapq(60);
                rec.set_flags(0);
                writer.write(&rec).expect("fixture record is writable");
            };
            // Hotspot stacked inside region [0, 100), with one dropped read extending past 100 into [100, 200)
            for idx in 0..8 {
                let pos = if idx < 5 { 40 } else { 42 };
                let len = if idx == 7 { 90 } else { 20 };
                push_read(format!("hot-{idx}").as_bytes(), pos, len);
            }
            // Normal reads in later uncapped regions
            push_read(b"tail-a", 150, 30);
            push_read(b"tail-b", 260, 40);
        }
        bam::index::build(&bam_path, None, bam::index::Type::Bai, 1)
            .expect("BAM fixture is indexable");
        let input = bam_path.to_string_lossy().to_string();
        let ref_str = reference.to_string_lossy().to_string();

        let opts = WgsOptions {
            minimum_mapping_quality: 20,
            minimum_base_quality: 20,
            coverage_cap: 3,
            locus_accumulation_cap: 5,
            count_unpaired: true,
            exclude_supplementary: false,
        };
        let mut expected_collector =
            WgsCollector::for_run(&ref_str, None, opts, None).expect("reference loads");
        let target_names = vec!["chr1".to_string()];
        let mut reader = bam::Reader::from_path(&input).expect("fixture opens");
        let mut record = bam::Record::new();
        while let Some(res) = reader.read(&mut record) {
            res.expect("record decodes");
            expected_collector
                .observe(&record, &target_names)
                .expect("record observed");
        }
        expected_collector.finish();
        let expected = expected_collector.to_picard_text(10_000, true);

        for span in [60, 100, 125] {
            let mut collector =
                WgsCollector::for_run(&ref_str, None, opts, None).expect("reference loads");
            assert!(
                collect_by_region_with(&input, &ref_str, &mut collector, 2, span)
                    .expect("regional collection succeeds")
            );
            collector.finish();
            assert_eq!(
                collector.to_picard_text(10_000, true),
                expected,
                "span={span}"
            );
        }
    }
}
