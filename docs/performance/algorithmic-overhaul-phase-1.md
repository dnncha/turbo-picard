# Algorithmic Overhaul Phase 1 Notes

Date: 2026-06-20
Branch: `perf/algorithmic-overhaul`

## Scope

This phase addresses the `CollectMultipleMetrics` worker-count defect: the
resolved `TURBO_PICARD_CMM_THREADS` value now caps application collector worker
threads instead of only deciding whether the old one-worker-per-handler pipeline
is entered.

The worker pool groups collector handlers behind at most the resolved cap. A cap
of `1` creates one application collector worker. `CollectWgsMetrics` remains on
the ordered serial path inside CMM because its current state is order-dependent
until the planned sliding-frontier implementation exists.

The CMM batch size and queue depth are now centralized in the pipeline module.
Defaults remain `512` records per batch and `16` in-flight batches. The internal
overrides `TURBO_PICARD_CMM_BATCH_SIZE` and `TURBO_PICARD_CMM_QUEUE_DEPTH` accept
positive integer values; invalid or zero values fall back to the defaults.

## Baseline context read

Reviewed before editing:

- `README.md`
- `CONTRIBUTING.md`
- `docs/command-matrix.yml`
- `crates/turbo-picard-core/src/hts_io.rs`
- `crates/turbo-picard-core/src/bgzf_threads.rs`
- `crates/turbo-picard-markdup/src/lib.rs`
- `crates/turbo-picard-cli/src/cmm_pipeline.rs`
- relevant `CollectMultipleMetrics` portions of `crates/turbo-picard-cli/src/lib.rs`
- `tools/bench_collectmultiplemetrics.py`
- `tools/verify_basic_collectmultiplemetrics_parity.sh`

## Checks

Passing:

```bash
cargo fmt --check
cargo test -p turbo-picard-cli cmm_ -- --nocapture
cargo test --workspace
python3 tools/verify_command_matrix.py
cargo build --release -p turbo-picard-cli --bin picard --bin turbo-picard
```

Additional CMM tests cover exact worker caps, error/panic propagation, record
gate filtering, configurable small batches, and batch-vector allocation reuse.

Blocked or failing:

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

This currently fails on existing workspace lint debt, including
`items_after_test_module`, `too_many_arguments`, `collapsible_if`,
`type_complexity`, and related lints across `crates/turbo-picard-cli/src/lib.rs`.

```bash
bash tools/verify_basic_collectmultiplemetrics_parity.sh
```

This found `mamba`, but the configured Picard prefix
`.conda-turbo-picard` does not exist in this worktree.

## Local CMM timing sweep

Fixture: 100,000 coordinate-sorted 100M reads synthesized as SAM, converted to
BAM with the release `picard SortSam` binary. Command:

```bash
TURBO_PICARD_CMM_THREADS=<N> ./target/release/picard CollectMultipleMetrics \
  I=input.bam \
  O=cmm-<N> \
  PROGRAM=null \
  PROGRAM=CollectBaseDistributionByCycle \
  PROGRAM=QualityScoreDistribution \
  PROGRAM=MeanQualityByCycle \
  PROGRAM=CollectQualityYieldMetrics \
  QUIET=true \
  VALIDATION_STRINGENCY=SILENT
```

| CMM threads | Wall seconds |
| --- | ---: |
| 1 | 0.089198 |
| 2 | 0.049144 |
| 4 | 0.059850 |
| 8 | 0.053703 |
| auto | 0.061247 |

`quality_yield_metrics` output was identical for `1`, `2`, `4`, `8`, and
`auto` thread settings.

These are local smoke timings, not README benchmark evidence.
