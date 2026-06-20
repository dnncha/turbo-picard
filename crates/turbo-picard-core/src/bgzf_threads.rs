use std::collections::BTreeMap;

pub const DEFAULT_MAX_THREADS: usize = 16;

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
    let env = std::env::vars().collect::<BTreeMap<_, _>>();
    Some(bgzf_threads_for_env(role, current_reported_cpus(), &env))
}

pub fn current_reported_cpus() -> usize {
    std::thread::available_parallelism()
        .ok()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1)
        .max(1)
}

pub fn global_thread_ceiling_from_env(env: &BTreeMap<String, String>) -> usize {
    positive_env_usize(env, "TURBO_PICARD_MAX_THREADS")
        .unwrap_or(DEFAULT_MAX_THREADS)
        .max(1)
}

pub fn bgzf_threads_for_env(
    role: HtsThreadRole,
    reported_cpus: usize,
    env: &BTreeMap<String, String>,
) -> usize {
    if let Some(threads) = explicit_threads(role, reported_cpus.max(1), env) {
        return threads;
    }

    default_threads(role, reported_cpus, env)
}

pub fn bgzf_reader_threads_per_input(simultaneous_readers: usize) -> Option<usize> {
    let env = std::env::vars().collect::<BTreeMap<_, _>>();
    Some(bgzf_reader_threads_per_input_for_env(
        simultaneous_readers,
        current_reported_cpus(),
        &env,
    ))
}

pub fn bgzf_reader_threads_per_input_for_env(
    simultaneous_readers: usize,
    reported_cpus: usize,
    env: &BTreeMap<String, String>,
) -> usize {
    if positive_env_usize(env, "TURBO_PICARD_READER_THREADS").is_some() {
        return bgzf_threads_for_env(HtsThreadRole::Reader, reported_cpus, env);
    }
    let reader_budget = bgzf_threads_for_env(HtsThreadRole::Reader, reported_cpus, env);
    reader_budget
        .checked_div(simultaneous_readers.max(1))
        .unwrap_or(reader_budget)
        .max(1)
}

fn explicit_threads(
    role: HtsThreadRole,
    reported_cpus: usize,
    env: &BTreeMap<String, String>,
) -> Option<usize> {
    let role_var = match role {
        HtsThreadRole::Reader => "TURBO_PICARD_READER_THREADS",
        HtsThreadRole::Writer => "TURBO_PICARD_WRITER_THREADS",
        HtsThreadRole::Index => "TURBO_PICARD_INDEX_THREADS",
        HtsThreadRole::PipelineReader => "TURBO_PICARD_PIPELINE_READER_THREADS",
    };
    env.get(role_var)
        .and_then(|value| parse_thread_override(role, reported_cpus, env, value))
        .or_else(|| {
            if role == HtsThreadRole::PipelineReader {
                return env
                    .get("TURBO_PICARD_READER_THREADS")
                    .and_then(|value| parse_thread_override(role, reported_cpus, env, value));
            }
            None
        })
        .or_else(|| {
            env.get("TURBO_PICARD_THREADS")
                .and_then(|value| parse_thread_override(role, reported_cpus, env, value))
        })
}

fn parse_thread_override(
    role: HtsThreadRole,
    reported_cpus: usize,
    env: &BTreeMap<String, String>,
    value: &str,
) -> Option<usize> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("auto") || value.is_empty() {
        return Some(default_threads(role, reported_cpus, env));
    }
    value
        .parse::<usize>()
        .ok()
        .and_then(|threads| (threads > 0).then_some(threads))
}

fn default_threads(
    role: HtsThreadRole,
    reported_cpus: usize,
    env: &BTreeMap<String, String>,
) -> usize {
    let reported_cpus = reported_cpus.max(1);
    let max_threads = global_thread_ceiling_from_env(env);
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
    reported_cpus
        .saturating_sub(reserved)
        .min(role_cap)
        .min(max_threads)
        .max(1)
}

fn positive_env_usize(env: &BTreeMap<String, String>, name: &str) -> Option<usize> {
    env.get(name)
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|threads| *threads > 0)
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

    #[test]
    fn env_plan_is_deterministic_for_reported_cpu_counts() {
        let env = BTreeMap::new();
        let cases = [
            (1, (1, 1, 1, 1)),
            (2, (1, 1, 1, 1)),
            (4, (3, 3, 3, 2)),
            (8, (7, 7, 7, 2)),
            (16, (8, 12, 12, 2)),
            (64, (8, 12, 12, 2)),
        ];
        for (cpus, (expected_reader, expected_writer, expected_index, expected_pipeline)) in cases {
            assert_eq!(
                bgzf_threads_for_env(HtsThreadRole::Reader, cpus, &env),
                expected_reader
            );
            assert_eq!(
                bgzf_threads_for_env(HtsThreadRole::Writer, cpus, &env),
                expected_writer
            );
            assert_eq!(
                bgzf_threads_for_env(HtsThreadRole::Index, cpus, &env),
                expected_index
            );
            assert_eq!(
                bgzf_threads_for_env(HtsThreadRole::PipelineReader, cpus, &env),
                expected_pipeline
            );
        }
    }

    #[test]
    fn explicit_role_threads_supersede_global_ceiling() {
        let env = BTreeMap::from([
            ("TURBO_PICARD_MAX_THREADS".to_string(), "4".to_string()),
            ("TURBO_PICARD_READER_THREADS".to_string(), "9".to_string()),
            ("TURBO_PICARD_WRITER_THREADS".to_string(), "3".to_string()),
            ("TURBO_PICARD_INDEX_THREADS".to_string(), "2".to_string()),
            (
                "TURBO_PICARD_PIPELINE_READER_THREADS".to_string(),
                "1".to_string(),
            ),
        ]);
        assert_eq!(global_thread_ceiling_from_env(&env), 4);
        assert_eq!(bgzf_threads_for_env(HtsThreadRole::Reader, 16, &env), 9);
        assert_eq!(bgzf_threads_for_env(HtsThreadRole::Writer, 16, &env), 3);
        assert_eq!(bgzf_threads_for_env(HtsThreadRole::Index, 16, &env), 2);
        assert_eq!(
            bgzf_threads_for_env(HtsThreadRole::PipelineReader, 16, &env),
            1
        );
    }

    #[test]
    fn global_thread_override_sets_all_bgzf_roles() {
        let env = BTreeMap::from([("TURBO_PICARD_THREADS".to_string(), "5".to_string())]);
        assert_eq!(bgzf_threads_for_env(HtsThreadRole::Reader, 16, &env), 5);
        assert_eq!(bgzf_threads_for_env(HtsThreadRole::Writer, 16, &env), 5);
        assert_eq!(bgzf_threads_for_env(HtsThreadRole::Index, 16, &env), 5);
        assert_eq!(
            bgzf_threads_for_env(HtsThreadRole::PipelineReader, 16, &env),
            5
        );
    }

    #[test]
    fn broad_reader_budget_is_divided_across_simultaneous_inputs() {
        let env = BTreeMap::new();
        assert_eq!(bgzf_reader_threads_per_input_for_env(1, 16, &env), 8);
        assert_eq!(bgzf_reader_threads_per_input_for_env(2, 16, &env), 4);
        assert_eq!(bgzf_reader_threads_per_input_for_env(3, 16, &env), 2);
        assert_eq!(bgzf_reader_threads_per_input_for_env(8, 16, &env), 1);

        let env = BTreeMap::from([("TURBO_PICARD_THREADS".to_string(), "6".to_string())]);
        assert_eq!(bgzf_reader_threads_per_input_for_env(3, 16, &env), 2);
    }

    #[test]
    fn explicit_reader_override_is_per_reader() {
        let env = BTreeMap::from([
            ("TURBO_PICARD_THREADS".to_string(), "6".to_string()),
            ("TURBO_PICARD_READER_THREADS".to_string(), "5".to_string()),
        ]);
        assert_eq!(bgzf_reader_threads_per_input_for_env(4, 16, &env), 5);
    }
}
