# turbo-picard Benchmarks

Benchmark runs are written under `benchmarks/runs/` and are intentionally
ignored by git. Large generated input BAMs should go under `benchmarks/inputs/`,
which is also ignored.

The current saved public benchmark story is straightforward: every one of the
32 benchmarked, parity-checked commands is faster than Picard 3.4.0, with a
`22.88x` floor speedup, an `84.52x` geometric mean speedup, and a `272.12x` top
speedup. The checked `MarkDuplicates` measurements are fixture-specific: the
saved copy-path run reports median RSS of about `1.2 GB` for Picard and about
`8.7 MB` for `turbo-picard`, while the newer adversarial bounded-plan run is
documented separately in `docs/performance.rst`. Neither result is a capacity
or production-scale guarantee for another workflow.

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

The second checked-in public smoke bundle uses Picard's own
`testdata/picard/sam/test.bam` fixture:

- Source citation:
  `https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/test.bam`
- Source commit:
  `fc0b08410d38a10afd08e467dab74bf5e2e71310`
- Local input:
  `benchmarks/real-data/picard-test-bam/input.bam`
- SHA-256:
  `1d499e5683479b88fad373b2de8b49f85cceae68a316e06b3cfdf60491fd7990`
- Evidence:
  `benchmarks/real-data/picard-test-bam/evidence/real-data-comparison.md`
- Manifest entry:
  `benchmarks/real-data/manifest.json`
- Scope caveat:
  `Picard public test BAM; below default release-candidate size threshold.`
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

This fixture is valuable because it exercises an unmapped paired BAM from
Picard's own public test corpus, including adapter-read and bad-cycle behavior
in `CollectAlignmentSummaryMetrics`. It is still a smoke fixture, not a
production-scale release-candidate dataset.

## Release-Candidate Real-Data Evidence

The first checked-in release-candidate bundle uses GATK's public NA12878
mitochondrial test BAM:

Together with the SNVQ bundle below, this is the current 12-command release set
used for the release-ready evidence check.

- Source citation:
  `https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam`
- Source commit:
  `e8c49f600b06c658e0fa9bf67256340ebb46bc48`
- Local input:
  `benchmarks/real-data/gatk-na12878-mito/input.bam`
- SHA-256:
  `70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3`
- Evidence:
  `benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md`
- Manifest entry:
  `benchmarks/real-data/manifest.json`
- Scope caveat:
  `GATK public NA12878 mitochondrial test BAM.`
- Release tier:
  `release_candidate`
- Minimum input threshold:
  `1000000` bytes

Current saved result against Picard 3.4.0:

| Command | Status | Comparison |
| --- | --- | --- |
| ViewSam | PASS | SAM record digest |
| CleanSam | PASS | post-command SAM record digest |
| CollectQualityYieldMetrics | PASS | stable metrics digest |
| CollectAlignmentSummaryMetrics | PASS | stable metrics digest |
| MarkDuplicates | PASS | duplicate-marking semantic digest plus stable metrics digest |
| AddOrReplaceReadGroups | PASS | SAM record digest plus read-group header digest |
| BuildBamIndex | PASS | BAI binary digest |
| RevertSam | PASS | reverted SAM record digest |
| SortSam | PASS | coordinate-sorted SAM record multiset digest |
| SamToFastq | PASS | FASTQ trio digest |
| CollectInsertSizeMetrics | PASS | stable metrics digest with insert-size histogram |
| ValidateSamFile | PASS | summary validation histogram plus exit code |

This is still a public test BAM, not proof for every dataset a lab might
process, but it is large enough for the release check and it exercises real
Picard edge cases around duplicate flags, mate-unmapped reads, soft clips, read-group
rewriting, alignment reversion to unmapped output, FASTQ conversion,
mitochondrial alignment metrics, coordinate sorting,
orientation-aware insert-size metrics, and validation-summary behavior for
missing mates and missing NM tags.

The second checked-in release-candidate bundle uses Picard's public
`testdata/picard/sam/snvq_metrics_test.bam` fixture:

- Source citation:
  `https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam`
- Source commit:
  `fc0b08410d38a10afd08e467dab74bf5e2e71310`
- Local input:
  `benchmarks/real-data/picard-snvq/input.bam`
- SHA-256:
  `be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3`
- Evidence:
  `benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md`
- Manifest entry:
  `benchmarks/real-data/manifest.json`
- Scope caveat:
  `Picard public SNVQ metrics test BAM.`
- Release tier:
  `release_candidate`
- Minimum input threshold:
  `1000000` bytes

Current saved result against Picard 3.4.0:

| Command | Status | Comparison |
| --- | --- | --- |
| ViewSam | PASS | SAM record digest |
| CleanSam | PASS | post-command SAM record digest |
| CollectQualityYieldMetrics | PASS | stable metrics digest |
| CollectAlignmentSummaryMetrics | PASS | stable metrics digest |
| MarkDuplicates | PASS | duplicate-marking semantic digest plus stable metrics digest |

This fixture is larger than the NA12878 mitochondrial bundle and comes from
Picard's own metrics test corpus. It adds coverage for insertion-heavy CIGARs,
SAM floating-tag representation differences, and alignment-summary standard
deviation behavior that matters when scientists compare Picard metrics files.

The third checked-in release-candidate bundle is the GATK mitochondrial shard
stored as CRAM:

- Source citation:
  `https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam`
- Source commit:
  `e8c49f600b06c658e0fa9bf67256340ebb46bc48`
- Local input:
  `benchmarks/real-data/gatk-na12878-mito-cram/input.cram`
- SHA-256:
  `68931e7cea6e9a35029cfed3638d0d8ea2c4bb662b4d83232968da247b68f7bc`
- Evidence:
  `benchmarks/real-data/gatk-na12878-mito-cram/evidence/real-data-comparison.md`
- Manifest entry:
  `benchmarks/real-data/manifest.json`
- Scope caveat:
  `GATK public NA12878 mitochondrial test BAM converted to CRAM with assembly38 mt-only reference.`
- Release tier:
  `release_candidate`
- Minimum input threshold:
  `910668` bytes

Current saved result against Picard 3.4.0:

| Command | Status | Comparison |
| --- | --- | --- |
| CleanSam | PASS | post-command SAM record digest |
| CollectQualityYieldMetrics | PASS | stable metrics digest |
| CollectInsertSizeMetrics | PASS | stable metrics digest with insert-size histogram |
| MarkDuplicates | PASS | duplicate-marking semantic digest plus stable metrics digest |
| SortSam | PASS | coordinate-sorted SAM record multiset digest |
| AddOrReplaceReadGroups | PASS | SAM record digest plus read-group header digest |

This exercises native CRAM I/O and reference-backed preprocessing on the same
public mitochondrial shard used for the BAM release-candidate bundle.

To regenerate CRAM real-data evidence for the GATK mitochondrial fixture after
changing comparison logic:

```bash
TURBO_PICARD_CONDA_PREFIX=/path/to/conda-env ./tools/bootstrap_gatk_mito_cram_evidence.sh
```

That script writes `benchmarks/real-data/gatk-na12878-mito-cram/` using
`fixtures/reference/chrM.fa` and updates the manifest entry when parity passes.
It compares ViewSam, CleanSam, CollectQualityYieldMetrics,
CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics, MarkDuplicates, SortSam,
AddOrReplaceReadGroups, and ValidateSamFile.
Validate a checked-in bundle with:

```bash
./tools/validate_gatk_mito_cram_evidence.sh
```

Before a scientific release, repeat `tools/compare_real_data.py` on larger
public benchmark material such as GIAB/HG001 shards and on representative
production BAMs from the workflows that would be switched. The release-ready
check also requires enough pinned input data that one tiny fixture cannot carry
the release by itself:

```bash
python3 tools/compare_real_data.py \
  --input-bam /data/HG001-or-production-shard.bam \
  --input-source-url https://example.org/datasets/GIAB-HG001-v4.2.1/input.bam \
  --input-source-commit GIAB-HG001-v4.2.1 \
  --output-dir benchmarks/real-data/HG001-smoke/evidence \
  --dataset-id HG001-smoke \
  --scope-caveat "representative HG001 shard" \
  --release-tier release_candidate \
  --commands ViewSam CollectQualityYieldMetrics CollectAlignmentSummaryMetrics CleanSam MarkDuplicates CollectInsertSizeMetrics
```

The comparator writes `manifest-entry.json` when `--dataset-id` is supplied. Add
each reviewed entry to `benchmarks/real-data/manifest.json` with:

```bash
python3 tools/update_real_data_manifest.py \
  --entry benchmarks/real-data/HG001-smoke/evidence/manifest-entry.json
```

Then run `python3 tools/verify_real_data_evidence.py` so the checked manifest
proves the source citation, input hash, passing command list, and public
documentation are still in sync.

For GitHub-hosted fixtures, cite a URL containing `/blob/<commit>/` and pass the
full 40-character Git commit SHA as `--input-source-commit`; do not use a branch
name or short hash. For accession-hosted data, cite an HTTPS URL that contains
the accession or release identifier passed as `--input-source-commit`. The
comparator and verifier reject raw GitHub branch URLs, short GitHub commits, and
accession-style citations where the identifier is not visible in the URL.

Use `release_tier: public_smoke` for small fixtures like this one. Use
`release_tier: release_candidate` only for larger public or representative
runs that should count toward a scientific release, and do not treat the
current checked-in fixtures as proof for every dataset a lab might process. The stricter
command `python3 tools/verify_real_data_evidence.py --release-ready` now passes
for the checked-in release-candidate fixtures, which proves the manifest,
citations, hashes, evidence files, timing rows, and public documentation are in
sync. It does not prove every workflow is safe to switch.

Release-candidate datasets must include `ViewSam`, `CleanSam`,
`CollectQualityYieldMetrics`, `CollectAlignmentSummaryMetrics`, and
`MarkDuplicates`, and each input must be at least 1 MB by default. The
release check must also cover AddOrReplaceReadGroups,
BuildBamIndex, CleanSam, CollectAlignmentSummaryMetrics,
CollectInsertSizeMetrics, CollectQualityYieldMetrics, MarkDuplicates,
RevertSam, SamToFastq, SortSam, ValidateSamFile, ViewSam somewhere in pinned
release-candidate evidence. The aggregate release-candidate input threshold is
currently `10000000` bytes across pinned release-candidate inputs, so a single
tiny fixture cannot satisfy the release check. If a deliberately
smaller public shard is reviewed, make that exception explicit with
`minimum_input_bytes` in the manifest rather than letting the threshold be
implicit.

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
