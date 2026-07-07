# Bioconda packaging notes

This recipe builds the main `turbo-picard` package. It installs only the
`turbo-picard` command, so it can live beside Bioconda's existing `picard`
package.

The separate `turbo-picard-picard-shim` recipe installs the optional `picard`
command name. Keep that split: users should opt into the shim because it owns a
command name that the upstream Picard package also provides.

## Local recipe state

Inside this repository the recipe uses the working tree:

```yaml
source:
  path: ../../..
```

That is only for local packaging tests. Do not open a Bioconda PR while the
recipe still uses `source.path`; wait until the release-ready verifier passes.

Before submission, cut the GitHub release, download the source archive, and run:

```bash
python3 tools/bioconda_release_preflight.py
python3 tools/prepare_bioconda_release.py \
  --archive ~/Downloads/turbo-picard-0.1.8.tar.gz
python3 tools/verify_bioconda_recipes.py --release-ready
```

Use `--archive` when possible. It checks the downloaded GitHub source archive,
computes the SHA-256, and writes the same tagged source URL and digest into both
recipes and `packaging/bioconda/BIOCONDA_PR.md`.

## Evidence

Before submitting, keep the release evidence green:

```bash
python3 tools/update_real_data_manifest.py \
  --entry benchmarks/real-data/gatk-na12878-mito/evidence/manifest-entry.json \
  --entry benchmarks/real-data/picard-snvq/evidence/manifest-entry.json \
  --replace
python3 tools/verify_benchmark_suite_coverage.py
python3 tools/verify_benchmark_thresholds.py
python3 tools/verify_ci_coverage.py
python3 tools/verify_parity_docs.py
python3 tools/verify_readme_links.py
python3 tools/verify_site_links.py
python3 tools/verify_workflow_starters.py
python3 tools/verify_real_data_evidence.py
python3 tools/verify_real_data_evidence.py --release-ready
python3 tools/verify_bioconda_recipes.py --release-ready
```

The current release evidence covers:

AddOrReplaceReadGroups, BuildBamIndex, CleanSam,
CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics,
CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, SortSam,
ValidateSamFile, ViewSam.

`CITATION.cff` cites the archived turbo-picard release. Benchmark and
validation inputs are cited separately with source URLs, commits or accessions,
and SHA-256 hashes.

Benchmark numbers are release evidence only while the saved suite keeps full
parity, at least `5.00x` floor speedup, at least `20.00x` geometric mean
speedup, and at least `50.00x` top speedup.

## Bioconda checkout

After copying both recipes into `bioconda-recipes`, lint and build them together:

```bash
cp -R packaging/bioconda/turbo-picard recipes/turbo-picard
cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim
bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim
bioconda-utils build --docker --mulled-test turbo-picard
bioconda-utils build --docker --mulled-test turbo-picard-picard-shim
```

The build writes `THIRDPARTY.yml` with `cargo-bundle-licenses`, and both
`LICENSE` and `THIRDPARTY.yml` are listed as license files.
