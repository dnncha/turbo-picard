# Add turbo-picard

This PR adds `turbo-picard` and a separate optional compatibility shim.

Upstream submission: https://github.com/bioconda/bioconda-recipes/pull/65922

`turbo-picard` installs the `turbo-picard` command. It can be installed next to
the existing Bioconda `picard` package.

`turbo-picard-picard-shim` installs a `picard` command that forwards supported
Picard-style invocations to turbo-picard. It is split into its own recipe so the
`picard` command name is installed only when requested. The shim depends on the
matching `turbo-picard` version and declares `picard ==0` as a run constraint
because it owns the same command name as the existing Picard package.

turbo-picard is not a full Picard replacement. The supported commands are
documented, tested against Picard 3.4.0, and kept explicit.

## Recipes

- `turbo-picard`: native Rust binary package.
- `turbo-picard-picard-shim`: optional `picard` command shim.
- Both recipes are compiled packages, not `noarch`.
- Windows is skipped.
- Rust dependency licenses are bundled into `THIRDPARTY.yml`.

## Source

- URL: `https://github.com/dnncha/turbo-picard/archive/refs/tags/v0.1.9.tar.gz`
Archive SHA-256:
`1fcc3b7b4a86028bd4326378c18ebd7940564469f76029f8014051c457a8e7f8`

## Evidence

The release includes checked-in parity and benchmark evidence so the package
scope is reviewable.

Parity evidence:

- `benchmarks/real-data/manifest.json`
- `benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md`
- `benchmarks/real-data/gatk-na12878-mito-cram/evidence/real-data-comparison.md`
- `benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md`
- `docs/parity.rst`

Benchmark evidence:

- `python3 tools/bench_suite.py --repeats 3 --skip-build`
- `docs/site/assets/benchmark-data.json`
- `docs/site/assets/bench-suite-output.txt`

Current benchmark summary:

- Date: 2026-06-13.
- Parity: 32/32 PASS.
- Geometric mean speedup: 24.94x.
- Median speedup: 26.72x.
- Slowest saved speedup: 6.86x on RevertSam.
- Fastest saved speedup: 94.36x on UpdateVcfSequenceDictionary.
- Recently promoted benchmarks include IntervalListTools, LiftoverVcf,
  CollectMultipleMetrics, and CollectGcBiasMetrics.

The real-data fixtures are public, pinned, and small. They are suitable for this
package review, but they should not be read as a claim about every Picard
workflow.

## Citation

`CITATION.cff` cites the archived turbo-picard release. The benchmark and
validation inputs are cited separately with their source URLs, commits, and
SHA-256 hashes.

## Checks

Repository checks used to keep the recipe release-ready:

```bash
python3 tools/bioconda_release_preflight.py
python3 -m unittest discover tools
python3 tools/update_real_data_manifest.py
python3 tools/verify_real_data_evidence.py --release-ready
python3 tools/verify_bioconda_recipes.py --release-ready
python3 tools/verify_release_versions.py
python3 tools/verify_benchmark_suite_coverage.py
python3 tools/verify_benchmark_thresholds.py
python3 tools/verify_ci_coverage.py
python3 tools/verify_parity_docs.py
python3 tools/verify_readme_links.py
python3 tools/verify_site_links.py
python3 tools/verify_workflow_starters.py
./tools/verify_package_install.sh
cargo test --workspace
```

Expected Bioconda checkout checks:

```bash
bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim --full-report
bioconda-utils build --docker --mulled-test turbo-picard
bioconda-utils build --docker --mulled-test turbo-picard-picard-shim
```
