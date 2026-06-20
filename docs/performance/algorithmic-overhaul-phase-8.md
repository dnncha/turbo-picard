# Algorithmic Overhaul Phase 8 Notes

Date: 2026-06-20
Branch: `perf/algorithmic-overhaul`

## Scope

Phase 8 introduces command-level resource planning so BGZF, application worker,
indexing, and batching choices can be reasoned about together instead of as
separate local defaults.

## Implemented

- Added a `resource_plan` module in the CLI crate. The plan reports detected CPU
  count, global thread ceiling, BGZF reader/writer/index/pipeline-reader thread
  counts, an application worker budget, and CMM batch/queue settings.
- The resolver has deterministic tests for reported CPU counts 1, 2, 4, 8, 16,
  and 64, plus tests for role overrides, global thread overrides, `auto`/invalid
  override fallback, and CMM batch/queue environment overrides.
- `turbo-picard doctor` now prints `resource_plan_*` lines so the resolved
  resource plan is visible without running a data-processing command.
- Core BGZF thread selection now exposes deterministic environment-based
  helpers, and runtime reader/writer/index configuration plus `doctor` use the
  same role resolution. Explicit role overrides continue to supersede the
  global default ceiling.
- K-way BAM/CRAM merge readers and temporary-run merge readers divide the broad
  reader budget across simultaneously open inputs, so merge-style paths no
  longer assign every input the full default reader pool.
  `TURBO_PICARD_READER_THREADS` remains an explicit per-reader override.

## Tests

Passing:

```bash
cargo fmt --check
cargo test -p turbo-picard-cli resource_plan -- --nocapture
cargo test -p turbo-picard-core bgzf_threads -- --nocapture
cargo test -p turbo-picard-cli mergesamfiles -- --nocapture
cargo test -p turbo-picard-cli doctor -- --nocapture
cargo test -p turbo-picard-cli acceleration_status -- --nocapture
```

Local release benchmark smoke:

```text
command=MergeSamFiles
reads=20000
shards=4
thread_env=default
repeats=3
median_wall_seconds=0.088589
wall_seconds=0.606000,0.084408,0.088589
record_count_check=PASS
```

The Picard-backed `tools/verify_basic_mergesamfiles_parity.sh` comparison was
not available in this worktree because `.conda-turbo-picard` did not exist.

Remaining Phase 8 work:

- Add command-specific planning for sorter buffers, memory budgets, and mate
  caches.
