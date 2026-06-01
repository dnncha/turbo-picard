# turbo-picard-picard-shim

Opt-in Bioconda shim package that installs the `picard` command name for
`turbo-picard`. The main `turbo-picard` package does not install this binary so
it can coexist with upstream Picard.

Keep this recipe separate from the main package. It intentionally declares a
constraint against upstream `picard` because both packages expose the same
command name, and workflow owners should opt into that shadowing behavior.

Before submission, run the release source helper from the repository root after
creating the GitHub release archive:

```bash
python3 tools/prepare_bioconda_release.py \
  --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
python3 tools/verify_benchmark_suite_coverage.py
python3 tools/verify_benchmark_thresholds.py
python3 tools/verify_ci_coverage.py
python3 tools/verify_parity_docs.py
python3 tools/verify_readme_links.py
python3 tools/verify_site_links.py
python3 tools/verify_bioconda_recipes.py --release-ready
```

Prefer `--archive` for release submission because it validates the downloaded
GitHub source archive before writing the digest. If you must pass a digest
explicitly, use `--sha256` only when it was computed from the downloaded GitHub
source archive; that fallback skips archive filename and content validation. The
helper refuses to hash an archive whose filename does not match the recipe
version. For `0.1.0`, use `turbo-picard-0.1.0.tar.gz` or GitHub's
`v0.1.0.tar.gz`. It also checks for a GitHub-style source tarball with a
`turbo-picard-0.1.0/` top-level directory, workspace `Cargo.toml`,
`Cargo.lock`, `CITATION.cff`, `docs/command-matrix.yml`, `docs/parity.rst`,
`benchmarks/real-data/manifest.json`, `docs/site/assets/benchmark-data.json`,
`packaging/bioconda/turbo-picard/meta.yaml`, and
`packaging/bioconda/turbo-picard-picard-shim/meta.yaml`. It rejects unsafe
paths, duplicate entries, unsupported tar member types, and empty required
source files. It also checks archive-internal metadata: the workspace version,
`CITATION.cff` archived-release citation fields, the `picard_reference` entry
for Picard 3.4.0, the `datasets` and `benchmarks` JSON evidence sections, and
matching recipe version and source block metadata before writing the same URL
and SHA-256 into `packaging/bioconda/BIOCONDA_PR.md`.

Submit this recipe only alongside the main `turbo-picard` recipe, so the exact
`{{ pin_subpackage('turbo-picard', exact=True) }}` dependency remains valid.
Use the same PR body as the main package: cite the archived turbo-picard release
through `CITATION.cff`, and keep benchmark input citations separate with pinned
source URLs, commits or accessions, and SHA-256 hashes.
The shared release checklist also keeps the benchmark threshold gate visible:
full saved benchmark parity, at least `5.00x` floor speedup, at least `20.00x`
geometric mean speedup, and at least `50.00x` top speedup before benchmark
numbers are used as release evidence.
The paired Bioconda PR should also show the release-candidate command portfolio:
AddOrReplaceReadGroups, BuildBamIndex, CleanSam,
CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics,
CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, SortSam,
ValidateSamFile, ViewSam.
After copying both recipes into `bioconda-recipes`, lint the pair before the
Docker/mulled builds:

```bash
cp -R packaging/bioconda/turbo-picard recipes/turbo-picard
cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim
bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim
bioconda-utils build --docker --mulled-test turbo-picard
bioconda-utils build --docker --mulled-test turbo-picard-picard-shim
```
