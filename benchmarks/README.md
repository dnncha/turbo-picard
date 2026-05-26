# MarkDuplicates Benchmarks

Benchmark runs are written under `benchmarks/runs/` and are intentionally
ignored by git. Large generated input BAMs should go under `benchmarks/inputs/`,
which is also ignored.

## 20x Smoke

The current high-confidence 20x smoke targets a common no-duplicate,
coordinate-monotonic paired-end BAM where callers do not request per-record PG
tag insertion or DT tag clearing:

```bash
cargo build --release -p turbo-picard-cli

python3 tools/bench_markduplicates.py \
  --picard-command 'mamba run -p ./.conda-turbo-picard picard MarkDuplicates ASSUME_SORTED=true VALIDATION_STRINGENCY=SILENT QUIET=true READ_NAME_REGEX=null ADD_PG_TAG_TO_READS=false CLEAR_DT=false' \
  --turbo-picard-command 'target/release/turbo-picard MarkDuplicates ASSUME_SORTED=true VALIDATION_STRINGENCY=SILENT QUIET=true READ_NAME_REGEX=null ADD_PG_TAG_TO_READS=false CLEAR_DT=false' \
  --input-bam benchmarks/inputs/unique-300k-pairs.bam \
  --output-dir benchmarks/runs/perf-pass-copy-300k-lto \
  --warmup 1 \
  --repeats 7
```

Local result on May 26 2026:

| Tool | Median wall time | Median RSS |
| --- | ---: | ---: |
| Picard 3.4.0 | 2.595 s | ~1.2 GB |
| turbo-picard | 0.127 s | ~8.7 MB |

Median speedup: 20.45x.

This is not the claim for every MarkDuplicates mode. Default PG-tagging runs on
the same fixture were around 10x. Duplicate-heavy and non-monotonic pair streams
fall back to the fully general duplicate marking path.
