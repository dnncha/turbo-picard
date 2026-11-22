use rust_htslib::bam::{self, FetchDefinition, Read};
use rust_htslib::htslib;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use turbo_picard_core::bgzf_threads::{HtsThreadRole, bgzf_threads_for};
use turbo_picard_core::hts_io;

pub(crate) const BAM_ENC_TO_ASCII: [u8; 16] = *b"=ACMGRSVTWYHKDBN";

/// Maps 4-bit BAM base nibbles directly to `BaseCycleCounts` slot indices:
/// `0=A`, `1=C`, `2=G`, `3=T`, `4=N/other`.
pub(crate) const BAM_ENC_TO_BASE_SLOT: [u8; 16] = [
    4, // =
    0, // A
    1, // C
    4, // M
    2, // G
    4, // R
    4, // S
    4, // V
    3, // T
    4, // W
    4, // Y
    4, // H
    4, // K
    4, // D
    4, // B
    4, // N
];

const SINGLE_CONTIG_SPLIT_SPAN: usize = 4_000_000;

#[inline]
pub(crate) fn parallel_workers() -> usize {
    bgzf_threads_for(HtsThreadRole::RegionWorker)
        .unwrap_or(1)
        .max(1)
}

#[inline]
pub(crate) fn bam_nibble(encoded: &[u8], index: usize) -> u8 {
    let byte = encoded[index >> 1];
    (byte >> ((1 - (index & 1)) * 4)) & 0xf
}

pub(crate) fn count_mismatches_4bit(
    encoded: &[u8],
    read_offset: usize,
    reference: &[u8],
    ref_offset: usize,
    len: usize,
) -> u64 {
    let mut mismatches = 0_u64;
    let ref_slice = &reference[ref_offset..ref_offset + len];
    for (idx, &ref_base) in ref_slice.iter().enumerate() {
        let read_base = BAM_ENC_TO_ASCII[usize::from(bam_nibble(encoded, read_offset + idx))];
        let up_ref = if ref_base.is_ascii_lowercase() {
            ref_base - 32
        } else {
            ref_base
        };
        mismatches += u64::from(read_base != up_ref);
    }
    mismatches
}

pub(crate) fn build_gc_bins(
    sequence: &[u8],
    window_size: usize,
    window_counts: &mut [u64; 101],
) -> Vec<u8> {
    let window_end = sequence.len().saturating_sub(window_size);
    if window_size == 0 || window_end <= 1 {
        return Vec::new();
    }
    let mut bins = vec![255_u8; window_end];
    let mut gc = 0_usize;
    let mut no_calls = 0_usize;

    let base_class = |b: u8| -> (usize, usize) {
        match b {
            b'G' | b'g' | b'C' | b'c' => (1, 0),
            b'N' | b'n' => (0, 1),
            _ => (0, 0),
        }
    };

    for &b in &sequence[..window_size] {
        let (is_gc, is_n) = base_class(b);
        gc += is_gc;
        no_calls += is_n;
    }
    for start in 1..window_end {
        let (sub_gc, sub_n) = base_class(sequence[start - 1]);
        let (add_gc, add_n) = base_class(sequence[start + window_size - 1]);
        gc = gc + add_gc - sub_gc;
        no_calls = no_calls + add_n - sub_n;
        if no_calls <= 4 {
            let pct = (gc * 100) / window_size;
            if let Ok(pct_u8) = u8::try_from(pct) {
                bins[start] = pct_u8;
            }
            if pct <= 100 {
                window_counts[pct] += 1;
            }
        }
    }
    bins
}

#[inline]
pub(crate) fn lookup_gc_bin(
    bins: &[u8],
    ref_len: usize,
    start: usize,
    window_size: usize,
) -> Option<usize> {
    if start > ref_len {
        return None;
    }
    let last_calculated_start = ref_len.saturating_sub(window_size);
    if start >= last_calculated_start {
        return Some(0);
    }
    let bin = *bins.get(start)?;
    (bin <= 100).then_some(usize::from(bin))
}

pub(crate) fn current_index_for(input: &str) -> Option<PathBuf> {
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

pub(crate) fn open_indexed_optional_ref(
    input: &str,
    index: &Path,
    reference: Option<&str>,
) -> Result<bam::IndexedReader, hts_io::HtsIoError> {
    let reference = hts_io::resolve_reference_sequence(input, reference)?;
    let mut reader = bam::IndexedReader::from_path_and_index(Path::new(input), index)?;
    if let Some(reference) = reference.as_deref() {
        reader.set_reference(reference)?;
    }
    if hts_io::path_format(input) == Some(bam::Format::Cram) {
        reader.set_cram_options(htslib::hts_fmt_option_CRAM_OPT_DECODE_MD, 0)?;
    }
    Ok(reader)
}

#[derive(Clone, Copy)]
pub(crate) enum FetchJob {
    Region {
        tid: u32,
        start: i64,
        end: i64,
        split_bounds: bool,
    },
    Unmapped,
}

fn build_jobs(
    header: &bam::HeaderView,
    workers: usize,
    split_single_contig: bool,
) -> Vec<FetchJob> {
    let count = header.target_count();
    let mut jobs = Vec::with_capacity(usize::try_from(count).unwrap_or(0) + 1);
    let split_single = split_single_contig && count == 1;
    for tid in 0..count {
        let len = header
            .target_len(tid)
            .and_then(|value| i64::try_from(value).ok())
            .unwrap_or(i64::MAX);
        if split_single && len > i64::try_from(SINGLE_CONTIG_SPLIT_SPAN).unwrap_or(i64::MAX) {
            let span = i64::try_from(SINGLE_CONTIG_SPLIT_SPAN / workers.clamp(1, 4))
                .unwrap_or(1_000_000)
                .max(250_000);
            let mut start = 0_i64;
            while start < len {
                let end = start.saturating_add(span).min(len);
                jobs.push(FetchJob::Region {
                    tid,
                    start,
                    end,
                    split_bounds: true,
                });
                start = end;
            }
        } else if len > 0 {
            jobs.push(FetchJob::Region {
                tid,
                start: 0,
                end: len,
                split_bounds: false,
            });
        }
    }
    jobs.push(FetchJob::Unmapped);
    jobs
}

pub(crate) fn collect_indexed_parallel<S, MakeState, Observe, Merge>(
    input: &str,
    reference: Option<&str>,
    workers: usize,
    split_single_contig: bool,
    make_state: MakeState,
    observe: Observe,
    mut merge: Merge,
) -> Result<Option<(S, bam::HeaderView)>, String>
where
    S: Send,
    MakeState: Fn() -> Result<S, String> + Sync,
    Observe: Fn(&mut S, &bam::Record) -> Result<(), String> + Sync,
    Merge: FnMut(&mut S, S),
{
    if workers < 2 {
        return Ok(None);
    }
    let Some(index) = current_index_for(input) else {
        return Ok(None);
    };
    let Ok(probe) = open_indexed_optional_ref(input, &index, reference) else {
        return Ok(None);
    };
    let header = probe.header().clone();
    drop(probe);

    let jobs = build_jobs(&header, workers, split_single_contig);
    if jobs.len() <= 1 {
        return Ok(None);
    }
    let next_job = AtomicUsize::new(0);
    let active_workers = workers.min(jobs.len());

    let worker_results = thread::scope(|scope| {
        let mut handles = Vec::with_capacity(active_workers);
        for _ in 0..active_workers {
            let index = index.as_path();
            let jobs = jobs.as_slice();
            let next_job = &next_job;
            let make_state = &make_state;
            let observe = &observe;
            handles.push(scope.spawn(move || -> Result<S, String> {
                let mut reader = open_indexed_optional_ref(input, index, reference)
                    .map_err(|error| error.to_string())?;
                let mut state = make_state()?;
                let mut record = bam::Record::new();
                loop {
                    let idx = next_job.fetch_add(1, Ordering::Relaxed);
                    let Some(&job) = jobs.get(idx) else {
                        break;
                    };
                    match job {
                        FetchJob::Region {
                            tid,
                            start,
                            end,
                            split_bounds,
                        } => {
                            reader
                                .fetch((tid, start, end))
                                .map_err(|error| error.to_string())?;
                            while let Some(result) = reader.read(&mut record) {
                                result.map_err(|error| error.to_string())?;
                                if split_bounds {
                                    let pos = record.pos();
                                    if pos < start || pos >= end {
                                        continue;
                                    }
                                }
                                observe(&mut state, &record)?;
                            }
                        }
                        FetchJob::Unmapped => {
                            reader
                                .fetch(FetchDefinition::Unmapped)
                                .map_err(|error| error.to_string())?;
                            while let Some(result) = reader.read(&mut record) {
                                result.map_err(|error| error.to_string())?;
                                observe(&mut state, &record)?;
                            }
                        }
                    }
                }
                Ok(state)
            }));
        }
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err("indexed BAM worker panicked".to_string()))
            })
            .collect::<Result<Vec<_>, String>>()
    })?;

    let mut iter = worker_results.into_iter();
    let Some(mut combined) = iter.next() else {
        return Ok(None);
    };
    for part in iter {
        merge(&mut combined, part);
    }
    Ok(Some((combined, header)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_htslib::bam::record::{Cigar, CigarString};

    #[test]
    fn collect_indexed_parallel_visits_same_records_as_sequential_reader() {
        let dir = tempfile::tempdir().expect("tempdir exists");
        let bam_path = dir.path().join("multi.bam");
        let mut header = bam::Header::new();
        for name in ["chr1", "chr2"] {
            let mut sq = bam::header::HeaderRecord::new(b"SQ");
            sq.push_tag(b"SN", name);
            sq.push_tag(b"LN", 1000);
            header.push_record(&sq);
        }
        {
            let mut writer = bam::Writer::from_path(&bam_path, &header, bam::Format::Bam)
                .expect("writer creates");
            for (tid, pos, name, unmapped) in [
                (0, 10, b"r1".as_slice(), false),
                (0, 10, b"r1_unm_placed".as_slice(), true),
                (0, 500, b"r2".as_slice(), false),
                (1, 200, b"r3".as_slice(), false),
                (-1, -1, b"unplaced".as_slice(), true),
            ] {
                let mut rec = bam::Record::new();
                if tid >= 0 && !unmapped {
                    rec.set(
                        name,
                        Some(&CigarString(vec![Cigar::Match(10)])),
                        b"ACGTACGTAC",
                        &[30; 10],
                    );
                    rec.set_flags(0);
                } else {
                    rec.set(name, None, b"ACGTACGTAC", &[30; 10]);
                    rec.set_flags(0x4);
                }
                rec.set_tid(tid);
                rec.set_pos(pos);
                rec.set_mapq(if unmapped { 0 } else { 60 });
                writer.write(&rec).expect("record writes");
            }
        }
        bam::index::build(&bam_path, None, bam::index::Type::Bai, 1).expect("index builds");
        let input = bam_path.to_string_lossy().to_string();

        let mut seq_names = Vec::new();
        let mut reader = bam::Reader::from_path(&input).expect("reader opens");
        for rec in reader.records() {
            let rec = rec.expect("record reads");
            seq_names.push(String::from_utf8_lossy(rec.qname()).to_string());
        }
        seq_names.sort();

        let (mut par_names, _) = collect_indexed_parallel(
            &input,
            None,
            4,
            true,
            || Ok(Vec::<String>::new()),
            |names, rec| {
                names.push(String::from_utf8_lossy(rec.qname()).to_string());
                Ok(())
            },
            Extend::extend,
        )
        .expect("parallel runs")
        .expect("parallel used");
        par_names.sort();

        assert_eq!(par_names, seq_names);
    }

    #[test]
    fn build_gc_bins_matches_window_definition() {
        let seq = b"ACGTNnacgtGGGGCCCCAAAATTTT".repeat(10);
        let win = 20;
        let mut fast_counts = [0_u64; 101];
        let bins = build_gc_bins(&seq, win, &mut fast_counts);

        let mut ref_counts = [0_u64; 101];
        for start in 1..seq.len().saturating_sub(win) {
            let window = &seq[start..start + win];
            let gc = window
                .iter()
                .filter(|b| matches!(b.to_ascii_uppercase(), b'G' | b'C'))
                .count();
            let ns = window
                .iter()
                .filter(|b| b.eq_ignore_ascii_case(&b'N'))
                .count();
            let expected = if ns > 4 { None } else { Some((gc * 100) / win) };
            assert_eq!(lookup_gc_bin(&bins, seq.len(), start, win), expected);
            if let Some(pct) = expected {
                ref_counts[pct] += 1;
            }
        }
        assert_eq!(fast_counts, ref_counts);
    }
}
