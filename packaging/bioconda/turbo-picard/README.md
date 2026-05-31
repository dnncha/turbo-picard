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
exist. Before opening a Bioconda PR:

1. Cut a GitHub release for the exact commit being packaged.
2. Replace `source.path` with the release archive URL.
3. Add the release archive `sha256`.
4. Confirm `about.home`, `about.summary`, and `recipe-maintainers`.
5. Regenerate and review the real-data replacement evidence. Every public
   dataset used for confidence claims should be pinned in
   `benchmarks/real-data/manifest.json` with source URL, immutable source
   commit, local input hash, passing command comparisons, and the rendered
   evidence report. At least one larger public or production-like run must be
   marked `release_tier: release_candidate`; the current HTSlib fixture remains
   `release_tier: public_smoke`.
6. Build both recipes locally.
7. Build with Bioconda's Docker/mulled test path from a `bioconda-recipes`
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
and `sha256`:

```bash
python3 tools/update_real_data_manifest.py \
  --entry benchmarks/real-data/HG001-smoke/manifest-entry.json
python3 tools/verify_real_data_evidence.py
python3 tools/verify_real_data_evidence.py --release-ready
python3 tools/verify_bioconda_recipes.py --release-ready
```

Include the real-data evidence summary in the Bioconda PR description. At a
minimum, link the manifest entry, the pinned public input source, the input
SHA-256, the generated comparison report, and the commands that pass against
Picard. The current public fixture is a smoke test; do not describe it as
production-scale validation until larger public datasets are added to the
manifest and pass the same verifier.

Bioconda-style smoke after adding the recipe to a bioconda-recipes checkout:

```bash
bioconda-utils build --docker --mulled-test turbo-picard
```

Submit the shim recipe in the same PR only if reviewers are comfortable with the
explicit command-name conflict:

```bash
bioconda-utils build --docker --mulled-test turbo-picard-picard-shim
```
