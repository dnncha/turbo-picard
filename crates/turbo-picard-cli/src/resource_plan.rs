use std::collections::BTreeMap;
use std::env;
use turbo_picard_core::bgzf_threads::{self, HtsThreadRole, bgzf_threads_for_env};

const DEFAULT_CMM_BATCH_SIZE: usize = 512;
const DEFAULT_CMM_QUEUE_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourcePlan {
    pub reported_cpus: usize,
    pub global_thread_ceiling: usize,
    pub bgzf_reader_threads: usize,
    pub bgzf_writer_threads: usize,
    pub bgzf_index_threads: usize,
    pub bgzf_pipeline_reader_threads: usize,
    pub application_worker_budget: usize,
    pub cmm_batch_size: usize,
    pub cmm_queue_depth: usize,
}

pub fn resolve_current() -> ResourcePlan {
    let reported_cpus = bgzf_threads::current_reported_cpus();
    let env = env::vars().collect::<BTreeMap<_, _>>();
    resolve_from_env(reported_cpus, &env)
}

fn resolve_from_env(reported_cpus: usize, env: &BTreeMap<String, String>) -> ResourcePlan {
    let reported_cpus = reported_cpus.max(1);
    let global_thread_ceiling = bgzf_threads::global_thread_ceiling_from_env(env);
    let bgzf_reader_threads = bgzf_threads_for_env(HtsThreadRole::Reader, reported_cpus, env);
    let bgzf_writer_threads = bgzf_threads_for_env(HtsThreadRole::Writer, reported_cpus, env);
    let bgzf_index_threads = bgzf_threads_for_env(HtsThreadRole::Index, reported_cpus, env);
    let bgzf_pipeline_reader_threads =
        bgzf_threads_for_env(HtsThreadRole::PipelineReader, reported_cpus, env);
    let application_worker_budget = reported_cpus
        .min(global_thread_ceiling)
        .saturating_sub(bgzf_pipeline_reader_threads.min(reported_cpus.saturating_sub(1)))
        .max(1);
    let cmm_batch_size =
        positive_env_usize(env, "TURBO_PICARD_CMM_BATCH_SIZE").unwrap_or(DEFAULT_CMM_BATCH_SIZE);
    let cmm_queue_depth =
        positive_env_usize(env, "TURBO_PICARD_CMM_QUEUE_DEPTH").unwrap_or(DEFAULT_CMM_QUEUE_DEPTH);
    ResourcePlan {
        reported_cpus,
        global_thread_ceiling,
        bgzf_reader_threads,
        bgzf_writer_threads,
        bgzf_index_threads,
        bgzf_pipeline_reader_threads,
        application_worker_budget,
        cmm_batch_size,
        cmm_queue_depth,
    }
}

fn positive_env_usize(env: &BTreeMap<String, String>, name: &str) -> Option<usize> {
    env.get(name)
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_plan_is_deterministic_for_reported_cpu_counts() {
        let env = BTreeMap::new();
        let cases = [
            (1, (1, 1, 1, 1, 1)),
            (2, (1, 1, 1, 1, 1)),
            (4, (3, 3, 3, 2, 2)),
            (8, (7, 7, 7, 2, 6)),
            (16, (8, 12, 12, 2, 14)),
            (64, (8, 12, 12, 2, 14)),
        ];
        for (
            cpus,
            (
                expected_reader,
                expected_writer,
                expected_index,
                expected_pipeline,
                expected_app_budget,
            ),
        ) in cases
        {
            let plan = resolve_from_env(cpus, &env);
            assert_eq!(plan.reported_cpus, cpus);
            assert_eq!(
                plan.global_thread_ceiling,
                bgzf_threads::DEFAULT_MAX_THREADS
            );
            assert_eq!(plan.bgzf_reader_threads, expected_reader);
            assert_eq!(plan.bgzf_writer_threads, expected_writer);
            assert_eq!(plan.bgzf_index_threads, expected_index);
            assert_eq!(plan.bgzf_pipeline_reader_threads, expected_pipeline);
            assert_eq!(plan.application_worker_budget, expected_app_budget);
        }
    }

    #[test]
    fn role_overrides_supersede_global_default_ceiling() {
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
        let plan = resolve_from_env(16, &env);
        assert_eq!(plan.global_thread_ceiling, 4);
        assert_eq!(plan.bgzf_reader_threads, 9);
        assert_eq!(plan.bgzf_writer_threads, 3);
        assert_eq!(plan.bgzf_index_threads, 2);
        assert_eq!(plan.bgzf_pipeline_reader_threads, 1);
        assert_eq!(plan.application_worker_budget, 3);
    }

    #[test]
    fn global_thread_override_sets_all_bgzf_roles() {
        let env = BTreeMap::from([("TURBO_PICARD_THREADS".to_string(), "5".to_string())]);
        let plan = resolve_from_env(16, &env);
        assert_eq!(plan.bgzf_reader_threads, 5);
        assert_eq!(plan.bgzf_writer_threads, 5);
        assert_eq!(plan.bgzf_index_threads, 5);
        assert_eq!(plan.bgzf_pipeline_reader_threads, 5);
    }

    #[test]
    fn batch_and_queue_overrides_are_reported() {
        let env = BTreeMap::from([
            (
                "TURBO_PICARD_CMM_BATCH_SIZE".to_string(),
                "1024".to_string(),
            ),
            ("TURBO_PICARD_CMM_QUEUE_DEPTH".to_string(), "8".to_string()),
        ]);
        let plan = resolve_from_env(8, &env);
        assert_eq!(plan.cmm_batch_size, 1024);
        assert_eq!(plan.cmm_queue_depth, 8);
    }

    #[test]
    fn auto_and_invalid_overrides_fall_back_to_defaults() {
        let env = BTreeMap::from([
            (
                "TURBO_PICARD_READER_THREADS".to_string(),
                "auto".to_string(),
            ),
            ("TURBO_PICARD_WRITER_THREADS".to_string(), "0".to_string()),
            ("TURBO_PICARD_INDEX_THREADS".to_string(), "bad".to_string()),
        ]);
        let plan = resolve_from_env(8, &env);
        assert_eq!(plan.bgzf_reader_threads, 7);
        assert_eq!(plan.bgzf_writer_threads, 7);
        assert_eq!(plan.bgzf_index_threads, 7);
    }
}
