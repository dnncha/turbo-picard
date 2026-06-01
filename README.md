# turbo-picard

![turbo-picard branded hero](docs/site/assets/turbo-picard-branded-readme.png)

`turbo-picard` is for people who already have Picard in production and would
like the slow parts to be less slow. It keeps the command style that is already
in WDLs, Nextflow processes, Snakemake rules, shell scripts, and old lab notes:
Picard command names, Picard `KEY=VALUE` arguments, and a `picard` shim when you
are ready to test it in place.

When a command surface is implemented and checked, `turbo-picard` runs it
natively in Rust. When it is not ready, it should be obvious: the command fails
plainly or, if you configured a fallback, hands the work to upstream Picard.

```bash
picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

The goal is not to surprise anyone. The goal is familiar command lines,
published scope, repeatable comparisons, and no quiet claim that every Picard
behavior has already been rebuilt. Use the native pieces where the evidence
matches your workflow. Keep upstream Picard available for everything else.

## Documentation

The full docs are on Read the Docs:

**https://turbo-picard.readthedocs.io/en/latest/**

If you are trying the project for the first time, start here:

- [Quickstart](https://turbo-picard.readthedocs.io/en/latest/quickstart.html)
  for installation, entrypoints, and first commands.
- [Command coverage](https://turbo-picard.readthedocs.io/en/latest/commands.html)
  for native, partial, and fallback-supported Picard surfaces.
- [Adoption guide](https://turbo-picard.readthedocs.io/en/latest/adoption.html)
  for safe pipeline rollout, parity checks, and CI gates.
- [What parity means](https://turbo-picard.readthedocs.io/en/latest/parity.html)
  for the exact boundary between checked equivalence and broader validation.
- [Fallback behavior](https://turbo-picard.readthedocs.io/en/latest/fallback.html)
  for mixed deployments that still need upstream Picard.
- [Benchmarks](https://turbo-picard.readthedocs.io/en/latest/benchmarks.html)
  for reproducible performance checks tied to parity.
- [Citation](https://turbo-picard.readthedocs.io/en/latest/citation.html)
  for citing the software separately from pinned benchmark input data.
- [Packaging](https://turbo-picard.readthedocs.io/en/latest/packaging.html)
  for the main binary, the optional `picard` shim, and the Bioconda release path.
- [Troubleshooting](https://turbo-picard.readthedocs.io/en/latest/troubleshooting.html)
  for common setup and output-comparison issues.

The docs source lives in [`docs/`](docs/) if you want to read the raw files or
build them locally.

## Why Use It

Picard is deeply embedded in computational biology. That is not a problem to
work around; it is the contract this project has to respect. Labs and platform
teams have years of assumptions encoded in workflow definitions, containers,
validation reports, QC procedures, and methods sections.

`turbo-picard` is useful only if it fits into that world carefully. The native
commands are meant to speed up the parts that have been implemented and checked,
while unsupported behavior stays explicit instead of being guessed.

Use it when you want to:

- reduce wall-clock time for common Picard-heavy pipeline stages;
- keep familiar `KEY=VALUE` Picard command lines;
- evaluate switching behavior one command at a time;
- keep unsupported or uncommon surfaces routed to upstream Picard;
- make coverage, parity, and benchmark evidence explicit.

Do not flip a whole production pipeline at once. Treat `turbo-picard` as a
measured, reversible acceleration layer: prove the commands you use, keep the
evidence, then switch only the surfaces that are backed by your own checks.

## Install From Source

From a repository checkout:

```bash
cargo install --locked --path crates/turbo-picard-cli --bin turbo-picard --bin picard
```

This installs:

- `turbo-picard`, the explicit non-shadowing entrypoint.
- `picard`, a compatibility shim for workflow managers and scripts that already
  call Picard by command name.

Use `turbo-picard` while you are evaluating. Put the `picard` shim on `PATH`
only when you deliberately want it to stand in for upstream Picard in a specific
workflow or environment.

## First Command

```bash
turbo-picard MarkDuplicates \
  I=input.bam \
  O=marked.bam \
  M=metrics.txt \
  ASSUME_SORTED=true \
  VALIDATION_STRINGENCY=SILENT
```

The shim accepts the same Picard-style syntax:

```bash
picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

For command-specific behavior, start with local help, then check the command
coverage docs:

```bash
turbo-picard --help
turbo-picard MarkDuplicates --help
turbo-picard SortSam --help
```

## Fallback To Upstream Picard

By default, unsupported commands fail clearly. For mixed deployments, configure
an upstream Picard command:

```bash
export TURBO_PICARD_FALLBACK_COMMAND='java -jar /opt/picard/picard.jar'
```

Native commands still run natively. Unsupported commands and explicitly
unsupported native surfaces delegate to upstream Picard. Use an absolute JAR or
command path so the fallback cannot accidentally resolve back to the `picard`
shim.

See the [fallback documentation](https://turbo-picard.readthedocs.io/en/latest/fallback.html)
for the exact delegation rules.

## Adoption In Pipelines

For real workflows, start narrow:

1. Run `turbo-picard` beside upstream Picard on representative BAM, FASTQ, VCF,
   interval-list, and metrics-producing steps.
2. Compare outputs, sidecars, exit codes, and runtime for the command surfaces
   your workflow actually uses.
3. Run `tools/compare_real_data.py` on representative BAMs and keep the
   generated JSON, Markdown, manifest entry, source citation, and input SHA-256.
4. Add the relevant parity scripts and benchmark checks to CI.
5. Switch only proven surfaces to the `picard` shim, with upstream Picard
   configured as fallback where needed.

The [adoption guide](https://turbo-picard.readthedocs.io/en/latest/adoption.html)
goes deeper. The short version is: run it beside Picard first, compare
carefully, and only then switch. The
[parity guide](https://turbo-picard.readthedocs.io/en/latest/parity.html)
spells out what the checked comparisons mean, and what they do not prove.

## Benchmarks

Benchmark claims only matter when they stay tied to parity. This repository
keeps the numbers close to the evidence that produced them. The local suite runs
each benchmark against upstream Picard and fails if the stable output comparison
fails:

```bash
python3 tools/bench_suite.py --repeats 1 --skip-build
```

The project site and benchmark assets live under [`docs/site/`](docs/site/).
For the reproducible workflow, see the
[benchmark documentation](https://turbo-picard.readthedocs.io/en/latest/benchmarks.html).

Latest checked benchmark asset: `32/32` parity, `112.07x` top speedup: `UpdateVcfSequenceDictionary`, `7.40x` floor speedup: `RevertSam`, `26.24x` median speedup, `27.31x` geometric mean speedup. Saved on `2026-05-31` from `python3 tools/bench_suite.py --repeats 1 --skip-build`; raw log: `docs/site/assets/bench-suite-output.txt`.
The command matrix is broader than the speedup table. The benchmark exceptions
list is currently empty: every native or partial-native command in
`docs/command-matrix.yml` has a parity-checked public speedup claim. For
meta-commands and chart-producing metrics, the benchmarked scope is still
explicit: `CollectMultipleMetrics` uses `PROGRAM=CollectQualityYieldMetrics`,
and chart-producing commands compare metrics text rather than rendered plots.
For chart-producing metrics commands, the benchmarked parity target is the
metrics text; `CollectBaseDistributionByCycle`, `CollectGcBiasMetrics`,
`CollectInsertSizeMetrics`, `MeanQualityByCycle`, and
`QualityScoreDistribution` emit lightweight PDF chart sidecars.

The table below is intentionally plain. It is the checked benchmark snapshot,
not a marketing graphic.

| Command | Speedup | Parity |
| --- | ---: | --- |
| UpdateVcfSequenceDictionary | 112.07x | PASS |
| NormalizeFasta | 88.38x | PASS |
| CreateSequenceDictionary | 61.41x | PASS |
| BuildBamIndex | 59.88x | PASS |
| MergeVcfs | 48.06x | PASS |
| CollectInsertSizeMetrics | 46.42x | PASS |
| GatherVcfs | 45.45x | PASS |
| SamToFastq | 43.91x | PASS |
| CollectGcBiasMetrics | 39.92x | PASS |
| CleanSam | 37.49x | PASS |
| IntervalListTools | 34.07x | PASS |
| CollectAlignmentSummaryMetrics | 30.23x | PASS |
| ViewSam | 29.48x | PASS |
| SortVcf | 27.27x | PASS |
| CollectQualityYieldMetrics | 26.92x | PASS |
| SortSam | 26.24x | PASS |
| AddOrReplaceReadGroups | 25.07x | PASS |
| BedToIntervalList | 25.00x | PASS |
| MergeSamFiles | 24.64x | PASS |
| ValidateSamFile | 23.90x | PASS |
| CollectWgsMetrics | 22.54x | PASS |
| MeanQualityByCycle | 21.90x | PASS |
| CollectMultipleMetrics | 21.41x | PASS |
| MarkDuplicates | 20.59x | PASS |
| ReplaceSamHeader | 19.81x | PASS |
| FastqToSam | 18.12x | PASS |
| LiftoverVcf | 17.12x | PASS |
| CollectBaseDistributionByCycle | 13.82x | PASS |
| FixMateInformation | 11.72x | PASS |
| SetNmMdAndUqTags | 10.51x | PASS |
| QualityScoreDistribution | 10.39x | PASS |
| RevertSam | 7.40x | PASS |

Refresh the benchmark log and verify that the README and site still match with:

```bash
python3 tools/verify_benchmark_log_evidence.py
python3 tools/verify_benchmark_suite_coverage.py
python3 tools/verify_benchmark_thresholds.py
python3 tools/verify_readme_benchmark_evidence.py
python3 tools/verify_site_benchmark_evidence.py
```

The threshold verifier enforces full benchmark parity plus at least `5.00x`
floor speedup, `20.00x` geometric mean speedup, and `50.00x` top speedup before
those numbers are used as release evidence.

Synthetic benchmarks are useful, but they are not enough for biological
confidence. Real-data parity evidence is tracked separately, with pinned source
URLs, input hashes, Picard/turbo versions, command outputs, and digest
comparisons. That evidence lives under
[`benchmarks/real-data/`](benchmarks/real-data/) and is described in the
[benchmark documentation](https://turbo-picard.readthedocs.io/en/latest/benchmarks.html).

For GitHub-hosted public inputs, cite a `/blob/<commit>/` URL and the
full 40-character Git commit SHA; short hashes and branch names are not
accepted. Add a passing pinned dataset with:

```bash
python3 tools/update_real_data_manifest.py \
  --entry benchmarks/real-data/<dataset-id>/evidence/manifest-entry.json
```

Then run the normal evidence verifier:

```bash
python3 tools/verify_real_data_evidence.py
```

Before a scientist-facing release or Bioconda submission, run the stricter gate:

```bash
python3 tools/verify_real_data_evidence.py --release-ready
```

Current checked-in release-candidate evidence:

- `gatk-na12878-mito`: the GATK NA12878 mitochondrial test BAM currently
  passes `ViewSam`, `CleanSam`, `CollectQualityYieldMetrics`,
  `CollectAlignmentSummaryMetrics`, `MarkDuplicates`,
  `AddOrReplaceReadGroups`, `BuildBamIndex`, `RevertSam`, `SortSam`,
  `SamToFastq`, `CollectInsertSizeMetrics`, and `ValidateSamFile` against
  Picard 3.4.0. Source:
  `https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam`;
  commit `e8c49f600b06c658e0fa9bf67256340ebb46bc48`; SHA-256
  `70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3`;
  evidence
  `benchmarks/real-data/gatk-na12878-mito/evidence/real-data-comparison.md`;
  scope caveat `GATK public NA12878 mitochondrial test BAM.`; minimum input
  threshold `1000000` bytes.
- `picard-snvq`: the Picard SNVQ metrics test BAM currently passes `ViewSam`,
  `CleanSam`, `CollectQualityYieldMetrics`,
  `CollectAlignmentSummaryMetrics`, and `MarkDuplicates` against Picard 3.4.0.
  Source:
  `https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam`;
  commit `fc0b08410d38a10afd08e467dab74bf5e2e71310`; SHA-256
  `be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3`;
  evidence
  `benchmarks/real-data/picard-snvq/evidence/real-data-comparison.md`;
  scope caveat `Picard public SNVQ metrics test BAM.`; minimum input threshold
  `1000000` bytes.

Treat the checked-in release-candidate fixtures as the minimum release gate, not
as proof for broad cohort-scale switching claims. They are public, pinned, and
reviewable, but they are still small compared with the variety of real cohort
data. Add larger public shards or workflow-owned representatives before
widening the claim.

The release-candidate portfolio must cover this command set somewhere in the
pinned release-candidate evidence: AddOrReplaceReadGroups, BuildBamIndex,
CleanSam, CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics,
CollectQualityYieldMetrics, MarkDuplicates, RevertSam, SamToFastq, SortSam,
ValidateSamFile, ViewSam. The GATK NA12878 mitochondrial bundle currently
supplies that 12-command comparison. The aggregate release-candidate input
threshold is currently `10000000` bytes across pinned release-candidate inputs,
which keeps the release gate from being satisfied by a single token-sized
fixture.

## Citation

The software citation lives in [`CITATION.cff`](CITATION.cff). Cite the archived
release you used, then cite benchmark or validation inputs separately with their
own immutable source URLs, commits or accessions, and SHA-256 hashes. The
[citation documentation](https://turbo-picard.readthedocs.io/en/latest/citation.html)
lists the pieces to record in a methods section.

## Bioconda Status

The Bioconda recipes intentionally use the local checkout while release
artifacts are being prepared. That keeps the recipes testable before there is a
tagged source archive. Commit the intended release state before tagging; the
preflight command reports a dirty worktree as a release wait state. After
tagging a GitHub release, switch both recipes and the draft Bioconda PR body to
the immutable release archive with:

```bash
python3 tools/bioconda_release_preflight.py
python3 tools/prepare_bioconda_release.py \
  --archive ~/Downloads/turbo-picard-0.1.0.tar.gz
python3 tools/verify_bioconda_recipes.py --release-ready
```

The release helper checks the archive filename against the recipe version before
hashing it; for `0.1.0`, use `turbo-picard-0.1.0.tar.gz` or GitHub's
`v0.1.0.tar.gz`. It also checks that the archive opens as a GitHub-style source
tarball with a `turbo-picard-0.1.0/` top-level directory, workspace
`Cargo.toml`, `Cargo.lock`, `CITATION.cff`, `docs/command-matrix.yml`,
`docs/parity.rst`,
`benchmarks/real-data/manifest.json`, `docs/site/assets/benchmark-data.json`,
`packaging/bioconda/turbo-picard/meta.yaml`, and
`packaging/bioconda/turbo-picard-picard-shim/meta.yaml`. It rejects unsafe
paths, duplicate entries, unsupported tar member types, and empty required
source files. It also checks archive-internal metadata: the workspace version,
`CITATION.cff` archived-release citation fields, the `picard_reference` entry
for Picard 3.4.0, the `datasets` and `benchmarks` JSON evidence sections, and
matching recipe version and source block metadata. The same helper writes the
tagged source URL and computed SHA-256 into `packaging/bioconda/BIOCONDA_PR.md`,
so the PR text submitted to Bioconda matches the recipes.

## Chart Outputs

Several metrics commands now produce native metrics text and a simple one-page
PDF chart sidecar. The boundary is intentional: the metrics files are the
parity target for `CollectBaseDistributionByCycle`,
`CollectGcBiasMetrics`, `CollectInsertSizeMetrics`, `MeanQualityByCycle`, and
`QualityScoreDistribution`; the chart PDFs are present so existing Picard-style
workflows do not break when they request chart outputs. Do not treat those PDFs
as Picard-equivalent rendered plots yet.

## Contributing

The most useful contributions are command-surface improvements backed by tests,
parity checks, and clear documentation updates. Before broadening a native
implementation, check the machine-readable command matrix in
[`docs/command-matrix.yml`](docs/command-matrix.yml) and the contributor notes in
the [development documentation](https://turbo-picard.readthedocs.io/en/latest/development.html).
