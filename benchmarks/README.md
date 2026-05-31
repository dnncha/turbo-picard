# turbo-picard Benchmarks

Benchmark runs are written under `benchmarks/runs/` and are intentionally
ignored by git. Large generated input BAMs should go under `benchmarks/inputs/`,
which is also ignored.

## Public Real-Data Smoke

The first checked-in public-BAM evidence bundle uses HTSlib's small
`test/range.bam` alignment fixture:

- Source citation:
  `https://github.com/samtools/htslib/blob/5cded8325aca2f84f6c18641664893b900638086/test/range.bam`
- Source commit:
  `5cded8325aca2f84f6c18641664893b900638086`
- Local input:
  `benchmarks/real-data/htslib-range/input.bam`
- SHA-256:
  `e15d14e3994027d433431c960bf1c5f2d6939f26b5094cd5a86bc6229a5b2661`
- Evidence:
  `benchmarks/real-data/htslib-range/evidence/real-data-comparison.md`
- Manifest entry:
  `benchmarks/real-data/manifest.json`
- Release tier:
  `public_smoke`

Current saved result against Picard 3.4.0:

| Command | Status | Comparison |
| --- | --- | --- |
| ViewSam | PASS | SAM record digest |
| CleanSam | PASS | post-command SAM record digest |
| CollectQualityYieldMetrics | PASS | stable metrics digest |
| CollectAlignmentSummaryMetrics | PASS | stable metrics digest |
| MarkDuplicates | PASS | duplicate-marking semantic digest plus stable metrics digest |

This small public fixture is useful as a reproducible public smoke test, not as
the final basis for production replacement. It now covers
read viewing, BAM cleanup, two metrics commands, and duplicate-marking output on
a paired alignment fixture with chimeric, indel, soft-clipped, and orphaned
paired reads.

Before a scientist-facing release, repeat `tools/compare_real_data.py` on larger
public benchmark material such as GIAB/HG001 shards and on representative
production BAMs from the workflows that would be switched:

```bash
python3 tools/compare_real_data.py \
  --input-bam /data/HG001-or-production-shard.bam \
  --input-source-url https://example.org/datasets/GIAB-HG001-v4.2.1/input.bam \
  --input-source-commit GIAB-HG001-v4.2.1 \
  --output-dir benchmarks/real-data/HG001-smoke \
  --dataset-id HG001-smoke \
  --scope-caveat "representative HG001 shard" \
  --release-tier release_candidate \
  --commands ViewSam CollectQualityYieldMetrics CollectAlignmentSummaryMetrics CleanSam MarkDuplicates
```

The comparator writes `manifest-entry.json` when `--dataset-id` is supplied. Add
each reviewed entry to `benchmarks/real-data/manifest.json` with:

```bash
python3 tools/update_real_data_manifest.py \
  --entry benchmarks/real-data/HG001-smoke/manifest-entry.json
```

Then run `python3 tools/verify_real_data_evidence.py` so the checked manifest
proves the source citation, input hash, passing command list, and public
documentation are still in sync.

For GitHub-hosted fixtures, cite a URL containing `/blob/<commit>/`. For
accession-hosted data, cite an HTTPS URL that contains the accession or release
identifier passed as `--input-source-commit`. The verifier rejects raw GitHub
branch URLs and accession-style citations where the identifier is not visible in
the URL.

Use `release_tier: public_smoke` for small fixtures like this one. Use
`release_tier: release_candidate` only for larger public or production-like
runs that should count toward a scientist-facing release. The stricter command
`python3 tools/verify_real_data_evidence.py --release-ready` fails until the
manifest contains at least one pinned release-candidate dataset. Release
candidates must include `ViewSam`, `CleanSam`, `CollectQualityYieldMetrics`,
`CollectAlignmentSummaryMetrics`, and `MarkDuplicates`, and the input must be at
least 1 MB by default. If a deliberately smaller public shard is reviewed, make
that exception explicit with `minimum_input_bytes` in the manifest rather than
letting the threshold be implicit.

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
