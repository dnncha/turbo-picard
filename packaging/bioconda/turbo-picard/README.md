# Bioconda Packaging Notes

This directory contains a Bioconda-oriented recipe for local packaging smoke
tests and eventual submission. The main recipe installs only `turbo-picard`, so
it can coexist with upstream Picard. The sibling `turbo-picard-picard-shim`
recipe installs the optional `picard` command name and is intentionally separate.

The recipe intentionally uses:

```yaml
source:
  path: ../../..
```

That keeps packaging tests tied to the working tree before release artifacts
exist. Do not open a Bioconda PR while the recipe still uses `source.path`;
that local source block is only for smoke testing this repository. Copy the
recipe into `bioconda-recipes` only after the tagged archive URL and SHA-256 are
written and the release-ready verifier passes.

Commit the intended release state before tagging. The preflight command reports
a dirty worktree as a release wait state so the source archive is not cut from
the wrong commit.

Before opening a Bioconda PR:

1. Cut a GitHub release for the exact commit being packaged.
2. Download the GitHub release archive.
3. Replace `source.path` with the tagged release archive URL and computed
   SHA-256, and update `packaging/bioconda/BIOCONDA_PR.md` with the same
   source archive:

   ```bash
   python3 tools/bioconda_release_preflight.py
   python3 tools/prepare_bioconda_release.py \
     --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
   ```

   The preflight command summarizes the checks that are already green and calls
   out the expected wait state while the recipes still use `source.path`.

   Prefer `--archive` for release submission because it validates the downloaded
   GitHub source archive before writing the digest. If you must pass a digest
   explicitly, use `--sha256` only when it was computed from the downloaded
   GitHub source archive; that fallback skips archive filename and content
   validation. The helper refuses to hash an archive whose filename does not
   match the recipe version. For `0.1.0`, use `turbo-picard-0.1.0.tar.gz` or
   GitHub's `v0.1.0.tar.gz`. It also checks for a GitHub-style source tarball
   with a `turbo-picard-0.1.0/` top-level directory, workspace `Cargo.toml`,
   `Cargo.lock`, `CITATION.cff`, `docs/command-matrix.yml`, `docs/parity.rst`,
   `benchmarks/real-data/manifest.json`, `docs/site/assets/benchmark-data.json`,
   `packaging/bioconda/turbo-picard/meta.yaml`, and
   `packaging/bioconda/turbo-picard-picard-shim/meta.yaml`. It rejects unsafe
   paths, duplicate entries, unsupported tar member types, and empty required
   source files. It also checks archive-internal metadata: the workspace
   version, `CITATION.cff` archived-release citation fields, the
   `picard_reference` entry for Picard 3.4.0, the `datasets` and `benchmarks`
   JSON evidence sections, and matching recipe version and source block
   metadata before writing the same URL and SHA-256 into the draft Bioconda PR
   body.

4. Confirm `about.home`, `about.summary`, and `recipe-maintainers`.
5. Regenerate and review the real-data switching evidence. Every public
   dataset used for confidence claims should be pinned in
   `benchmarks/real-data/manifest.json` with source URL, immutable source
   commit, local input hash, passing command comparisons, and the rendered
   evidence report. At least one larger public or workflow-representative run must be
   marked `release_tier: release_candidate`; the checked-in GATK NA12878
   mitochondrial and Picard SNVQ bundles are the current release-candidate
   evidence.
6. Build both recipes locally.
7. Lint and build with Bioconda's Docker/mulled test path from a `bioconda-recipes`
   checkout.

The build generates `THIRDPARTY.yml` with `cargo-bundle-licenses`, matching the
Bioconda Rust packaging guidance for dependency license metadata.

Local conda-build smoke:

```bash
python3 tools/verify_bioconda_recipes.py
python3 tools/verify_real_data_evidence.py
conda build packaging/bioconda/turbo-picard
```

Release-ready check after replacing `source.path` with the tagged release URL
and `sha256`, and after the helper has updated the draft PR body:

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
python3 tools/verify_real_data_evidence.py
python3 tools/verify_real_data_evidence.py --release-ready
python3 tools/prepare_bioconda_release.py \
  --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
python3 tools/verify_bioconda_recipes.py --release-ready
```

Include the real-data evidence summary in the Bioconda PR description. At a
minimum, link the manifest entry, the pinned public input source, the input
SHA-256, the generated comparison report, and the commands that pass against
Picard. The GATK NA12878 mitochondrial and Picard SNVQ fixtures are now the
minimum checked-in release-candidate evidence; larger public datasets or
workflow-owned representative data should still be added before making broad
cohort-scale switching claims.
The release-candidate command portfolio expected in the Bioconda PR is:
AddOrReplaceReadGroups, BuildBamIndex, CleanSam,
CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics,
CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, SortSam,
ValidateSamFile, ViewSam.
Also include the software citation boundary: `CITATION.cff` cites the
archived turbo-picard release, while benchmark and validation inputs must be cited
separately with immutable source URLs, commits or accessions, and SHA-256
hashes.
Use `packaging/bioconda/BIOCONDA_PR.md` as the starting PR body so the shim
conflict, release source, and evidence citations stay visible to reviewers.
The benchmark threshold gate requires full saved benchmark parity, at least
`5.00x` floor speedup, at least `20.00x` geometric mean speedup, and at least
`50.00x` top speedup before benchmark numbers are used as release evidence.

Bioconda-style smoke after adding the recipe to a bioconda-recipes checkout:

```bash
cp -R packaging/bioconda/turbo-picard recipes/turbo-picard
cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim
bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim
bioconda-utils build --docker --mulled-test turbo-picard
```

Submit the shim recipe in the same PR only if reviewers are comfortable with the
explicit command-name conflict:

```bash
bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim
bioconda-utils build --docker --mulled-test turbo-picard-picard-shim
```
