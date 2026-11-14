use super::{
    JavaRandom, f64_to_u64, fold_base_penalty, format_float, is_adapter_read,
    mad_f64_from_histogram_u64, mean_from_histogram_u32, median_f64_from_histogram_u64,
    pct_at_least, read_reference_contigs_for_wgs, sample_standard_deviation_from_histogram_u32,
    u16_or_max, usize_or_max,
};
use flate2::read::MultiGzDecoder;
use rust_htslib::bam;
use rust_htslib::faidx;
use rustc_hash::{FxHashMap, FxHasher};
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::fs;
use std::hash::Hasher;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use turbo_picard_core::bgzf_threads::{HtsThreadRole, bgzf_threads_for};
use turbo_picard_core::hts_io;

const FLAG_PAIRED: u16 = 0x1;
const FLAG_UNMAPPED: u16 = 0x4;
const FLAG_MATE_UNMAPPED: u16 = 0x8;
const FLAG_REVERSE: u16 = 0x10;
const FLAG_SECONDARY: u16 = 0x100;
const FLAG_FAILS_VENDOR_QUALITY: u16 = 0x200;
const FLAG_DUPLICATE: u16 = 0x400;
const FLAG_SUPPLEMENTARY: u16 = 0x800;
const LOWEST_COUNTED_QUALITY: u8 = 3;
const PICARD_QUALITY_BINS: usize = 127;
const REFERENCE_SCAN_SPAN: usize = 16_000_000;
const HET_SENSITIVITY_MAX_DEPTH: usize = 1000;
const HET_SENSITIVITY_LOG_ODDS: f64 = 3.0;

type NoCallRuns = Vec<(usize, usize)>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct WgsOptions {
    pub(crate) minimum_mapping_quality: u8,
    pub(crate) minimum_base_quality: u8,
    pub(crate) coverage_cap: u32,
    pub(crate) locus_accumulation_cap: u32,
    pub(crate) count_unpaired: bool,
    pub(crate) exclude_supplementary: bool,
}

#[derive(Debug)]
struct WgsContig {
    length: usize,
    no_call_runs: Vec<(usize, usize)>,
    included: Option<Vec<bool>>,
    interval_runs: Option<Vec<(usize, usize)>>,
    included_loci: usize,
}

#[derive(Debug)]
pub(crate) struct WgsReference {
    contigs: Vec<WgsContig>,
    by_name: FxHashMap<String, usize>,
    has_intervals: bool,
}

impl WgsReference {
    pub(crate) fn load(
        reference: &str,
        interval_masks: Option<BTreeMap<String, Vec<bool>>>,
        scan_threads: usize,
    ) -> Result<Self, String> {
        let lengths = read_reference_contigs_for_wgs(reference)?;
        let no_call_runs = reference_no_call_runs(reference, &lengths, scan_threads)?;
        Ok(Self::from_parts(lengths, no_call_runs, interval_masks))
    }

    fn from_parts(
        lengths: Vec<(String, usize)>,
        no_call_runs: Vec<Vec<(usize, usize)>>,
        mut interval_masks: Option<BTreeMap<String, Vec<bool>>>,
    ) -> Self {
        let has_intervals = interval_masks.is_some();
        let mut by_name = FxHashMap::default();
        let contigs = lengths
            .into_iter()
            .zip(no_call_runs)
            .enumerate()
            .map(|(index, ((name, length), no_call_runs))| {
                by_name.insert(name.clone(), index);
                let mask = interval_masks
                    .as_mut()
                    .map(|masks| masks.remove(&name).unwrap_or_default());
                let interval_runs = mask.as_deref().map(true_runs);
                let included = mask.map(|mut mask| {
                    for (start, end) in &no_call_runs {
                        let end = (*end).min(mask.len());
                        if *start < end {
                            mask[*start..end].fill(false);
                        }
                    }
                    mask
                });
                let included_loci = included.as_ref().map_or_else(
                    || length - no_call_runs.iter().map(|(s, e)| e - s).sum::<usize>(),
                    |mask| mask.iter().filter(|included| **included).count(),
                );
                WgsContig {
                    length,
                    no_call_runs,
                    included,
                    interval_runs,
                    included_loci,
                }
            })
            .collect();
        Self {
            contigs,
            by_name,
            has_intervals,
        }
    }

    pub(crate) fn limit_included_loci(&mut self, limit: usize) -> Option<(usize, usize)> {
        let mut remaining = limit;
        let mut last_kept = None;
        for (contig_index, contig) in self.contigs.iter_mut().enumerate() {
            if remaining >= contig.included_loci {
                remaining -= contig.included_loci;
                continue;
            }
            let mut mask = contig.included.take().unwrap_or_else(|| {
                let mut mask = vec![true; contig.length];
                for (start, end) in &contig.no_call_runs {
                    mask[*start..*end].fill(false);
                }
                mask
            });
            if remaining == 0 {
                mask.clear();
            } else {
                let mut kept = 0;
                for (locus, included) in mask.iter_mut().enumerate() {
                    if *included {
                        if kept == remaining {
                            *included = false;
                        } else {
                            kept += 1;
                            last_kept = Some((contig_index, locus));
                        }
                    }
                }
            }
            contig.included = Some(mask);
            contig.included_loci = remaining;
            remaining = 0;
        }
        last_kept
    }

    fn territory(&self) -> u64 {
        self.contigs
            .iter()
            .map(|contig| contig.included_loci as u64)
            .sum()
    }
}

fn true_runs(mask: &[bool]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut open: Option<usize> = None;
    for (index, included) in mask.iter().enumerate() {
        match (included, open) {
            (true, None) => open = Some(index),
            (false, Some(start)) => {
                runs.push((start, index));
                open = None;
            }
            _ => {}
        }
    }
    if let Some(start) = open {
        runs.push((start, mask.len()));
    }
    runs
}

fn is_no_call_reference_base(base: u8) -> bool {
    matches!(base, b'N' | b'n' | b'.')
}

fn append_no_call_runs(bases: &[u8], origin: usize, runs: &mut Vec<(usize, usize)>) {
    let mut offset = 0;
    for chunk in bases.chunks(64) {
        if chunk.iter().any(|base| is_no_call_reference_base(*base)) {
            for (index, base) in chunk.iter().enumerate() {
                if is_no_call_reference_base(*base) {
                    let position = origin + offset + index;
                    match runs.last_mut() {
                        Some(last) if last.1 == position => last.1 = position + 1,
                        _ => runs.push((position, position + 1)),
                    }
                }
            }
        }
        offset += chunk.len();
    }
}

fn reference_no_call_runs(
    reference: &str,
    lengths: &[(String, usize)],
    scan_threads: usize,
) -> Result<Vec<Vec<(usize, usize)>>, String> {
    if !Path::new(&format!("{reference}.fai")).is_file() {
        return fasta_text_no_call_runs(reference, lengths);
    }
    let mut spans = Vec::new();
    for (contig, (_, length)) in lengths.iter().enumerate() {
        let mut start = 0;
        while start < *length {
            let end = start.saturating_add(REFERENCE_SCAN_SPAN).min(*length);
            spans.push((contig, start, end));
            start = end;
        }
    }
    let next_span = AtomicUsize::new(0);
    let threads = scan_threads.clamp(1, spans.len().max(1));
    let scanned = thread::scope(|scope| {
        let handles = (0..threads)
            .map(|_| {
                let spans = spans.as_slice();
                let next_span = &next_span;
                scope.spawn(move || -> Result<Vec<(usize, NoCallRuns)>, String> {
                    let reader =
                        faidx::Reader::from_path(reference).map_err(|error| error.to_string())?;
                    let mut found = Vec::new();
                    loop {
                        let position = next_span.fetch_add(1, Ordering::Relaxed);
                        let Some((contig, start, end)) = spans.get(position).copied() else {
                            return Ok(found);
                        };
                        let bases = reader
                            .fetch_seq(&lengths[contig].0, start, end - 1)
                            .map_err(|error| error.to_string())?;
                        let mut runs = Vec::new();
                        append_no_call_runs(&bases, start, &mut runs);
                        found.push((position, runs));
                    }
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err("reference scan worker panicked".to_string()))
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    let mut by_span = scanned.into_iter().flatten().collect::<Vec<_>>();
    by_span.sort_by_key(|(position, _)| *position);
    let mut runs = vec![Vec::new(); lengths.len()];
    for (position, span_runs) in by_span {
        let contig_runs: &mut Vec<(usize, usize)> = &mut runs[spans[position].0];
        for run in span_runs {
            match contig_runs.last_mut() {
                Some(last) if last.1 == run.0 => last.1 = run.1,
                _ => contig_runs.push(run),
            }
        }
    }
    Ok(runs)
}

fn fasta_text_no_call_runs(
    reference: &str,
    lengths: &[(String, usize)],
) -> Result<Vec<Vec<(usize, usize)>>, String> {
    let file = fs::File::open(reference).map_err(|error| error.to_string())?;
    let mut reader: Box<dyn BufRead> = if Path::new(reference)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"))
    {
        Box::new(BufReader::with_capacity(1 << 20, MultiGzDecoder::new(file)))
    } else {
        Box::new(BufReader::with_capacity(1 << 20, file))
    };
    let mut runs = vec![Vec::new(); lengths.len()];
    let mut contig: Option<usize> = None;
    let mut next_contig = 0;
    let mut position = 0;
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        while line.last().is_some_and(u8::is_ascii_whitespace) {
            line.pop();
        }
        if line.first() == Some(&b'>') {
            contig = (next_contig < lengths.len()).then_some(next_contig);
            next_contig += 1;
            position = 0;
        } else if let Some(contig) = contig {
            append_no_call_runs(&line, position, &mut runs[contig]);
            position += line.len();
        }
    }
    Ok(runs)
}

#[derive(Debug, Default)]
struct NameEntry {
    serial: u64,
    hash: u64,
    start: usize,
    end: usize,
    name: Vec<u8>,
    cigar: Vec<u32>,
    qualities: Vec<u8>,
    encoded_bases: Vec<u8>,
    previous: Option<(usize, u64)>,
}

impl NameEntry {
    fn passes_at(&self, read_index: usize, pass_floor: u8) -> bool {
        self.qualities
            .get(read_index)
            .is_some_and(|quality| *quality >= pass_floor)
            && (self.encoded_bases.is_empty()
                || !is_no_call_read_base(&self.encoded_bases, read_index))
    }

    fn mark_passing_loci(&self, start: usize, end: usize, pass_floor: u8, bits: &mut [u64]) {
        let mut reference_offset = self.start;
        let mut read_offset = 0;
        for raw in &self.cigar {
            let length = (raw >> 4) as usize;
            match raw & 0xf {
                0 | 7 | 8 => {
                    let lo = reference_offset.max(start);
                    let hi = (reference_offset + length).min(end);
                    for locus in lo..hi {
                        if self.passes_at(read_offset + (locus - reference_offset), pass_floor) {
                            let bit = locus - start;
                            bits[bit / 64] |= 1 << (bit % 64);
                        }
                    }
                    reference_offset += length;
                    read_offset += length;
                }
                1 | 4 => read_offset += length,
                2 | 3 => reference_offset += length,
                _ => {}
            }
        }
    }
}

#[derive(Debug, Default)]
struct NameWindow {
    entries: Vec<NameEntry>,
    free: Vec<usize>,
    arrival: VecDeque<usize>,
    newest: FxHashMap<u64, usize>,
    serial: u64,
}

impl NameWindow {
    fn clear(&mut self) {
        for slot in self.arrival.drain(..) {
            self.entries[slot].serial = 0;
            self.free.push(slot);
        }
        self.newest.clear();
    }

    fn expire(&mut self, before: usize) {
        while let Some(slot) = self.arrival.front().copied() {
            if self.entries[slot].end > before {
                break;
            }
            self.arrival.pop_front();
            if self.newest.get(&self.entries[slot].hash) == Some(&slot) {
                self.newest.remove(&self.entries[slot].hash);
            }
            self.entries[slot].serial = 0;
            self.free.push(slot);
        }
    }

    fn mark_seen(
        &self,
        hash: u64,
        name: &[u8],
        start: usize,
        end: usize,
        pass_floor: u8,
        bits: &mut Vec<u64>,
    ) -> Option<(usize, usize)> {
        let mut cursor = self
            .newest
            .get(&hash)
            .map(|slot| (*slot, self.entries[*slot].serial));
        let mut hull: Option<(usize, usize)> = None;
        while let Some((slot, serial)) = cursor {
            let entry = &self.entries[slot];
            if entry.serial != serial {
                break;
            }
            if entry.end > start && entry.start < end && entry.name == name {
                if hull.is_none() {
                    bits.clear();
                    bits.resize((end - start).div_ceil(64), 0);
                }
                entry.mark_passing_loci(start, end, pass_floor, bits);
                let lo = entry.start.max(start);
                let hi = entry.end.min(end);
                hull = Some(hull.map_or((lo, hi), |(seen_lo, seen_hi)| {
                    (seen_lo.min(lo), seen_hi.max(hi))
                }));
            }
            cursor = entry.previous;
        }
        hull
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "one borrowed field of the alignment per argument"
    )]
    fn insert(
        &mut self,
        hash: u64,
        name: &[u8],
        start: usize,
        end: usize,
        cigar: &[u32],
        qualities: &[u8],
        encoded_bases: &[u8],
    ) {
        let slot = self.free.pop().unwrap_or_else(|| {
            self.entries.push(NameEntry::default());
            self.entries.len() - 1
        });
        self.serial += 1;
        let previous = self
            .newest
            .insert(hash, slot)
            .filter(|older| *older != slot)
            .map(|older| (older, self.entries[older].serial));
        let entry = &mut self.entries[slot];
        entry.serial = self.serial;
        entry.hash = hash;
        entry.start = start;
        entry.end = end;
        entry.name.clear();
        entry.name.extend_from_slice(name);
        entry.cigar.clear();
        entry.cigar.extend_from_slice(cigar);
        entry.qualities.clear();
        entry.qualities.extend_from_slice(qualities);
        entry.encoded_bases.clear();
        entry.encoded_bases.extend_from_slice(encoded_bases);
        entry.previous = previous;
        self.arrival.push_back(slot);
    }
}

fn is_no_call_read_base(encoded_bases: &[u8], read_index: usize) -> bool {
    encoded_bases
        .get(read_index / 2)
        .is_some_and(|byte| (byte >> (4 * (1 - read_index % 2))) & 0xf == 0xf)
}

fn has_no_call_read_base(encoded_bases: &[u8]) -> bool {
    encoded_bases
        .iter()
        .any(|byte| byte & 0xf0 == 0xf0 || byte & 0x0f == 0x0f)
}

fn cigar_extent(cigar: &[u32]) -> (u64, usize) {
    let mut aligned_bases = 0_u64;
    let mut reference_span = 0_usize;
    for raw in cigar {
        let length = raw >> 4;
        match raw & 0xf {
            0 | 7 | 8 => {
                aligned_bases += u64::from(length);
                reference_span += length as usize;
            }
            2 | 3 => reference_span += length as usize,
            _ => {}
        }
    }
    (aligned_bases, reference_span)
}

#[derive(Debug, Default)]
pub(crate) struct WgsCounts {
    excluded: [u64; 7],
    covered: [u64; 2],
    high_quality: Vec<u64>,
    unfiltered: Vec<u64>,
    base_quality: Vec<u64>,
}

impl WgsCounts {
    pub(crate) fn merge(&mut self, other: &Self) {
        for (total, part) in self.excluded.iter_mut().zip(other.excluded) {
            *total += part;
        }
        for (total, part) in self.covered.iter_mut().zip(other.covered) {
            *total += part;
        }
        add_bins(&mut self.high_quality, &other.high_quality);
        add_bins(&mut self.unfiltered, &other.unfiltered);
        add_bins(&mut self.base_quality, &other.base_quality);
    }
}

fn add_bins(total: &mut Vec<u64>, part: &[u64]) {
    if total.len() < part.len() {
        total.resize(part.len(), 0);
    }
    for (total, part) in total.iter_mut().zip(part) {
        *total += part;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadExclusion {
    Adapter,
    MappingQuality,
    Duplicate,
    Unpaired,
}

#[derive(Debug)]
pub(crate) struct WgsCollector {
    reference: Arc<WgsReference>,
    options: WgsOptions,
    depth_limit: u16,
    pass_floor: u8,
    excluded_adapter: u64,
    excluded_mapq: u64,
    excluded_duplicate: u64,
    excluded_unpaired: u64,
    excluded_baseq: u64,
    excluded_overlap: u64,
    excluded_capped: u64,
    high_quality_histogram: Vec<u64>,
    unfiltered_histogram: Vec<u64>,
    high_quality_covered: u64,
    unfiltered_covered: u64,
    base_quality_histogram: Vec<u64>,
    quality_lanes: Box<[[u64; 256]; 4]>,
    active_tid: Option<i32>,
    active_contig: usize,
    visited: Vec<bool>,
    high_quality: Vec<u16>,
    unfiltered: Vec<u16>,
    raw: Vec<u32>,
    window_base: usize,
    window_offset: usize,
    last_start: Option<usize>,
    no_call_cursor: usize,
    names: NameWindow,
    seen_bits: Vec<u64>,
    read_bases: Vec<u8>,
    simulate_accumulation: bool,
    accumulator_head: Option<usize>,
    accumulator_end: usize,
    accumulation_cap_reached: bool,
    locus_window: Option<(usize, usize)>,
    stop_target: Option<(usize, usize)>,
    last_settled_covered: Option<usize>,
    stop: StopState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopState {
    Running,
    AwaitingNextPassingRead,
    Stopped,
}

struct AlignedRead<'a> {
    start: usize,
    qualities: &'a [u8],
    encoded_bases: &'a [u8],
    plain: bool,
    guard: Option<(usize, usize)>,
}

impl WgsCollector {
    pub(crate) fn new(reference: Arc<WgsReference>, options: WgsOptions) -> Self {
        let options = WgsOptions {
            locus_accumulation_cap: options.locus_accumulation_cap.max(options.coverage_cap),
            ..options
        };
        let bins = options.coverage_cap as usize + 1;
        Self {
            visited: vec![false; reference.contigs.len()],
            reference,
            options,
            depth_limit: u16_or_max(options.coverage_cap),
            pass_floor: options.minimum_base_quality.max(LOWEST_COUNTED_QUALITY),
            excluded_adapter: 0,
            excluded_mapq: 0,
            excluded_duplicate: 0,
            excluded_unpaired: 0,
            excluded_baseq: 0,
            excluded_overlap: 0,
            excluded_capped: 0,
            high_quality_histogram: vec![0; bins],
            unfiltered_histogram: vec![0; bins],
            high_quality_covered: 0,
            unfiltered_covered: 0,
            base_quality_histogram: vec![0; 256],
            quality_lanes: Box::new([[0; 256]; 4]),
            active_tid: None,
            active_contig: 0,
            high_quality: Vec::new(),
            unfiltered: Vec::new(),
            raw: Vec::new(),
            window_base: 0,
            window_offset: 0,
            last_start: None,
            no_call_cursor: 0,
            names: NameWindow::default(),
            seen_bits: Vec::new(),
            read_bases: Vec::new(),
            simulate_accumulation: true,
            accumulator_head: None,
            accumulator_end: 0,
            accumulation_cap_reached: false,
            locus_window: None,
            stop_target: None,
            last_settled_covered: None,
            stop: StopState::Running,
        }
    }

    pub(crate) fn for_run(
        reference: &str,
        interval_masks: Option<BTreeMap<String, Vec<bool>>>,
        options: WgsOptions,
        stop_after: Option<usize>,
    ) -> Result<Self, String> {
        let scan_threads = bgzf_threads_for(HtsThreadRole::RegionWorker).unwrap_or(1);
        let mut reference = WgsReference::load(reference, interval_masks, scan_threads)?;
        let stop_target = stop_after
            .filter(|limit| (*limit as u64) < reference.territory())
            .and_then(|limit| reference.limit_included_loci(limit));
        let mut collector = Self::new(Arc::new(reference), options);
        collector.stop_target = stop_target;
        Ok(collector)
    }

    pub(crate) fn stopped(&self) -> bool {
        self.stop == StopState::Stopped
    }

    fn note_emitted_through(&mut self, bound: usize) {
        if self.stop != StopState::Running {
            return;
        }
        let Some(target) = self.stop_target else {
            return;
        };
        let live = (self.window_base
            ..bound.min(self.window_base + self.raw.len() - self.window_offset))
            .rev()
            .find(|locus| self.raw_at(*locus) != 0);
        if let Some(emitted) = live.or(self.last_settled_covered)
            && (self.active_contig, emitted) >= target
        {
            self.stop = StopState::AwaitingNextPassingRead;
        }
    }

    pub(crate) fn region_worker(&self) -> Self {
        let mut worker = Self::new(Arc::clone(&self.reference), self.options);
        worker.simulate_accumulation = false;
        worker
    }

    pub(crate) fn begin_region(&mut self, start: usize, end: usize) {
        self.leave_contig();
        self.visited.fill(false);
        self.locus_window = Some((start, end));
    }

    pub(crate) fn accumulation_cap_reached(&self) -> bool {
        self.accumulation_cap_reached
    }

    pub(crate) fn has_intervals(&self) -> bool {
        self.reference.has_intervals
    }

    pub(crate) fn drain_counts(&mut self) -> WgsCounts {
        self.finish();
        let bins = self.high_quality_histogram.len();
        WgsCounts {
            excluded: [
                std::mem::take(&mut self.excluded_adapter),
                std::mem::take(&mut self.excluded_mapq),
                std::mem::take(&mut self.excluded_duplicate),
                std::mem::take(&mut self.excluded_unpaired),
                std::mem::take(&mut self.excluded_baseq),
                std::mem::take(&mut self.excluded_overlap),
                std::mem::take(&mut self.excluded_capped),
            ],
            covered: [
                std::mem::take(&mut self.high_quality_covered),
                std::mem::take(&mut self.unfiltered_covered),
            ],
            high_quality: std::mem::replace(&mut self.high_quality_histogram, vec![0; bins]),
            unfiltered: std::mem::replace(&mut self.unfiltered_histogram, vec![0; bins]),
            base_quality: std::mem::replace(&mut self.base_quality_histogram, vec![0; 256]),
        }
    }

    pub(crate) fn absorb_counts(&mut self, counts: &WgsCounts) {
        let [adapter, mapq, duplicate, unpaired, baseq, overlap, capped] = counts.excluded;
        self.excluded_adapter += adapter;
        self.excluded_mapq += mapq;
        self.excluded_duplicate += duplicate;
        self.excluded_unpaired += unpaired;
        self.excluded_baseq += baseq;
        self.excluded_overlap += overlap;
        self.excluded_capped += capped;
        self.high_quality_covered += counts.covered[0];
        self.unfiltered_covered += counts.covered[1];
        add_bins(&mut self.high_quality_histogram, &counts.high_quality);
        add_bins(&mut self.unfiltered_histogram, &counts.unfiltered);
        add_bins(&mut self.base_quality_histogram, &counts.base_quality);
    }

    pub(crate) fn take_accumulation_cap_reached(&mut self) -> bool {
        std::mem::take(&mut self.accumulation_cap_reached)
    }

    pub(crate) fn finish(&mut self) {
        self.leave_contig();
        for quality in 0..256 {
            self.base_quality_histogram[quality] += self
                .quality_lanes
                .iter()
                .map(|lane| lane[quality])
                .sum::<u64>();
        }
        *self.quality_lanes = [[0; 256]; 4];
    }

    fn settle(histogram: &mut [u64], covered: &mut u64, depths: &[u16]) {
        let last = histogram.len() - 1;
        for depth in depths {
            if *depth != 0 {
                histogram[usize::from(*depth).min(last)] += 1;
                *covered += 1;
            }
        }
    }

    fn settle_window(&mut self, count: usize) {
        let range = self.window_offset..self.window_offset + count;
        Self::settle(
            &mut self.high_quality_histogram,
            &mut self.high_quality_covered,
            &self.high_quality[range.clone()],
        );
        Self::settle(
            &mut self.unfiltered_histogram,
            &mut self.unfiltered_covered,
            &self.unfiltered[range],
        );
    }

    fn leave_contig(&mut self) {
        if self.active_tid.is_some() {
            self.note_emitted_through(usize::MAX);
        }
        self.last_settled_covered = None;
        self.settle_window(self.high_quality.len() - self.window_offset);
        self.high_quality.clear();
        self.unfiltered.clear();
        self.raw.clear();
        self.window_offset = 0;
        self.window_base = 0;
        if self.active_tid.take().is_some() {
            self.visited[self.active_contig] = true;
        }
        self.last_start = None;
        self.no_call_cursor = 0;
        self.names.clear();
        self.accumulator_head = None;
        self.accumulator_end = 0;
    }

    fn enter_contig(&mut self, tid: i32, target_names: &[String]) -> Result<(), String> {
        if self.active_tid == Some(tid) {
            return Ok(());
        }
        let Some(name) = target_names.get(usize_or_max(tid)) else {
            return Err("CollectWgsMetrics record references unknown target".to_string());
        };
        let Some(contig) = self.reference.by_name.get(name).copied() else {
            return Err(format!("CollectWgsMetrics reference missing contig {name}"));
        };
        self.leave_contig();
        if self.visited[contig] {
            return Err(format!(
                "CollectWgsMetrics alignment is not coordinate-sorted: contig {name} was revisited"
            ));
        }
        self.active_tid = Some(tid);
        self.active_contig = contig;
        Ok(())
    }

    fn advance_window(&mut self, to: usize) {
        const COMPACT_AFTER: usize = 64 * 1024;
        if to <= self.window_base {
            return;
        }
        let live = self.high_quality.len() - self.window_offset;
        let discard = (to - self.window_base).min(live);
        if self.simulate_accumulation
            && let Some(offset) = self.raw[self.window_offset..self.window_offset + discard]
                .iter()
                .rposition(|depth| *depth != 0)
        {
            self.last_settled_covered = Some(self.window_base + offset);
        }
        self.settle_window(discard);
        self.window_offset += discard;
        self.window_base = to;
        if self.window_offset == self.high_quality.len() {
            self.high_quality.clear();
            self.unfiltered.clear();
            self.raw.clear();
            self.window_offset = 0;
        } else if self.window_offset >= COMPACT_AFTER
            && self.window_offset >= self.high_quality.len() / 2
        {
            let offset = self.window_offset;
            for depths in [&mut self.high_quality, &mut self.unfiltered] {
                let live = depths.len() - offset;
                depths.copy_within(offset.., 0);
                depths.truncate(live);
            }
            let live = self.raw.len() - offset;
            self.raw.copy_within(offset.., 0);
            self.raw.truncate(live);
            self.window_offset = 0;
        }
    }

    fn ensure_window_through(&mut self, end: usize) {
        let required = self.window_offset + end.saturating_sub(self.window_base);
        if required > self.high_quality.len() {
            self.high_quality.resize(required, 0);
            self.unfiltered.resize(required, 0);
            self.raw.resize(required, 0);
        }
    }

    fn raw_at(&self, locus: usize) -> u32 {
        locus
            .checked_sub(self.window_base)
            .and_then(|offset| self.raw.get(self.window_offset + offset))
            .copied()
            .unwrap_or(0)
    }

    fn accumulation_surpassed(&mut self, start: usize) -> bool {
        if let Some(head) = self.accumulator_head
            && head + 1 < start
        {
            let next_head = if self.raw_at(start - 2) != 0 || self.raw_at(start - 1) != 0 {
                start - 1
            } else {
                start
            };
            self.accumulator_head = (next_head < self.accumulator_end).then_some(next_head);
        }
        self.note_emitted_through(self.accumulator_head.unwrap_or(start));
        let surpassed = self
            .accumulator_head
            .is_some_and(|head| self.raw_at(head) >= self.options.locus_accumulation_cap);
        self.accumulation_cap_reached |= surpassed;
        surpassed
    }

    fn read_exclusion(
        &mut self,
        record: &bam::Record,
        flags: u16,
        aligned_bases: u64,
    ) -> Option<ReadExclusion> {
        let unmapped = flags & FLAG_UNMAPPED != 0;
        let mapq = record.mapq();
        if aligned_bases > 0 && (unmapped || mapq == 0) {
            self.read_bases.clear();
            self.read_bases.extend(record.seq().as_bytes());
            if is_adapter_read(&self.read_bases, unmapped, mapq, flags & FLAG_REVERSE != 0) {
                return Some(ReadExclusion::Adapter);
            }
        }
        if mapq < self.options.minimum_mapping_quality {
            Some(ReadExclusion::MappingQuality)
        } else if flags & FLAG_DUPLICATE != 0 {
            Some(ReadExclusion::Duplicate)
        } else if !self.options.count_unpaired
            && (flags & FLAG_PAIRED == 0 || flags & FLAG_MATE_UNMAPPED != 0)
        {
            Some(ReadExclusion::Unpaired)
        } else {
            None
        }
    }

    pub(crate) fn observe(
        &mut self,
        record: &bam::Record,
        target_names: &[String],
    ) -> Result<(), String> {
        let flags = record.flags();
        let tid = record.tid();
        if flags & FLAG_SECONDARY != 0
            || tid < 0
            || (self.options.exclude_supplementary && flags & FLAG_SUPPLEMENTARY != 0)
        {
            return Ok(());
        }
        let start = usize_or_max(record.pos().max(0));
        let cigar = record.raw_cigar();
        let (aligned_bases, reference_span) = cigar_extent(cigar);
        if self.reference.has_intervals {
            let end = start + reference_span.max(1);
            let overlaps = target_names
                .get(usize_or_max(tid))
                .and_then(|name| self.reference.by_name.get(name))
                .and_then(|contig| self.reference.contigs[*contig].interval_runs.as_deref())
                .is_some_and(|runs| {
                    let first = runs.partition_point(|run| run.1 <= start);
                    runs.get(first).is_some_and(|run| run.0 < end)
                });
            if !overlaps {
                return Ok(());
            }
        }
        if let Some(exclusion) = self.read_exclusion(record, flags, aligned_bases) {
            if self
                .locus_window
                .is_none_or(|(lo, hi)| start >= lo && start < hi)
            {
                match exclusion {
                    ReadExclusion::Adapter => self.excluded_adapter += aligned_bases,
                    ReadExclusion::MappingQuality => self.excluded_mapq += aligned_bases,
                    ReadExclusion::Duplicate => self.excluded_duplicate += aligned_bases,
                    ReadExclusion::Unpaired => self.excluded_unpaired += aligned_bases,
                }
            }
            return Ok(());
        }
        if self.stop == StopState::AwaitingNextPassingRead {
            self.stop = StopState::Stopped;
            return Ok(());
        }
        if flags & (FLAG_UNMAPPED | FLAG_FAILS_VENDOR_QUALITY) != 0 {
            return Ok(());
        }
        self.enter_contig(tid, target_names)?;
        self.accumulate(record, start, reference_span)
    }

    fn accumulate(
        &mut self,
        record: &bam::Record,
        start: usize,
        reference_span: usize,
    ) -> Result<(), String> {
        if let Some(previous) = self.last_start
            && start < previous
        {
            return Err(format!(
                "CollectWgsMetrics alignment is not coordinate-sorted: position {start} follows {previous}"
            ));
        }
        self.last_start = Some(start);
        let retained_from = start.saturating_sub(2);
        self.advance_window(retained_from);
        self.names.expire(retained_from);
        if reference_span == 0 {
            return Ok(());
        }
        if self.simulate_accumulation && self.accumulation_surpassed(start) {
            return Ok(());
        }
        let end = start + reference_span;
        let reference = Arc::clone(&self.reference);
        let contig = &reference.contigs[self.active_contig];
        if end > contig.length {
            return Err("CollectWgsMetrics alignment extends beyond reference".to_string());
        }
        self.ensure_window_through(end);
        while contig
            .no_call_runs
            .get(self.no_call_cursor)
            .is_some_and(|run| run.1 <= start)
        {
            self.no_call_cursor += 1;
        }

        let cigar = record.raw_cigar();
        let qualities = hts_io::base_qualities(record);
        let sequence = record.seq();
        let has_no_call = has_no_call_read_base(sequence.encoded);
        let name = record.qname();
        let mut hasher = FxHasher::default();
        hasher.write(name);
        let hash = hasher.finish();
        let mut seen_bits = std::mem::take(&mut self.seen_bits);
        let guard = self
            .names
            .mark_seen(hash, name, start, end, self.pass_floor, &mut seen_bits);
        let read = AlignedRead {
            start,
            qualities,
            encoded_bases: sequence.encoded,
            plain: !qualities.is_empty() && !has_no_call,
            guard,
        };
        let (window_lo, window_hi) = self.locus_window.unwrap_or((0, usize::MAX));
        let mut reference_offset = start;
        let mut read_offset = 0;
        for raw in cigar {
            let length = (raw >> 4) as usize;
            match raw & 0xf {
                0 | 7 | 8 => {
                    let from = self.window_offset + reference_offset - self.window_base;
                    let mut deepest = 0;
                    for depth in &mut self.raw[from..from + length] {
                        *depth += 1;
                        deepest = deepest.max(*depth);
                    }
                    if !self.simulate_accumulation && deepest >= self.options.locus_accumulation_cap
                    {
                        self.accumulation_cap_reached = true;
                    }
                    let lo = reference_offset.max(window_lo);
                    let hi = (reference_offset + length).min(window_hi);
                    if lo < hi {
                        self.observe_block(
                            contig,
                            &read,
                            &seen_bits,
                            lo,
                            hi,
                            read_offset + (lo - reference_offset),
                        );
                    }
                    reference_offset += length;
                    read_offset += length;
                }
                1 | 4 => read_offset += length,
                2 | 3 => reference_offset += length,
                _ => {}
            }
        }
        self.seen_bits = seen_bits;
        self.names.insert(
            hash,
            name,
            start,
            end,
            cigar,
            qualities,
            if has_no_call { sequence.encoded } else { &[] },
        );
        if self.simulate_accumulation {
            if self.accumulator_head.is_none() {
                self.accumulator_head = Some(start);
                self.accumulator_end = end;
            } else {
                self.accumulator_end = self.accumulator_end.max(end);
            }
        }
        Ok(())
    }

    fn observe_block(
        &mut self,
        contig: &WgsContig,
        read: &AlignedRead<'_>,
        seen_bits: &[u64],
        lo: usize,
        hi: usize,
        read_offset: usize,
    ) {
        if let Some(mask) = contig.included.as_deref() {
            self.observe_per_base(read, seen_bits, lo, hi, read_offset, Some(mask));
            return;
        }
        let runs = contig.no_call_runs.as_slice();
        let mut cursor = self.no_call_cursor;
        while runs.get(cursor).is_some_and(|run| run.1 <= lo) {
            cursor += 1;
        }
        let mut position = lo;
        while position < hi {
            let (run_start, run_end) = runs
                .get(cursor)
                .copied()
                .unwrap_or((usize::MAX, usize::MAX));
            let clean_end = hi.min(run_start);
            if position < clean_end {
                self.observe_clean(
                    read,
                    seen_bits,
                    position,
                    clean_end,
                    read_offset + (position - lo),
                );
            }
            if run_start >= hi {
                break;
            }
            position = position.max(run_end);
            cursor += 1;
        }
    }

    fn observe_clean(
        &mut self,
        read: &AlignedRead<'_>,
        seen_bits: &[u64],
        lo: usize,
        hi: usize,
        read_offset: usize,
    ) {
        if !read.plain {
            self.observe_per_base(read, seen_bits, lo, hi, read_offset, None);
            return;
        }
        let (guard_lo, guard_hi) = read
            .guard
            .map_or((hi, hi), |(a, b)| (a.clamp(lo, hi), b.clamp(lo, hi)));
        if lo < guard_lo {
            self.observe_unguarded(read, seen_bits, lo, guard_lo, read_offset);
        }
        if guard_lo < guard_hi {
            self.observe_per_base(
                read,
                seen_bits,
                guard_lo,
                guard_hi,
                read_offset + (guard_lo - lo),
                None,
            );
        }
        if guard_hi < hi {
            self.observe_unguarded(read, seen_bits, guard_hi, hi, read_offset + (guard_hi - lo));
        }
    }

    fn observe_unguarded(
        &mut self,
        read: &AlignedRead<'_>,
        seen_bits: &[u64],
        lo: usize,
        hi: usize,
        read_offset: usize,
    ) {
        let bases = hi - lo;
        let from = self.window_offset + lo - self.window_base;
        let limit = self.depth_limit;
        let unfiltered = &mut self.unfiltered[from..from + bases];
        if unfiltered.iter().any(|depth| *depth >= limit) {
            self.observe_per_base(read, seen_bits, lo, hi, read_offset, None);
            return;
        }
        let qualities = &read.qualities[read_offset..read_offset + bases];
        let high_quality = &mut self.high_quality[from..from + bases];
        let pass_floor = self.pass_floor;
        let mut passed = 0_u32;
        let mut counted = 0_u32;
        for ((unfiltered, high_quality), quality) in unfiltered
            .iter_mut()
            .zip(high_quality.iter_mut())
            .zip(qualities)
        {
            *unfiltered += u16::from(*quality >= LOWEST_COUNTED_QUALITY);
            let pass = *quality >= pass_floor;
            let count = u16::from(pass) & u16::from(*high_quality < limit);
            *high_quality += count;
            passed += u32::from(pass);
            counted += u32::from(count);
        }
        let lanes = &mut *self.quality_lanes;
        let (quads, rest) = qualities.as_chunks::<4>();
        for quad in quads {
            lanes[0][usize::from(quad[0])] += u64::from(quad[0] >= LOWEST_COUNTED_QUALITY);
            lanes[1][usize::from(quad[1])] += u64::from(quad[1] >= LOWEST_COUNTED_QUALITY);
            lanes[2][usize::from(quad[2])] += u64::from(quad[2] >= LOWEST_COUNTED_QUALITY);
            lanes[3][usize::from(quad[3])] += u64::from(quad[3] >= LOWEST_COUNTED_QUALITY);
        }
        for (lane, quality) in rest.iter().enumerate() {
            lanes[lane][usize::from(*quality)] += u64::from(*quality >= LOWEST_COUNTED_QUALITY);
        }
        self.excluded_baseq += bases as u64 - u64::from(passed);
        self.excluded_capped += u64::from(passed - counted);
    }

    fn observe_per_base(
        &mut self,
        read: &AlignedRead<'_>,
        seen_bits: &[u64],
        lo: usize,
        hi: usize,
        read_offset: usize,
        mask: Option<&[bool]>,
    ) {
        let limit = self.depth_limit;
        for index in 0..hi - lo {
            let locus = lo + index;
            if mask.is_some_and(|mask| !mask.get(locus).copied().unwrap_or(false)) {
                continue;
            }
            let read_index = read_offset + index;
            let Some(quality) = read.qualities.get(read_index).copied() else {
                self.excluded_baseq += 1;
                continue;
            };
            if quality < LOWEST_COUNTED_QUALITY {
                self.excluded_baseq += 1;
                continue;
            }
            let slot = self.window_offset + locus - self.window_base;
            if self.unfiltered[slot] < limit {
                self.base_quality_histogram[usize::from(quality)] += 1;
                self.unfiltered[slot] += 1;
            }
            if quality < self.pass_floor || is_no_call_read_base(read.encoded_bases, read_index) {
                self.excluded_baseq += 1;
                continue;
            }
            let bit = locus - read.start;
            if read.guard.is_some()
                && seen_bits
                    .get(bit / 64)
                    .is_some_and(|word| word & (1 << (bit % 64)) != 0)
            {
                self.excluded_overlap += 1;
                continue;
            }
            if self.high_quality[slot] < limit {
                self.high_quality[slot] += 1;
            } else {
                self.excluded_capped += 1;
            }
        }
    }

    pub(crate) fn to_picard_text(&self, sample_size: u32, include_bq_histogram: bool) -> String {
        let territory = self.reference.territory();
        let mut high_quality = self.high_quality_histogram.clone();
        high_quality[0] += territory.saturating_sub(self.high_quality_covered);
        let mut unfiltered = self.unfiltered_histogram.clone();
        unfiltered[0] += territory.saturating_sub(self.unfiltered_covered);
        let histogram = high_quality.as_slice();
        let genome_territory = histogram.iter().sum::<u64>();
        let mean_coverage = mean_from_histogram_u32(histogram);
        let sd_coverage = sample_standard_deviation_from_histogram_u32(histogram, mean_coverage);
        let median_coverage = if genome_territory <= 1 {
            0.0
        } else {
            median_f64_from_histogram_u64(histogram)
        };
        let mad_coverage = if genome_territory <= 1 {
            0.0
        } else {
            mad_f64_from_histogram_u64(histogram, median_coverage)
        };
        let counted = histogram
            .iter()
            .enumerate()
            .map(|(depth, loci)| depth as u64 * loci)
            .sum::<u64>();
        let excluded = self.excluded_adapter
            + self.excluded_mapq
            + self.excluded_duplicate
            + self.excluded_unpaired
            + self.excluded_baseq
            + self.excluded_overlap
            + self.excluded_capped;
        let with_excluded = (counted + excluded) as f64;
        let share = |bases: u64| {
            if with_excluded == 0.0 {
                0.0
            } else {
                bases as f64 / with_excluded
            }
        };
        let quality_bins = &self.base_quality_histogram[..PICARD_QUALITY_BINS];
        let (het_sensitivity, het_q) = if sample_size > 0 && genome_territory > 0 {
            let sensitivity = het_snp_sensitivity(&unfiltered, quality_bins, sample_size as usize);
            (format_float(sensitivity), het_snp_phred(sensitivity))
        } else {
            ("0".to_string(), "0".to_string())
        };

        let mut output = String::new();
        if self.options.exclude_supplementary {
            output.push_str(
                "# turbo-picard EXCLUDE_SUPPLEMENTARY=true: supplementary alignments are not counted; Picard counts them\n",
            );
        }
        output.push_str("## METRICS CLASS\tpicard.analysis.WgsMetrics\n");
        output.push_str("GENOME_TERRITORY\tMEAN_COVERAGE\tSD_COVERAGE\tMEDIAN_COVERAGE\tMAD_COVERAGE\tPCT_EXC_ADAPTER\tPCT_EXC_MAPQ\tPCT_EXC_DUPE\tPCT_EXC_UNPAIRED\tPCT_EXC_BASEQ\tPCT_EXC_OVERLAP\tPCT_EXC_CAPPED\tPCT_EXC_TOTAL\tPCT_1X\tPCT_5X\tPCT_10X\tPCT_15X\tPCT_20X\tPCT_25X\tPCT_30X\tPCT_40X\tPCT_50X\tPCT_60X\tPCT_70X\tPCT_80X\tPCT_90X\tPCT_100X\tFOLD_80_BASE_PENALTY\tFOLD_90_BASE_PENALTY\tFOLD_95_BASE_PENALTY\tHET_SNP_SENSITIVITY\tHET_SNP_Q\n");
        let _ = write!(
            output,
            "{}\t{}\t{}\t{}\t{}",
            genome_territory,
            format_float(mean_coverage),
            if genome_territory < 2 {
                "?".to_string()
            } else {
                format_float(sd_coverage)
            },
            format_float(median_coverage),
            format_float(mad_coverage),
        );
        for bases in [
            self.excluded_adapter,
            self.excluded_mapq,
            self.excluded_duplicate,
            self.excluded_unpaired,
            self.excluded_baseq,
            self.excluded_overlap,
            self.excluded_capped,
            excluded,
        ] {
            let _ = write!(output, "\t{}", format_float(share(bases)));
        }
        for depth in [1, 5, 10, 15, 20, 25, 30, 40, 50, 60, 70, 80, 90, 100] {
            let _ = write!(output, "\t{}", format_float(pct_at_least(histogram, depth)));
        }
        let _ = write!(
            output,
            "\t{}\t{}\t{}\t{}\t{}\n\n",
            fold_base_penalty(histogram, mean_coverage, 80.0),
            fold_base_penalty(histogram, mean_coverage, 90.0),
            fold_base_penalty(histogram, mean_coverage, 95.0),
            het_sensitivity,
            het_q,
        );
        output.push_str("## HISTOGRAM\tjava.lang.Integer\n");
        if include_bq_histogram {
            output.push_str("coverage\thigh_quality_coverage_count\tunfiltered_baseq_count\n");
            for bin in 0..histogram.len().max(PICARD_QUALITY_BINS) {
                let _ = writeln!(
                    output,
                    "{bin}\t{}\t{}",
                    histogram.get(bin).copied().unwrap_or(0),
                    quality_bins.get(bin).copied().unwrap_or(0)
                );
            }
        } else {
            output.push_str("coverage\thigh_quality_coverage_count\n");
            for (coverage, loci) in histogram.iter().enumerate() {
                let _ = writeln!(output, "{coverage}\t{loci}");
            }
        }
        output
    }
}

fn het_snp_phred(sensitivity: f64) -> String {
    let scaled = -10.0 * (1.0 - sensitivity).log10();
    if scaled.is_nan() || scaled == f64::NEG_INFINITY {
        return "0".to_string();
    }
    if scaled == f64::INFINITY {
        return "-1".to_string();
    }
    let low_byte = (scaled + 0.5).floor().rem_euclid(256.0);
    let byte = u8::try_from(f64_to_u64(low_byte)).unwrap_or(0);
    i8::from_ne_bytes([byte]).to_string()
}

struct QualityWheel {
    probabilities: Vec<f64>,
    rejected: u32,
    rng: JavaRandom,
}

impl QualityWheel {
    fn new(weights: &[f64]) -> Self {
        let heaviest = weights.iter().copied().fold(0.0, f64::max);
        Self {
            probabilities: weights.iter().map(|weight| weight / heaviest).collect(),
            rejected: 0,
            rng: JavaRandom::new(51),
        }
    }

    fn draw(&mut self) -> i32 {
        loop {
            let bin = usize_or_max(f64_to_u64(
                self.probabilities.len() as f64 * self.rng.next_double(),
            ));
            self.rejected += 1;
            if self.rng.next_double() < self.probabilities[bin] {
                self.rejected = 0;
                return i32::try_from(bin).unwrap_or(i32::MAX);
            }
            if self.rejected >= 600 {
                self.rejected = 0;
                return 0;
            }
        }
    }
}

fn normalized(histogram: &[u64]) -> Vec<f64> {
    let total = histogram.iter().sum::<u64>() as f64;
    histogram.iter().map(|bin| *bin as f64 / total).collect()
}

fn het_snp_sensitivity(
    unfiltered_depths: &[u64],
    base_qualities: &[u64],
    sample_size: usize,
) -> f64 {
    if base_qualities.iter().all(|bin| *bin == 0) {
        return 0.0;
    }
    let depth_distribution = normalized(unfiltered_depths);
    let depths = depth_distribution.len().min(HET_SENSITIVITY_MAX_DEPTH + 1);
    let mut wheel = QualityWheel::new(&normalized(base_qualities));
    let mut quality_sums = vec![Vec::with_capacity(sample_size); depths];
    for _ in 0..sample_size {
        let mut cumulative = 0_i32;
        for sums in &mut quality_sums {
            sums.push(cumulative);
            cumulative = cumulative.wrapping_add(wheel.draw());
        }
    }
    let log10_of_two = 2.0_f64.log10();
    let thresholds = (0..depths)
        .map(|depth| 10.0 * (depth as f64 * log10_of_two + HET_SENSITIVITY_LOG_ODDS))
        .collect::<Vec<_>>();
    let exceed = quality_sums
        .iter_mut()
        .map(|sums| {
            sums.sort_unstable();
            let mut row = vec![0.0; depths];
            let mut depth = 0;
            let mut rank = 0;
            while depth < depths && rank < sums.len() {
                if thresholds[depth] > f64::from(sums[rank]) {
                    rank += 1;
                } else {
                    row[depth] = (sums.len() - rank) as f64 / sums.len() as f64;
                    depth += 1;
                }
            }
            row
        })
        .collect::<Vec<_>>();
    let mut alt_depths: Vec<Vec<f64>> = Vec::with_capacity(depths);
    for depth in 0..depths {
        let mut row = Vec::with_capacity(depth + 1);
        row.push(0.5_f64.powi(i32::try_from(depth).unwrap_or(i32::MAX)));
        for alt in 1..depth {
            row.push((depth as f64 * 0.5 / alt as f64) * alt_depths[depth - 1][alt - 1]);
        }
        if depth > 0 {
            row.push(row[0]);
        }
        alt_depths.push(row);
    }
    let mut sensitivity = 0.0;
    for depth in 0..depths {
        for alt in 0..=depth {
            sensitivity += depth_distribution[depth] * alt_depths[depth][alt] * exceed[alt][depth];
        }
    }
    sensitivity
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashSet;

    pub(crate) struct Lcg(pub(crate) u64);

    impl Lcg {
        pub(crate) fn next(&mut self, bound: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            usize_or_max(self.0 >> 33) % bound
        }

        fn next_u32(&mut self, bound: usize) -> u32 {
            u32::try_from(self.next(bound)).expect("fixture bound fits")
        }
    }

    #[derive(Debug, Clone)]
    pub(crate) struct FixtureRead {
        pub(crate) name: String,
        pub(crate) tid: i32,
        pub(crate) start: usize,
        pub(crate) flags: u16,
        pub(crate) mapq: u8,
        pub(crate) cigar: Vec<(char, u32)>,
        pub(crate) bases: Vec<u8>,
        pub(crate) qualities: Vec<u8>,
    }

    impl FixtureRead {
        fn blocks(&self) -> Vec<(usize, usize, usize)> {
            let mut blocks = Vec::new();
            let mut reference = self.start;
            let mut read = 0;
            for (op, length) in &self.cigar {
                let length = *length as usize;
                match op {
                    'M' => {
                        blocks.push((reference, read, length));
                        reference += length;
                        read += length;
                    }
                    'I' | 'S' => read += length,
                    'D' | 'N' => reference += length,
                    _ => {}
                }
            }
            blocks
        }

        fn end(&self) -> usize {
            self.start
                + self
                    .cigar
                    .iter()
                    .filter(|(op, _)| matches!(op, 'M' | 'D' | 'N'))
                    .map(|(_, length)| *length as usize)
                    .sum::<usize>()
        }

        pub(crate) fn to_record(&self) -> bam::Record {
            use rust_htslib::bam::record::{Cigar, CigarString};
            let cigar = CigarString(
                self.cigar
                    .iter()
                    .map(|(op, length)| match op {
                        'M' => Cigar::Match(*length),
                        'I' => Cigar::Ins(*length),
                        'D' => Cigar::Del(*length),
                        'N' => Cigar::RefSkip(*length),
                        _ => Cigar::SoftClip(*length),
                    })
                    .collect(),
            );
            let mut record = bam::Record::new();
            record.set(
                self.name.as_bytes(),
                Some(&cigar),
                &self.bases,
                &self.qualities,
            );
            record.set_tid(self.tid);
            record.set_pos(i64::try_from(self.start).expect("fixture position fits"));
            record.set_mapq(self.mapq);
            record.set_flags(self.flags);
            record.set_mtid(if self.flags & FLAG_PAIRED != 0 {
                self.tid
            } else {
                -1
            });
            record.set_mpos(-1);
            record
        }
    }

    pub(crate) const FIXTURE_CONTIGS: [(&str, usize); 3] =
        [("chrA", 6_000), ("chrB", 500), ("chrC", 2_500)];

    pub(crate) fn fixture_reference_text(seed: u64) -> Vec<Vec<u8>> {
        let mut rng = Lcg(seed ^ 0x5eed);
        FIXTURE_CONTIGS
            .iter()
            .map(|(_, length)| {
                let mut bases = vec![b'A'; *length];
                for _ in 0..4 {
                    let start = rng.next(*length);
                    let run = 1 + rng.next(90);
                    let end = (start + run).min(*length);
                    let fill = if rng.next(3) == 0 { b'n' } else { b'N' };
                    bases[start..end].fill(fill);
                }
                bases
            })
            .collect()
    }

    fn fixture_segment(
        rng: &mut Lcg,
        name: &str,
        tid: i32,
        start: usize,
        contig_length: usize,
        base_flags: u16,
    ) -> Option<FixtureRead> {
        let mut cigar = Vec::new();
        let mut read_length = 0;
        if rng.next(6) == 0 {
            let clip = 1 + rng.next_u32(6);
            cigar.push(('S', clip));
            read_length += clip as usize;
        }
        let blocks = 1 + rng.next(3);
        for block in 0..blocks {
            let matched = 15 + rng.next_u32(50);
            cigar.push(('M', matched));
            read_length += matched as usize;
            if block + 1 < blocks {
                let gap = 1 + rng.next_u32(5);
                match rng.next(3) {
                    0 => {
                        cigar.push(('I', gap));
                        read_length += gap as usize;
                    }
                    1 => cigar.push(('D', gap)),
                    _ => cigar.push(('N', gap + 20)),
                }
            }
        }
        let mut flags = base_flags;
        if rng.next(9) == 0 {
            flags |= FLAG_DUPLICATE;
        }
        if rng.next(31) == 0 {
            flags |= FLAG_FAILS_VENDOR_QUALITY;
        }
        if rng.next(29) == 0 {
            flags |= FLAG_SECONDARY;
        }
        if rng.next(2) == 0 {
            flags |= FLAG_REVERSE;
        }
        let read = FixtureRead {
            name: name.to_string(),
            tid,
            start,
            flags,
            mapq: [0, 5, 20, 60, 60, 60][rng.next(6)],
            cigar,
            bases: (0..read_length)
                .map(|_| {
                    if rng.next(40) == 0 {
                        b'N'
                    } else {
                        b"ACGT"[rng.next(4)]
                    }
                })
                .collect(),
            qualities: (0..read_length)
                .map(|_| [0_u8, 2, 3, 11, 19, 20, 30, 37][rng.next(8)])
                .collect(),
        };
        (read.end() <= contig_length).then_some(read)
    }

    pub(crate) fn fixture_reads(seed: u64, depth_scale: usize) -> Vec<FixtureRead> {
        let mut rng = Lcg(seed);
        let mut keyed = Vec::new();
        let mut serial = 0_u64;
        for (tid, (_, length)) in FIXTURE_CONTIGS.iter().enumerate() {
            let tid = i32::try_from(tid).expect("fixture tid fits");
            let templates = *length * depth_scale / 60;
            for template in 0..templates {
                let name = format!("t{tid}_{template}");
                let anchor = rng.next(*length);
                let segments = match rng.next(10) {
                    0 => 1,
                    1..=6 => 2,
                    7 | 8 => 3,
                    _ => 4,
                };
                for segment in 0..segments {
                    let start = anchor + rng.next(120);
                    let mut flags = 0;
                    if segments > 1 {
                        flags |= FLAG_PAIRED;
                        if rng.next(17) == 0 {
                            flags |= FLAG_MATE_UNMAPPED;
                        }
                    }
                    if segment >= 2 {
                        flags |= FLAG_SUPPLEMENTARY;
                    }
                    if let Some(read) = fixture_segment(&mut rng, &name, tid, start, *length, flags)
                    {
                        keyed.push(((tid, read.start, serial), read));
                        serial += 1;
                    }
                }
            }
        }
        keyed.sort_by_key(|(key, _)| *key);
        keyed.into_iter().map(|(_, read)| read).collect()
    }

    pub(crate) fn fixture_reference(
        seed: u64,
        interval_masks: Option<BTreeMap<String, Vec<bool>>>,
    ) -> WgsReference {
        let text = fixture_reference_text(seed);
        let lengths = FIXTURE_CONTIGS
            .iter()
            .map(|(name, length)| ((*name).to_string(), *length))
            .collect::<Vec<_>>();
        let runs = text
            .iter()
            .map(|bases| {
                let mut runs = Vec::new();
                append_no_call_runs(bases, 0, &mut runs);
                runs
            })
            .collect();
        WgsReference::from_parts(lengths, runs, interval_masks)
    }

    pub(crate) fn fixture_target_names() -> Vec<String> {
        FIXTURE_CONTIGS
            .iter()
            .map(|(name, _)| (*name).to_string())
            .collect()
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct Tally {
        excluded: [u64; 7],
        high_quality: Vec<u64>,
        unfiltered: Vec<u64>,
        base_qualities: Vec<u64>,
    }

    fn collector_tally(collector: &WgsCollector) -> Tally {
        let territory = collector.reference.territory();
        let mut high_quality = collector.high_quality_histogram.clone();
        high_quality[0] += territory - collector.high_quality_covered;
        let mut unfiltered = collector.unfiltered_histogram.clone();
        unfiltered[0] += territory - collector.unfiltered_covered;
        Tally {
            excluded: [
                collector.excluded_adapter,
                collector.excluded_mapq,
                collector.excluded_duplicate,
                collector.excluded_unpaired,
                collector.excluded_baseq,
                collector.excluded_overlap,
                collector.excluded_capped,
            ],
            high_quality,
            unfiltered,
            base_qualities: collector.base_quality_histogram.clone(),
        }
    }

    fn locus_by_locus_tally(
        reads: &[FixtureRead],
        reference_text: &[Vec<u8>],
        intervals: Option<&BTreeMap<String, Vec<bool>>>,
        options: WgsOptions,
    ) -> Tally {
        let cap = options.coverage_cap as usize;
        let accumulation_cap = options.locus_accumulation_cap.max(options.coverage_cap) as usize;
        let mut tally = Tally {
            high_quality: vec![0; cap + 1],
            unfiltered: vec![0; cap + 1],
            base_qualities: vec![0; 256],
            ..Tally::default()
        };
        for (tid, (contig_name, length)) in FIXTURE_CONTIGS.iter().enumerate() {
            let interval_mask = intervals.map(|masks| &masks[*contig_name]);
            let mut pileups: Vec<Vec<(usize, usize)>> = vec![Vec::new(); *length];
            let mut head: Option<usize> = None;
            let mut extent = 0;
            for (read_index, read) in reads.iter().enumerate() {
                if read.tid != i32::try_from(tid).expect("fixture tid fits")
                    || read.flags & FLAG_SECONDARY != 0
                    || (options.exclude_supplementary && read.flags & FLAG_SUPPLEMENTARY != 0)
                {
                    continue;
                }
                if let Some(mask) = interval_mask
                    && !(read.start..read.end().max(read.start + 1))
                        .any(|locus| mask.get(locus).copied().unwrap_or(false))
                {
                    continue;
                }
                let aligned = read
                    .blocks()
                    .iter()
                    .map(|(_, _, length)| *length as u64)
                    .sum::<u64>();
                let unmapped = read.flags & FLAG_UNMAPPED != 0;
                if (unmapped || read.mapq == 0)
                    && is_adapter_read(
                        &read.bases,
                        unmapped,
                        read.mapq,
                        read.flags & FLAG_REVERSE != 0,
                    )
                {
                    tally.excluded[0] += aligned;
                    continue;
                }
                if read.mapq < options.minimum_mapping_quality {
                    tally.excluded[1] += aligned;
                    continue;
                }
                if read.flags & FLAG_DUPLICATE != 0 {
                    tally.excluded[2] += aligned;
                    continue;
                }
                if !options.count_unpaired
                    && (read.flags & FLAG_PAIRED == 0 || read.flags & FLAG_MATE_UNMAPPED != 0)
                {
                    tally.excluded[3] += aligned;
                    continue;
                }
                if unmapped || read.flags & FLAG_FAILS_VENDOR_QUALITY != 0 {
                    continue;
                }
                let start = read.start;
                while let Some(position) = head {
                    if position + 1 >= start {
                        break;
                    }
                    let mut first_covered = position;
                    while first_covered < extent
                        && first_covered < start
                        && pileups[first_covered].is_empty()
                    {
                        first_covered += 1;
                    }
                    head = if first_covered >= extent {
                        None
                    } else if first_covered + 1 >= start {
                        Some(first_covered)
                    } else {
                        (first_covered + 1 < extent).then_some(first_covered + 1)
                    };
                    if first_covered + 1 >= start {
                        break;
                    }
                }
                if head.is_some_and(|position| pileups[position].len() >= accumulation_cap) {
                    continue;
                }
                if read.end() == read.start {
                    continue;
                }
                if head.is_none() {
                    head = Some(start);
                    extent = read.end();
                } else {
                    extent = extent.max(read.end());
                }
                for (reference, offset, block) in read.blocks() {
                    for index in 0..block {
                        pileups[reference + index].push((read_index, offset + index));
                    }
                }
            }
            for (locus, pileup) in pileups.iter().enumerate() {
                if is_no_call_reference_base(reference_text[tid][locus])
                    || interval_mask.is_some_and(|mask| !mask[locus])
                {
                    continue;
                }
                let mut names = HashSet::new();
                let mut pileup_size = 0;
                let mut unfiltered_depth = 0;
                for (read_index, offset) in pileup {
                    let read = &reads[*read_index];
                    let quality = read.qualities[*offset];
                    if quality <= 2 {
                        tally.excluded[4] += 1;
                        continue;
                    }
                    if unfiltered_depth < cap {
                        tally.base_qualities[usize::from(quality)] += 1;
                        unfiltered_depth += 1;
                    }
                    if quality < options.minimum_base_quality || read.bases[*offset] == b'N' {
                        tally.excluded[4] += 1;
                        continue;
                    }
                    if !names.insert(read.name.as_str()) {
                        tally.excluded[5] += 1;
                        continue;
                    }
                    pileup_size += 1;
                }
                let high_quality_depth = pileup_size.min(cap);
                tally.excluded[6] += (pileup_size - high_quality_depth) as u64;
                tally.high_quality[high_quality_depth] += 1;
                tally.unfiltered[unfiltered_depth] += 1;
            }
        }
        tally
    }

    fn option_sets() -> Vec<WgsOptions> {
        let base = WgsOptions {
            minimum_mapping_quality: 20,
            minimum_base_quality: 20,
            coverage_cap: 250,
            locus_accumulation_cap: 100_000,
            count_unpaired: false,
            exclude_supplementary: false,
        };
        vec![
            base,
            WgsOptions {
                count_unpaired: true,
                ..base
            },
            WgsOptions {
                coverage_cap: 3,
                count_unpaired: true,
                ..base
            },
            WgsOptions {
                coverage_cap: 2,
                locus_accumulation_cap: 4,
                count_unpaired: true,
                ..base
            },
            WgsOptions {
                coverage_cap: 5,
                locus_accumulation_cap: 7,
                minimum_mapping_quality: 0,
                minimum_base_quality: 0,
                ..base
            },
            WgsOptions {
                coverage_cap: 0,
                locus_accumulation_cap: 1,
                count_unpaired: true,
                ..base
            },
            WgsOptions {
                exclude_supplementary: true,
                ..base
            },
        ]
    }

    fn interval_fixture() -> BTreeMap<String, Vec<bool>> {
        let mut rng = Lcg(77);
        FIXTURE_CONTIGS
            .iter()
            .map(|(name, length)| {
                let mut mask = vec![false; *length];
                for _ in 0..3 {
                    let start = rng.next(*length);
                    let end = (start + 1 + rng.next(400)).min(*length);
                    mask[start..end].fill(true);
                }
                ((*name).to_string(), mask)
            })
            .collect()
    }

    #[test]
    fn collector_matches_a_locus_by_locus_pileup() {
        let target_names = fixture_target_names();
        for (seed, depth_scale) in [(1, 3), (2, 12), (3, 40)] {
            let reads = fixture_reads(seed, depth_scale);
            let reference_text = fixture_reference_text(seed);
            for intervals in [None, Some(interval_fixture())] {
                for options in option_sets() {
                    let reference = Arc::new(fixture_reference(seed, intervals.clone()));
                    let mut collector = WgsCollector::new(reference, options);
                    for read in &reads {
                        collector
                            .observe(&read.to_record(), &target_names)
                            .expect("fixture read is accepted");
                    }
                    collector.finish();
                    assert_eq!(
                        collector_tally(&collector),
                        locus_by_locus_tally(&reads, &reference_text, intervals.as_ref(), options),
                        "seed {seed} intervals {} options {options:?}",
                        intervals.is_some()
                    );
                }
            }
        }
    }

    #[test]
    fn stop_after_keeps_the_first_called_loci_in_reference_order() {
        let mut reference = fixture_reference(4, None);
        let first_contig = reference.contigs[0].included_loci;
        let last_kept = reference.limit_included_loci(first_contig + 10);
        assert_eq!(last_kept.map(|(contig, _)| contig), Some(1));
        assert_eq!(reference.territory(), first_contig as u64 + 10);
        assert_eq!(reference.contigs[1].included_loci, 10);
        assert_eq!(reference.contigs[2].included_loci, 0);
        let kept = reference.contigs[1]
            .included
            .as_ref()
            .expect("the cut contig has a mask");
        assert_eq!(kept.iter().filter(|included| **included).count(), 10);
    }

    #[test]
    fn het_snp_phred_follows_the_java_byte_conversion() {
        assert_eq!(het_snp_phred(0.0), "0");
        assert_eq!(het_snp_phred(0.5), "3");
        assert_eq!(het_snp_phred(0.554_231), "4");
        assert_eq!(het_snp_phred(1.0), "-1");
        assert_eq!(het_snp_phred(1.0 - 1e-13), "-126");
        assert_eq!(het_snp_phred(f64::NAN), "0");
    }

    #[test]
    fn no_call_runs_join_across_chunks() {
        let mut runs = Vec::new();
        append_no_call_runs(b"ACNN", 0, &mut runs);
        append_no_call_runs(b"nnA.", 4, &mut runs);
        assert_eq!(runs, vec![(2, 6), (7, 8)]);
    }
}
