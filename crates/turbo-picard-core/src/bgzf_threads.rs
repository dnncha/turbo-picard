/// BGZF decode/encode thread count for htslib readers and writers.
pub fn bgzf_threads() -> Option<usize> {
    if let Ok(value) = std::env::var("TURBO_PICARD_THREADS") {
        return value
            .parse::<usize>()
            .ok()
            .and_then(|threads| (threads > 0).then_some(threads));
    }

    std::thread::available_parallelism()
        .ok()
        .map(|parallelism| parallelism.get().saturating_sub(1).min(4))
        .and_then(|threads| (threads > 0).then_some(threads))
}

/// Thread count for HTSlib index construction and other multi-threaded HTSlib work.
pub fn htslib_worker_threads() -> u32 {
    bgzf_threads().unwrap_or(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn htslib_worker_threads_is_at_least_one() {
        assert!(htslib_worker_threads() >= 1);
    }
}
