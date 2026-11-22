# Feedback net

Inner loop: `python3 -m pytest -q tools/test_duel.py tools/test_parity_compare.py` (under 1 s) plus `cargo test -p <crate> --locked` for the crate you touched.
Gate: `gate-run -C . -- bash tools/static_gate.sh` then `cargo test --workspace --locked` (minutes, fleet).
Parity gate: `TURBO_PICARD_CONDA_PREFIX=<picard 3.4.0 + samtools env> tools/parity_gate.sh` (tens of minutes; a Colab high-RAM CPU VM works).
Genome scale and competitors: `tools/colab_duel.sh launch <session> REF=<branch>` then `sync`, `report` (hours, Colab, about 0.3 units/hour on CPU).

## Signals by tier

| Tier | Signal | Command | Time | Status |
|---|---|---|---|---|
| edit | rustc, clippy pedantic, mypy --strict, ruff | `tools/static_gate.sh` | minutes | honest |
| inner | Python tool tests | `python3 -m pytest -q tools` | seconds | honest |
| inner | Rust unit tests | `cargo test --workspace --locked` | minutes | honest |
| gate | Picard parity scripts on fixtures | `tools/parity_gate.sh` | tens of minutes | honest; comparator exact at printed precision since 22 Nov 2026 |
| nightly | Duel vs Picard, riker, dupblaster, FastDup, samtools on 1000 Genomes HG02675 4x/15x/30x | `tools/colab_duel.sh` | hours | new 22 Nov 2026 |
| nightly | Full 30x CRAM parity, CollectWgsMetrics and MarkDuplicates | `wgs30x_parity.sh` pattern in `benchmarks/duel/README.md` | about 8 h | new 22 Nov 2026 |

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
| 2026-11-22 | turbo-picard left cores idle on 48-core hosts | global default cap of 16 threads, region workers capped at 32, CollectMultipleMetrics collectors at 6 | fixed thread ceilings | duel planning review | caps removed for region workers and collectors | `region_workers_default_to_every_core` test; duel `max-threads` variant flags any remaining gap as an under-parallel loss | 5 assertion + nightly measurement | seconds | red with `unwrap_or(16)` on hosts over 16 cores |
