# turbo-picard-picard-shim

This optional Bioconda recipe installs a `picard` command that forwards
supported Picard-style calls to `turbo-picard`.

The main `turbo-picard` package does not install this binary. Keep the shim as a
separate package so users choose when they want the `picard` command name to
resolve to turbo-picard instead of upstream Picard.

The shim depends on the matching `turbo-picard =={{ version }}` package and
declares a constraint against upstream `picard`, because both packages provide
the same command name.

## Before submission

Run the release helper and checks from the repository root after creating the
GitHub release archive:

```bash
python3 tools/prepare_bioconda_release.py \
  --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
python3 tools/verify_benchmark_suite_coverage.py
python3 tools/verify_benchmark_thresholds.py
python3 tools/verify_ci_coverage.py
python3 tools/verify_parity_docs.py
python3 tools/verify_readme_links.py
python3 tools/verify_site_links.py
python3 tools/verify_real_data_evidence.py --release-ready
python3 tools/verify_bioconda_recipes.py --release-ready
```

Use `packaging/bioconda/BIOCONDA_PR.md` as the Bioconda PR body for the pair.
It should explain the package split, cite the archived turbo-picard release
through `CITATION.cff`, and keep benchmark input citations separate with pinned
source URLs, commits or accessions, and SHA-256 hashes.

The release evidence currently covers:

AddOrReplaceReadGroups, BuildBamIndex, CleanSam,
CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics,
CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, SortSam,
ValidateSamFile, ViewSam.

Benchmark numbers are release evidence only while the saved suite keeps full
parity, at least `5.00x` floor speedup, at least `20.00x` geometric mean
speedup, and at least `50.00x` top speedup.

## Bioconda checkout

Copy and test both recipes together:

```bash
cp -R packaging/bioconda/turbo-picard recipes/turbo-picard
cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim
bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim
bioconda-utils build --docker --mulled-test turbo-picard
bioconda-utils build --docker --mulled-test turbo-picard-picard-shim
```
