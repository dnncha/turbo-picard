use std::thread;
use turbo_picard_core::bgzf_threads::{HtsThreadRole, bgzf_threads_for};

const LCG_MULTIPLIER: u64 = 0x5_DEEC_E66D;
const LCG_ADDEND: u64 = 0xB;
const LCG_MASK: u64 = (1_u64 << 48) - 1;
const REJECTION_LIMIT: u64 = 600;
const PICARD_WHEEL_SEED: u64 = 51;
const HET_SENSITIVITY_MAX_DEPTH: usize = 1000;
const HET_SENSITIVITY_LOG_ODDS: f64 = 3.0;
const INV_TWO_POW_53: f64 = 1.0 / ((1_u64 << 53) as f64);

#[inline]
const fn initial_seed(seed: u64) -> u64 {
    (seed ^ LCG_MULTIPLIER) & LCG_MASK
}

#[inline]
const fn lcg_step(seed: u64) -> u64 {
    seed.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_ADDEND) & LCG_MASK
}

#[inline]
const fn lcg_compose(first: (u64, u64), second: (u64, u64)) -> (u64, u64) {
    let (a1, c1) = first;
    let (a2, c2) = second;
    (
        a1.wrapping_mul(a2) & LCG_MASK,
        c1.wrapping_mul(a2).wrapping_add(c2) & LCG_MASK,
    )
}

const fn lcg_pow(mut steps: u64) -> (u64, u64) {
    let mut acc = (1_u64, 0_u64);
    let mut base = (LCG_MULTIPLIER, LCG_ADDEND);
    while steps > 0 {
        if steps & 1 != 0 {
            acc = lcg_compose(acc, base);
        }
        steps >>= 1;
        if steps > 0 {
            base = lcg_compose(base, base);
        }
    }
    acc
}

#[inline]
const fn lcg_apply(seed: u64, (a, c): (u64, u64)) -> u64 {
    seed.wrapping_mul(a).wrapping_add(c) & LCG_MASK
}

#[inline]
fn double_from_seeds(first: u64, second: u64) -> f64 {
    let high = first >> 22;
    let low = second >> 21;
    ((high << 27) + low) as f64 * INV_TWO_POW_53
}

#[inline]
fn roulette_trial_exact(seed0: u64, probs: &[f64]) -> Option<u16> {
    let s1 = lcg_step(seed0);
    let s2 = lcg_step(s1);
    let u1 = double_from_seeds(s1, s2);
    let raw_bin = (probs.len() as f64 * u1).floor().max(0.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "u1 < 1.0 guarantees 0 <= probs.len() * u1 < probs.len() <= u16::MAX"
    )]
    let bin = (raw_bin as usize).min(probs.len().saturating_sub(1));
    let p = probs[bin];
    if p == 0.0 {
        return None;
    }
    if p >= 1.0 {
        return u16::try_from(bin).ok();
    }
    let s3 = lcg_step(s2);
    let s4 = lcg_step(s3);
    (double_from_seeds(s3, s4) < p)
        .then(|| u16::try_from(bin).ok())
        .flatten()
}

const STEP3: (u64, u64) = lcg_pow(3);
const STEP4: (u64, u64) = lcg_pow(4);
const STEP32: (u64, u64) = lcg_pow(32);

fn prob_to_thresh53(p: f64) -> u64 {
    if p <= 0.0 {
        0
    } else if p >= 1.0 {
        1_u64 << 53
    } else {
        let scaled = (p * ((1_u64 << 53) as f64)).ceil();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "0 < p < 1 guarantees 0 <= scaled <= 2^53"
        )]
        let val = (scaled as u64).min(1_u64 << 53);
        val
    }
}

#[inline]
fn eval_eight_lanes(seeds: &mut [u64; 8], probs: &[f64], thresh53: &[u64]) -> (u8, [u16; 8]) {
    let n = probs.len() as u64;
    let max_bin = probs.len().saturating_sub(1);
    let mut bins = [0_u16; 8];
    let mut ts = [0_u64; 8];
    let mut edge_mask = 0_u8;
    for lane in 0..8 {
        let s1 = lcg_step(seeds[lane]);
        let prod = (s1 >> 22).saturating_mul(n);
        let rem = prod & ((1_u64 << 26) - 1);
        let fast_ok = rem.saturating_add(n) <= (1_u64 << 26) - 4;
        edge_mask |= u8::from(!fast_ok) << lane;
        let bin = usize::try_from(prod >> 26).unwrap_or(0).min(max_bin);
        bins[lane] = u16::try_from(bin).unwrap_or(0);
        ts[lane] = thresh53[bin];
    }
    if (ts[0] | ts[1] | ts[2] | ts[3] | ts[4] | ts[5] | ts[6] | ts[7]) == 0 && edge_mask == 0 {
        for seed in seeds.iter_mut() {
            *seed = lcg_apply(*seed, STEP32);
        }
        return (0, bins);
    }
    let mut hit_mask = 0_u8;
    for lane in 0..8 {
        let seed = seeds[lane];
        let s3 = lcg_apply(seed, STEP3);
        let s4 = lcg_apply(seed, STEP4);
        let u2_int = ((s3 >> 22) << 27) + (s4 >> 21);
        let mut is_hit = u2_int < ts[lane];
        if edge_mask & (1_u8 << lane) != 0 {
            if let Some(exact_bin) = roulette_trial_exact(seed, probs) {
                bins[lane] = exact_bin;
                is_hit = true;
            } else {
                is_hit = false;
            }
        }
        hit_mask |= u8::from(is_hit) << lane;
        seeds[lane] = lcg_apply(seed, STEP32);
    }
    (hit_mask, bins)
}

fn scan_trials_chunk(
    probs: &[f64],
    thresh53: &[u64],
    start_trial: u64,
    trial_count: u64,
    expected_hits: usize,
) -> Vec<(u64, u16)> {
    let mut seeds = [0_u64; 8];
    let mut cur = lcg_apply(initial_seed(PICARD_WHEEL_SEED), lcg_pow(start_trial * 4));
    for slot in &mut seeds {
        *slot = cur;
        cur = lcg_apply(cur, STEP4);
    }

    let mut hits = Vec::with_capacity(expected_hits.max(64));
    let full_blocks = trial_count / 8;
    let mut base_t = start_trial;
    for _ in 0..full_blocks {
        let (mut hit_mask, bins) = eval_eight_lanes(&mut seeds, probs, thresh53);
        while hit_mask != 0 {
            let lane = usize::try_from(hit_mask.trailing_zeros()).unwrap_or(0);
            hit_mask &= hit_mask - 1;
            hits.push((base_t + lane as u64, bins[lane]));
        }
        base_t += 8;
    }

    let rem = usize::try_from(trial_count % 8).unwrap_or(0);
    for (lane, seed) in seeds.iter().copied().enumerate().take(rem) {
        if let Some(bin) = roulette_trial_exact(seed, probs) {
            hits.push((base_t + lane as u64, bin));
        }
    }
    hits
}

fn finish_draws_sequential(
    probs: &[f64],
    thresh53: &[u64],
    start_trial: u64,
    mut rejected: u64,
    needed_draws: usize,
    emitted: &mut usize,
    on_draw: &mut impl FnMut(u16),
) {
    let mut seeds = [0_u64; 8];
    let mut cur = lcg_apply(initial_seed(PICARD_WHEEL_SEED), lcg_pow(start_trial * 4));
    for slot in &mut seeds {
        *slot = cur;
        cur = lcg_apply(cur, STEP4);
    }

    while *emitted < needed_draws {
        let (hit_mask, bins) = eval_eight_lanes(&mut seeds, probs, thresh53);
        if hit_mask == 0 {
            rejected += 8;
            while rejected >= REJECTION_LIMIT && *emitted < needed_draws {
                on_draw(0);
                *emitted += 1;
                rejected -= REJECTION_LIMIT;
            }
            continue;
        }
        let mut cursor_lane = 0_u64;
        let mut mask = hit_mask;
        while mask != 0 && *emitted < needed_draws {
            let lane = u64::from(mask.trailing_zeros());
            mask &= mask - 1;
            rejected += lane - cursor_lane;
            while rejected >= REJECTION_LIMIT && *emitted < needed_draws {
                on_draw(0);
                *emitted += 1;
                rejected -= REJECTION_LIMIT;
            }
            if *emitted < needed_draws {
                on_draw(bins[usize::try_from(lane).unwrap_or(0)]);
                *emitted += 1;
            }
            rejected = 0;
            cursor_lane = lane + 1;
        }
        rejected += 8 - cursor_lane;
        while rejected >= REJECTION_LIMIT && *emitted < needed_draws {
            on_draw(0);
            *emitted += 1;
            rejected -= REJECTION_LIMIT;
        }
    }
}

pub(crate) fn for_each_roulette_draw(probs: &[f64], needed_draws: usize, on_draw: impl FnMut(u16)) {
    if needed_draws == 0 || probs.is_empty() {
        return;
    }
    for_each_roulette_draw_with_workers(
        probs,
        needed_draws,
        bgzf_threads_for(HtsThreadRole::RegionWorker)
            .unwrap_or(1)
            .max(1),
        on_draw,
    );
}

fn for_each_roulette_draw_with_workers(
    probs: &[f64],
    needed_draws: usize,
    workers: usize,
    mut on_draw: impl FnMut(u16),
) {
    if needed_draws == 0 || probs.is_empty() {
        return;
    }
    let thresh53 = probs
        .iter()
        .copied()
        .map(prob_to_thresh53)
        .collect::<Vec<_>>();
    let avg_p = probs.iter().sum::<f64>() / probs.len() as f64;
    let effective_rate = avg_p.max(1.0 / REJECTION_LIMIT as f64);
    let est_trials = needed_draws as f64 / effective_rate;

    let mut emitted = 0_usize;
    if workers <= 1 || est_trials < 1_000_000.0 {
        finish_draws_sequential(
            probs,
            &thresh53,
            0,
            0,
            needed_draws,
            &mut emitted,
            &mut on_draw,
        );
        return;
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "est_trials is finite positive and bounded well below u64::MAX"
    )]
    let total_scan = ((est_trials * 1.08).ceil() as u64).max(8192);
    let active_workers = workers.min(usize::try_from(total_scan / 1024).unwrap_or(1).max(1));
    if active_workers <= 1 {
        finish_draws_sequential(
            probs,
            &thresh53,
            0,
            0,
            needed_draws,
            &mut emitted,
            &mut on_draw,
        );
        return;
    }

    let chunk_size = total_scan.div_ceil(active_workers as u64);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "expected_hits per chunk is positive and fits usize"
    )]
    let expected_hits = ((chunk_size as f64 * effective_rate * 1.15) as usize).max(64);
    let thresh53_ref = thresh53.as_slice();
    let chunks = thread::scope(|scope| {
        let handles = (0..active_workers)
            .map(|worker_idx| {
                let start = worker_idx as u64 * chunk_size;
                let end = (start + chunk_size).min(total_scan);
                let count = end.saturating_sub(start);
                scope.spawn(move || {
                    scan_trials_chunk(probs, thresh53_ref, start, count, expected_hits)
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or_default())
            .collect::<Vec<_>>()
    });

    let mut prev_t = 0_u64;
    'outer: for chunk in chunks {
        for (t, bin) in chunk {
            let mut gap = t - prev_t;
            while gap >= REJECTION_LIMIT {
                on_draw(0);
                emitted += 1;
                gap -= REJECTION_LIMIT;
                if emitted == needed_draws {
                    break 'outer;
                }
            }
            on_draw(bin);
            emitted += 1;
            prev_t = t + 1;
            if emitted == needed_draws {
                break 'outer;
            }
        }
    }

    if emitted < needed_draws {
        let mut tail = total_scan.saturating_sub(prev_t);
        while tail >= REJECTION_LIMIT && emitted < needed_draws {
            on_draw(0);
            emitted += 1;
            tail -= REJECTION_LIMIT;
        }
        if emitted < needed_draws {
            finish_draws_sequential(
                probs,
                &thresh53,
                total_scan,
                tail,
                needed_draws,
                &mut emitted,
                &mut on_draw,
            );
        }
    }
}

pub(crate) fn wgs_het_snp_sensitivity(
    unfiltered_depths: &[u64],
    base_qualities: &[u64],
    sample_size: usize,
) -> f64 {
    let heaviest = base_qualities.iter().copied().max().unwrap_or(0);
    if heaviest == 0 {
        return 0.0;
    }
    let total_bq = base_qualities.iter().sum::<u64>() as f64;
    let weights = base_qualities
        .iter()
        .map(|&bin| bin as f64 / total_bq)
        .collect::<Vec<_>>();
    let heaviest_weight = weights.iter().copied().fold(0.0_f64, f64::max);
    let probs = weights
        .into_iter()
        .map(|weight| weight / heaviest_weight)
        .collect::<Vec<_>>();

    let total_depths = unfiltered_depths.iter().sum::<u64>() as f64;
    if total_depths == 0.0 || sample_size == 0 {
        return 0.0;
    }
    let depth_distribution = unfiltered_depths
        .iter()
        .map(|&bin| bin as f64 / total_depths)
        .collect::<Vec<_>>();
    let depths = depth_distribution.len().min(HET_SENSITIVITY_MAX_DEPTH + 1);
    let Some(max_active_depth) = depth_distribution[..depths].iter().rposition(|&p| p > 0.0) else {
        return 0.0;
    };

    let log10_of_two = 2.0_f64.log10();
    let thresholds = (0..depths)
        .map(|depth| 10.0 * (depth as f64 * log10_of_two + HET_SENSITIVITY_LOG_ODDS))
        .collect::<Vec<_>>();
    let active_depths = max_active_depth + 1;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "threshold for depth <= 1000 is positive and <= 3041"
    )]
    let max_t = thresholds[max_active_depth].ceil() as usize;
    let stride = max_t + 1;
    let mut sum_hist = vec![0_u32; active_depths * stride];

    let needed_draws = sample_size.saturating_mul(depths);
    let mut sample_alt = 0_usize;
    let mut cumulative = 0_i32;

    for_each_roulette_draw(&probs, needed_draws, |bin| {
        if sample_alt < active_depths {
            let capped = usize::try_from(cumulative.max(0)).unwrap_or(0).min(max_t);
            sum_hist[sample_alt * stride + capped] += 1;
        }
        cumulative = cumulative.wrapping_add(i32::from(bin));
        sample_alt += 1;
        if sample_alt == depths {
            sample_alt = 0;
            cumulative = 0;
        }
    });

    let inv_samples = 1.0 / sample_size as f64;
    let mut exceed = vec![0.0_f64; active_depths * active_depths];
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "thresholds for depth <= max_active_depth are positive and <= max_t"
    )]
    let int_thresholds = thresholds[..active_depths]
        .iter()
        .map(|t| t.ceil() as usize)
        .collect::<Vec<_>>();

    for alt in 0..active_depths {
        let row_offset = alt * stride;
        let mut suffix = 0_u32;
        let mut t_val = max_t;
        for d_idx in (0..=max_active_depth).rev() {
            let thresh = int_thresholds[d_idx];
            while t_val >= thresh {
                suffix += sum_hist[row_offset + t_val];
                if t_val == 0 {
                    break;
                }
                t_val -= 1;
            }
            exceed[alt * active_depths + d_idx] = f64::from(suffix) * inv_samples;
        }
    }

    let mut alt_depths: Vec<Vec<f64>> = Vec::with_capacity(active_depths);
    for depth in 0..active_depths {
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
    for depth in 0..active_depths {
        let depth_p = depth_distribution[depth];
        if depth_p == 0.0 {
            continue;
        }
        for (alt, &alt_p) in alt_depths[depth].iter().enumerate() {
            sensitivity += depth_p * alt_p * exceed[alt * active_depths + depth];
        }
    }
    sensitivity
}

pub(crate) fn hs_called_proportions(
    iterations: usize,
    sample_size: usize,
    quality_histogram: &[u64],
) -> Vec<f64> {
    if sample_size == 0 || iterations == 0 {
        return vec![0.0; iterations];
    }
    let last_non_zero = quality_histogram
        .iter()
        .rposition(|count| *count > 0)
        .map_or(1, |index| index + 1);
    let histogram = &quality_histogram[..last_non_zero];
    let max = histogram.iter().copied().max().unwrap_or(0) as f64;
    let probs = if max == 0.0 {
        vec![1.0]
    } else {
        histogram.iter().map(|count| *count as f64 / max).collect()
    };

    let thresholds = (0..iterations)
        .map(|depth| 10.0 * (depth as f64 * 2.0_f64.log10() + 3.0))
        .collect::<Vec<_>>();
    let mut called_counts = vec![0_usize; iterations];

    let needed_draws = sample_size.saturating_mul(iterations);
    let mut index = 0_usize;
    let mut sum = 0_u32;
    for_each_roulette_draw(&probs, needed_draws, |bin| {
        if f64::from(sum) >= thresholds[index] {
            called_counts[index] += 1;
        }
        sum = sum.saturating_add(u32::from(bin));
        index += 1;
        if index == iterations {
            index = 0;
            sum = 0;
        }
    });

    called_counts
        .into_iter()
        .map(|count| count as f64 / sample_size as f64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RefJavaRandom {
        seed: u64,
    }

    impl RefJavaRandom {
        fn new(seed: u64) -> Self {
            Self {
                seed: (seed ^ LCG_MULTIPLIER) & LCG_MASK,
            }
        }

        fn next_bits(&mut self, bits: u32) -> u32 {
            self.seed = self
                .seed
                .wrapping_mul(LCG_MULTIPLIER)
                .wrapping_add(LCG_ADDEND)
                & LCG_MASK;
            u32::try_from(self.seed >> (48 - bits)).unwrap_or(0)
        }

        fn next_double(&mut self) -> f64 {
            let high = u64::from(self.next_bits(26));
            let low = u64::from(self.next_bits(27));
            ((high << 27) + low) as f64 / (1_u64 << 53) as f64
        }
    }

    fn ref_draws(probs: &[f64], count: usize) -> Vec<u16> {
        let mut rng = RefJavaRandom::new(PICARD_WHEEL_SEED);
        let mut rejected = 0_u64;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            loop {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "test reference mirrors exact float multiplication"
                )]
                let bin = (probs.len() as f64 * rng.next_double()) as usize;
                rejected += 1;
                if rng.next_double() < probs[bin] {
                    rejected = 0;
                    out.push(u16::try_from(bin).unwrap_or(0));
                    break;
                }
                if rejected >= REJECTION_LIMIT {
                    rejected = 0;
                    out.push(0);
                    break;
                }
            }
        }
        out
    }

    fn ref_wgs_het_snp(unfiltered: &[u64], base_qualities: &[u64], sample_size: usize) -> f64 {
        let heaviest_q = base_qualities.iter().copied().max().unwrap_or(0);
        if heaviest_q == 0 {
            return 0.0;
        }
        let total_bq = base_qualities.iter().sum::<u64>() as f64;
        let weights = base_qualities
            .iter()
            .map(|&b| b as f64 / total_bq)
            .collect::<Vec<_>>();
        let max_w = weights.iter().copied().fold(0.0, f64::max);
        let probs = weights.iter().map(|&w| w / max_w).collect::<Vec<_>>();
        let total_d = unfiltered.iter().sum::<u64>() as f64;
        let depth_dist = unfiltered
            .iter()
            .map(|&b| b as f64 / total_d)
            .collect::<Vec<_>>();
        let depths = depth_dist.len().min(HET_SENSITIVITY_MAX_DEPTH + 1);
        let draws = ref_draws(&probs, sample_size * depths);
        let mut sums = vec![Vec::with_capacity(sample_size); depths];
        let mut it = draws.into_iter();
        for _ in 0..sample_size {
            let mut cum = 0_i32;
            for row in &mut sums {
                row.push(cum);
                cum += i32::from(it.next().unwrap_or(0));
            }
        }
        let log10_two = 2.0_f64.log10();
        let thresholds = (0..depths)
            .map(|d| 10.0 * (d as f64 * log10_two + HET_SENSITIVITY_LOG_ODDS))
            .collect::<Vec<_>>();
        let exceed = sums
            .iter_mut()
            .map(|s| {
                s.sort_unstable();
                let mut row = vec![0.0; depths];
                let mut d = 0;
                let mut r = 0;
                while d < depths && r < s.len() {
                    if thresholds[d] > f64::from(s[r]) {
                        r += 1;
                    } else {
                        row[d] = (s.len() - r) as f64 / s.len() as f64;
                        d += 1;
                    }
                }
                row
            })
            .collect::<Vec<_>>();
        let mut alt_depths: Vec<Vec<f64>> = Vec::with_capacity(depths);
        for d in 0..depths {
            let mut row = Vec::with_capacity(d + 1);
            row.push(0.5_f64.powi(i32::try_from(d).unwrap_or(i32::MAX)));
            for a in 1..d {
                row.push((d as f64 * 0.5 / a as f64) * alt_depths[d - 1][a - 1]);
            }
            if d > 0 {
                row.push(row[0]);
            }
            alt_depths.push(row);
        }
        let mut sens = 0.0;
        for d in 0..depths {
            for a in 0..=d {
                sens += depth_dist[d] * alt_depths[d][a] * exceed[a][d];
            }
        }
        sens
    }

    #[test]
    fn fast_roulette_matches_sequential_reference_across_workers() {
        let mut sparse_q40 = vec![0.0; 127];
        sparse_q40[40] = 1.0;

        let mut sparse_edge = vec![0.0; 1024];
        sparse_edge[1000] = 0.2;

        let mut novaseq = vec![0.0; 127];
        novaseq[2] = 0.05;
        novaseq[12] = 0.15;
        novaseq[23] = 0.45;
        novaseq[37] = 1.0;

        let mut hiseq = vec![0.0; 127];
        for (idx, p) in hiseq.iter_mut().enumerate().take(42).skip(20) {
            *p = 0.3 + ((idx - 20) as f64 * 0.03);
        }

        for probs in [&sparse_q40, &sparse_edge, &novaseq, &hiseq] {
            let count = 20_000;
            let expected = ref_draws(probs, count);
            for workers in [1, 4] {
                let mut got = Vec::with_capacity(count);
                for_each_roulette_draw_with_workers(probs, count, workers, |b| got.push(b));
                assert_eq!(
                    got,
                    expected,
                    "mismatch for workers={workers}, len={}",
                    probs.len()
                );
            }
        }
    }

    #[test]
    fn wgs_het_snp_sensitivity_bit_exact_with_reference() {
        let mut unfiltered = vec![0_u64; 80];
        unfiltered[0] = 250_000;
        unfiltered[30] = 100_000;
        unfiltered[66] = 650_000;

        let mut bq_q40 = vec![0_u64; 127];
        bq_q40[40] = 50_000_000;

        let mut bq_novaseq = vec![0_u64; 127];
        bq_novaseq[2] = 100_000;
        bq_novaseq[12] = 500_000;
        bq_novaseq[23] = 2_000_000;
        bq_novaseq[37] = 20_000_000;

        for (bq, sizes) in [
            (&bq_q40, [1_usize, 2, 100, 1_000].as_slice()),
            (&bq_novaseq, [1_usize, 2, 100, 10_000].as_slice()),
        ] {
            for &sample_size in sizes {
                let expected = ref_wgs_het_snp(&unfiltered, bq, sample_size);
                let got = wgs_het_snp_sensitivity(&unfiltered, bq, sample_size);
                assert_eq!(
                    got.to_bits(),
                    expected.to_bits(),
                    "sample_size={sample_size}"
                );
            }
        }
    }
}
