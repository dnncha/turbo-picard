#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtsThreadRole {
    Reader,
    Writer,
    Index,
    PipelineReader,
}

/// BGZF decode/encode thread count for htslib readers and writers.
pub fn bgzf_threads() -> Option<usize> {
    bgzf_threads_for(HtsThreadRole::Reader)
}

pub fn bgzf_threads_for(role: HtsThreadRole) -> Option<usize> {
    if let Some(threads) = explicit_threads(role) {
        return threads;
    }

    default_threads(role)
}

fn explicit_threads(role: HtsThreadRole) -> Option<Option<usize>> {
    let role_var = match role {
        HtsThreadRole::Reader => "TURBO_PICARD_READER_THREADS",
        HtsThreadRole::Writer => "TURBO_PICARD_WRITER_THREADS",
        HtsThreadRole::Index => "TURBO_PICARD_INDEX_THREADS",
        HtsThreadRole::PipelineReader => "TURBO_PICARD_PIPELINE_READER_THREADS",
    };
    std::env::var(role_var)
        .ok()
        .map(|value| parse_thread_override(role, &value))
        .or_else(|| {
            if role == HtsThreadRole::PipelineReader {
                return std::env::var("TURBO_PICARD_READER_THREADS")
                    .ok()
                    .map(|value| parse_thread_override(role, &value));
            }
            None
        })
        .or_else(|| {
            std::env::var("TURBO_PICARD_THREADS")
                .ok()
                .map(|value| parse_thread_override(role, &value))
        })
}

fn parse_thread_override(role: HtsThreadRole, value: &str) -> Option<usize> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("auto") || value.is_empty() {
        return default_threads(role);
    }
    value
        .parse::<usize>()
        .ok()
        .and_then(|threads| (threads > 0).then_some(threads))
}

fn default_threads(role: HtsThreadRole) -> Option<usize> {
    let available = std::thread::available_parallelism()
        .ok()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1);
    let max_threads = std::env::var("TURBO_PICARD_MAX_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|threads| *threads > 0)
        .unwrap_or(16);
    let reserved = match role {
        HtsThreadRole::PipelineReader => 2,
        _ => 1,
    };
    let role_cap = match role {
        HtsThreadRole::Reader => 8,
        HtsThreadRole::Writer => 12,
        HtsThreadRole::Index => 12,
        HtsThreadRole::PipelineReader => 2,
    };
    let threads = available
        .saturating_sub(reserved)
        .min(role_cap)
        .min(max_threads)
        .max(1);
    Some(threads)
}

/// Thread count for HTSlib index construction and other multi-threaded HTSlib work.
pub fn htslib_worker_threads() -> u32 {
    bgzf_threads_for(HtsThreadRole::Index).unwrap_or(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn htslib_worker_threads_is_at_least_one() {
        assert!(htslib_worker_threads() >= 1);
    }

    #[test]
    fn default_threads_are_role_scoped() {
        assert!(bgzf_threads_for(HtsThreadRole::Reader).unwrap_or(1) >= 1);
        assert!(bgzf_threads_for(HtsThreadRole::Writer).unwrap_or(1) >= 1);
        assert!(bgzf_threads_for(HtsThreadRole::PipelineReader).unwrap_or(1) >= 1);
    }
}
