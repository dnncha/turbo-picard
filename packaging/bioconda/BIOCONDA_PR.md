# Add turbo-picard and optional picard compatibility shim

This PR adds `turbo-picard`, a Rust implementation of selected Picard command
surfaces, plus a separate opt-in shim package for workflows that deliberately
want the `picard` command name.

The split is intentional:

- `turbo-picard` installs only the explicit `turbo-picard` command, so it can
  live beside the existing Bioconda `picard` package.
- `turbo-picard-picard-shim` installs the compatibility `picard` entrypoint.
  It depends on the matching `turbo-picard` build and declares `picard ==0` as a
  run constraint because both packages own the same command name.

The package is not presented as a complete replacement for all Picard behavior.
The native command surfaces are documented, checked against upstream Picard, and
fall back or fail explicitly outside the supported scope.

## Recipe notes

- Both recipes build compiled Rust binaries, so they are not `noarch`.
- Windows is skipped with `skip: true  # [win]`.
- Rust dependency licenses are bundled during the build with
  `cargo-bundle-licenses --format yaml --output THIRDPARTY.yml`; both `LICENSE`
  and `THIRDPARTY.yml` are listed under `license_file`.
- The shim recipe is separate from the main package, pins the matching
  `turbo-picard` build with `turbo-picard =={{ version }}`, and declares
  `picard ==0` in `run_constrained` because it intentionally owns the same
  command name as upstream Picard.

## Source

- Tagged archive URL:
  `https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.0.tar.gz`
- Archive SHA-256:
  `95923bebbc7f6ab59e73c436b31d84c8da547939c1e6c63be984747acfbc387c`

The recipe source was prepared from the downloaded GitHub archive with:

```bash
python3 tools/prepare_bioconda_release.py \
  --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
```

Prefer `--archive` for release submission because it validates the downloaded
GitHub source archive before writing the digest. If `--sha256` is used instead,
the digest must have been computed from the downloaded GitHub source archive;
that fallback skips archive filename and content validation.

The helper accepts `turbo-picard-0.1.0.tar.gz` or GitHub's `v0.1.0.tar.gz` for
this version. It checks for a GitHub-style source tarball with a
`turbo-picard-0.1.0/` top-level directory and these required files:
`Cargo.toml`, `Cargo.lock`, `CITATION.cff`, `docs/command-matrix.yml`,
`docs/parity.rst`, `benchmarks/real-data/manifest.json`,
`docs/site/assets/benchmark-data.json`,
`packaging/bioconda/turbo-picard/meta.yaml`, and
`packaging/bioconda/turbo-picard-picard-shim/meta.yaml`.

It also rejects unsafe paths, duplicate archive entries, unsupported tar member
types, empty required source files, version mismatches, and mismatched recipe
source blocks before writing the same URL and SHA-256 into both recipes and
`packaging/bioconda/BIOCONDA_PR.md` as the PR body. The archive-internal
metadata check covers the workspace version, `CITATION.cff` archived-release
citation fields, the `picard_reference` entry for Picard 3.4.0, the `datasets`
and `benchmarks` evidence sections, and the matching recipe version and source
block metadata.

## Evidence and scope

The release-candidate evidence is checked in so reviewers can inspect the exact
inputs, commands, comparisons, and caveats rather than relying on a broad
performance claim.

Manifest:
`benchmarks/real-data/manifest.json`

Release-candidate command portfolio required for submission:
AddOrReplaceReadGroups, BuildBamIndex, CleanSam,
CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics,
CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, SortSam,
ValidateSamFile, ViewSam.

### GATK NA12878 mitochondrial test BAM

- Evidence report:
  `benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md`
- Evidence JSON:
  `benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.json`
- Source:
  `https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam`
- Commit:
  `e8c49f600b06c658e0fa9bf67256340ebb46bc48`
- Local SHA-256:
  `70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3`
- Scope caveat:
  `GATK public NA12878 mitochondrial test BAM.`
- Minimum input threshold:
  `1000000` bytes

Saved comparison against Picard 3.4.0:

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

### Picard SNVQ metrics test BAM

- Evidence report:
  `benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md`
- Evidence JSON:
  `benchmarks/real-data/picard-snvq/evidence/real-data-comparison.json`
- Source:
  `https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam`
- Commit:
  `fc0b08410d38a10afd08e467dab74bf5e2e71310`
- Local SHA-256:
  `be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3`
- Scope caveat:
  `Picard public SNVQ metrics test BAM.`
- Minimum input threshold:
  `1000000` bytes

Saved comparison against Picard 3.4.0:

| Command | Status | Comparison |
| --- | --- | --- |
| ViewSam | PASS | SAM record digest |
| CleanSam | PASS | post-command SAM record digest |
| CollectQualityYieldMetrics | PASS | stable metrics digest |
| CollectAlignmentSummaryMetrics | PASS | stable metrics digest |
| MarkDuplicates | PASS | duplicate-marking semantic digest plus stable metrics digest |

These fixtures are suitable for this packaging release gate. They are public,
pinned, and reviewable, but they are still small. Larger public shards or
workflow-owned representative data should be used before making broader
cohort-scale claims.

## Benchmark evidence

The public benchmark artifact was generated from:

- Source command:
  `python3 tools/bench_suite.py --repeats 1 --skip-build`
- Raw log:
  `docs/site/assets/bench-suite-output.txt`
- JSON summary:
  `docs/site/assets/benchmark-data.json`
- Benchmark date:
  `2026-05-31`
- Parity:
  `32/32 PASS`
- Summary:
  `112.07x` top speedup on `UpdateVcfSequenceDictionary`; `7.40x` floor
  speedup on `RevertSam`; `26.24x` median speedup; `27.31x` geometric mean
  speedup.
- Recently promoted public benchmarks:
  `IntervalListTools`, `LiftoverVcf`, `CollectMultipleMetrics`,
  `CollectGcBiasMetrics`

`python3 tools/verify_benchmark_thresholds.py` enforces full saved benchmark
parity, at least `5.00x` floor speedup, at least `20.00x` geometric mean
speedup, and at least `50.00x` top speedup before these numbers are used as
release evidence.

## Citation

`CITATION.cff` is the software citation for the archived turbo-picard release.
The benchmark and validation inputs are cited separately with immutable source
URLs, commits or accessions, and SHA-256 hashes. That separation is deliberate:
the software citation should not be used as a substitute for the input-data
provenance above.

## Checks run before copying recipes

```bash
python3 tools/bioconda_release_preflight.py
python3 -m unittest discover tools
python3 tools/update_real_data_manifest.py \
  --entry benchmarks/real-data/gatk-na12878-mito/evidence/manifest-entry.json \
  --entry benchmarks/real-data/picard-snvq/evidence/manifest-entry.json \
  --replace
python3 tools/verify_real_data_evidence.py --release-ready
python3 tools/verify_bioconda_recipes.py --release-ready
python3 tools/verify_release_versions.py
python3 tools/verify_benchmark_suite_coverage.py
python3 tools/verify_benchmark_thresholds.py
python3 tools/verify_ci_coverage.py
python3 tools/verify_parity_docs.py
python3 tools/verify_readme_links.py
python3 tools/verify_site_links.py
./tools/verify_package_install.sh
cargo test --workspace
```

## Bioconda checks after copying recipes

```bash
cp -R packaging/bioconda/turbo-picard recipes/turbo-picard
cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim
bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim
bioconda-utils build --docker --mulled-test turbo-picard
bioconda-utils build --docker --mulled-test turbo-picard-picard-shim
```
