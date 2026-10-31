use std::num::NonZero;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtsThreadRole {
    Reader,
    Writer,
    Index,
    PipelineReader,
}

pub fn bgzf_threads() -> Option<usize> {
    bgzf_threads_for(HtsThreadRole::Reader)
}

pub fn bgzf_threads_for(role: HtsThreadRole) -> Option<usize> {
    match explicit_threads(role) {
        ExplicitThreads::Unset => Some(default_threads(role)),
        ExplicitThreads::Resolved(threads) => threads,
    }
}

enum ExplicitThreads {
    Unset,
    Resolved(Option<usize>),
}

fn explicit_threads(role: HtsThreadRole) -> ExplicitThreads {
    let role_var = match role {
        HtsThreadRole::Reader => "TURBO_PICARD_READER_THREADS",
        HtsThreadRole::Writer => "TURBO_PICARD_WRITER_THREADS",
        HtsThreadRole::Index => "TURBO_PICARD_INDEX_THREADS",
        HtsThreadRole::PipelineReader => "TURBO_PICARD_PIPELINE_READER_THREADS",
    };
    std::env::var(role_var)
        .ok()
        .or_else(|| {
            if role == HtsThreadRole::PipelineReader {
                return std::env::var("TURBO_PICARD_READER_THREADS").ok();
            }
            None
        })
        .or_else(|| std::env::var("TURBO_PICARD_THREADS").ok())
        .map_or(ExplicitThreads::Unset, |value| {
            ExplicitThreads::Resolved(parse_thread_override(role, &value))
        })
}

fn parse_thread_override(role: HtsThreadRole, value: &str) -> Option<usize> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("auto") || value.is_empty() {
        return Some(default_threads(role));
    }
    value
        .parse::<usize>()
        .ok()
        .and_then(|threads| (threads > 0).then_some(threads))
}

fn default_threads(role: HtsThreadRole) -> usize {
    let available = std::thread::available_parallelism().map_or(1, NonZero::get);
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
        HtsThreadRole::Writer | HtsThreadRole::Index => 12,
        HtsThreadRole::PipelineReader => 2,
    };
    available
        .saturating_sub(reserved)
        .min(role_cap)
        .min(max_threads)
        .max(1)
}

pub fn htslib_worker_threads() -> u32 {
    let threads = bgzf_threads_for(HtsThreadRole::Index).unwrap_or(1);
    u32::try_from(threads).unwrap_or(u32::MAX)
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
