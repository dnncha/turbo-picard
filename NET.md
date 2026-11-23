# Feedback net

Inner loop: `python3 -m pytest -q tools/test_duel.py tools/test_parity_compare.py` (under 1 s) plus `cargo test -p <crate> --locked` for the crate you touched.
Gate: `gate-run -C . -- bash tools/static_gate.sh` then `cargo test --workspace --locked` (minutes, fleet).
Parity gate: `TURBO_PICARD_CONDA_PREFIX=<picard 3.4.0 + samtools env> tools/parity_gate.sh` (tens of minutes; a Colab high-RAM CPU VM works).
Genome scale and competitors: `tools/colab_duel.sh launch <session> REF=<branch>` then `sync`, `report` (hours, Colab, about 0.3 units/hour on CPU).

## Signals by tier

| Tier | Signal | Command | Runs | Time | Status |
|---|---|---|---|---|---|
| edit | rustc, clippy pedantic, mypy --strict, ruff | `tools/static_gate.sh` | gate (tools/static_gate.sh) | minutes | honest |
| inner | Python tool tests | `python3 -m pytest -q tools` | inner | seconds | honest |
| inner | Rust unit tests | `cargo test --workspace --locked` | inner, gate | minutes | honest |
| gate | Picard parity scripts on fixtures | `tools/parity_gate.sh` | release: before each release | tens of minutes | honest; comparator exact at printed precision since 22 Nov 2026 |
| human | Duel vs Picard, riker, dupblaster, FastDup, samtools on 1000 Genomes HG02675 4x/15x/30x | `tools/colab_duel.sh` | human: before a release; each run spends Colab units | hours | new 22 Nov 2026 |
| human | Full 30x CRAM parity, CollectWgsMetrics and MarkDuplicates | `wgs30x_parity.sh` pattern in `benchmarks/duel/README.md` | human: before a release; each run spends Colab units | about 8 h | new 22 Nov 2026 |

## Risk surfaces

| Surface | Paths | Risk (L×I) | Covered by | Status |
|---|---|---|---|---|
| Metric parity on real genomes | `crates/turbo-picard-cli/src/wgs_metrics*`, `lib.rs` collectors | high × high | parity gate on fixtures, duel accuracy rows | late: genome scale only nightly |
| MarkDuplicates selection and flags | `crates/turbo-picard-markdup/` | med × high | parity gate, duel duplicate-key diff vs Picard | late at genome scale |
| Thread sizing on large hosts | `crates/turbo-picard-core/src/bgzf_threads.rs` | high × med | unit test, duel `max-threads` variant | covered |
| Competitive speed and memory | whole CLI | high × high (adoption) | duel scoreboard and `losses.json` | covered nightly |
| Query-grouped MarkDuplicates input | `turbo-picard-markdup` | med × med | duel `markdup-qgroup` task | probe: refusal is logged as a loss |
| Benchmark claims in docs | `README.md`, `docs/` | med × high | `verify_benchmark_*` tools | covered |

## Holes, ranked

1. Riker `multi` accuracy is not compared field by field with Picard → map riker's per-tool files to Picard metrics in `tools/duel.py` (nightly, small).
2. Duel competitor accuracy at 30x has no Picard reference in the duel itself → feed the 30x parity run's Picard outputs into `duel.py report` (nightly, small).
3. No cold-cache timing on Colab (page cache cannot be dropped in the container) → measure cold runs on a host that allows `drop_caches` (manual, medium).

## Escape log

| Date | Symptom | Root cause | Class | Caught by | Fixed (+siblings) | Class guard | Rung | Time to red | Proved |
|---|---|---|---|---|---|---|---|---|---|
| 2026-11-13 | 0.1.18 CollectWgsMetrics differed from Picard on a real 4x genome | comparator slack plus fixtures too small to exceed it | tolerance hides real drift | manual genome-scale run | WGS comparator made exact (Nov 13); generic metrics comparator made exact at printed precision (Nov 22) | `same_printed_value` in `tools/parity_compare.py`; duel accuracy rows on real genomes | 4 sweep + 7 regression | under 1 s for the unit test; hours for genome scale | red on pre-fix comparator (0.069409 vs 0.069419 accepted), green after |
| 2026-11-22 | turbo-picard left cores idle on 48-core hosts | global default cap of 16 threads, region workers capped at 32, CollectMultipleMetrics collectors at 6 | fixed thread ceilings | duel planning review | caps removed for region workers and collectors | `region_workers_default_to_every_core` test; duel `max-threads` variant flags any remaining gap as an under-parallel loss | 5 assertion + nightly measurement; above: core counts are a property of the host at runtime | seconds | red with `unwrap_or(16)` on hosts over 16 cores |
| 2026-11-22 | Parity gate failed only when launched from a Jupyter kernel: samtools could not read Picard ViewSam output | JVM printed cgroup warnings on stdout, which carried the SAM | tool log lines mixed into data on stdout | Colab parity gate | parity harness and turbo-picard's Picard fallback send JVM logs to stderr | `fallback_jvm_logs_never_reach_stdout` test; `parity_picard` sets `-Xlog` to stderr | 3 rule + 7 regression | seconds | red on the kernel-launched gate before the fix |
| 2026-11-23 | CollectMultipleMetrics took 1,071 s on a 10x genome where riker multi took 240 s | GC-bias setup recounted every 100-base window of the 3.2 Gb reference with two divisions per base, serially; each read recounted its own window | per-element recomputation of a sliding quantity | duel losses.json plus perf profile | sliding tally, division-free histogram, per-contig GC lookup table, contigs scanned in parallel from the .fai | property tests against the Picard-rule oracles (`sliding_gc_windows_match_recounting_every_window`, `gc_lookup_table_matches_picard_window_rule`, `indexed_parallel_gc_windows_match_streaming`); duel scoreboard flags any rival win | 6 property + nightly duel; above: window arithmetic over the whole input space, checked against oracle properties | under 1 s for the properties | oracles kept test-only; outputs byte-identical to the pre-change build on chr20 |
| 2026-11-23 | Duel's \"4x\" dataset was about 10x | second subsample reused seed 42, and samtools subsampling hashes read names, so it kept the same reads | deterministic hash reuse across subsamples | MEAN_COVERAGE 7.6 in the 4x row | 4x now uses seed 7 | `check_depth` after every subsample | 5 assertion (`check_depth` in `tools/duel_vm.sh` refuses a subsample outside 0.75 to 1.25 of its named depth and deletes it; `tools/test_duel_depth.py` red on the 7.6x case and on a planted always-pass bound); above: depth is a property of data produced at run time | under 1 s in the test; at prepare time in the duel | yes |
| 2026-11-23 | After merging main into #79, CollectGcBiasMetrics MEAN_BASE_QUALITY was 0 where Picard gave 17 on the GATK mito BAM | auto-merge kept main's `gc_bias_error_count(record, &self.active_sequence)`, but #79's preloaded path leaves `active_sequence` empty, so no errors were counted | merged code reads a buffer the other branch stopped filling | parity gate before release | error count now uses the resolved reference | `gc_bias_counts_mismatches_against_preloaded_reference` unit test | 7 regression; above: a merge interaction between two branches, which no single-file rule sees | seconds | red with the auto-merged line, green after |
