# turbo-picard

![Abstract benchmark bars and sequencing-read streams](docs/site/assets/hero-pipeline.svg)

`turbo-picard` is a faster Rust implementation of selected Picard commands.
It keeps the familiar Picard command names and `KEY=VALUE` arguments, so it can
fit into existing WDL, Nextflow, Snakemake, and shell-scripted work without
making people learn a new command style.

It is not a full Picard replacement. The commands that are implemented natively
are documented and tested against Picard 3.4.0. Unsupported commands fail
clearly, or can be sent to upstream Picard if you configure a fallback.

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

There is also an optional `picard` shim for environments that already call a
binary named `picard`.

## Documentation

The full docs are on Read the Docs:

**https://turbo-picard.readthedocs.io/en/latest/**

Good starting points:

- [Quickstart](https://turbo-picard.readthedocs.io/en/latest/quickstart.html)
  for installation and first commands.
- [Command coverage](https://turbo-picard.readthedocs.io/en/latest/commands.html)
  for what is native, partly native, or delegated.
- [Fallback behavior](https://turbo-picard.readthedocs.io/en/latest/fallback.html)
  for using upstream Picard beside turbo-picard.
- [Benchmarks](https://turbo-picard.readthedocs.io/en/latest/benchmarks.html)
  for the saved benchmark data and how it is checked.
- [What parity means](https://turbo-picard.readthedocs.io/en/latest/parity.html)
  for what the comparisons prove and what they do not.
- [Trying it in a pipeline](https://turbo-picard.readthedocs.io/en/latest/adoption.html)
  for trying it safely in an existing pipeline.
- [Citation](https://turbo-picard.readthedocs.io/en/latest/citation.html)
  for citing the software and the input data correctly.
- [Packaging](https://turbo-picard.readthedocs.io/en/latest/packaging.html)
  for the main package, shim package, and Bioconda notes.

The docs source is in [`docs/`](docs/).

## Why Use It

Picard is everywhere in sequencing pipelines. That is the point of this project:
keep the parts people rely on, make selected commands much faster, and be clear
about what has and has not been rebuilt.

Use `turbo-picard` when you want to:

- speed up Picard-heavy steps without changing argument style;
- test one command at a time against your own data;
- keep upstream Picard available for commands that are not native yet;
- make performance claims traceable to saved benchmark and parity evidence.

Start with the explicit `turbo-picard` command. Use the `picard` shim only when
you deliberately want it to stand in for Picard in a particular environment.

## Install From Source

From a repository checkout:

```bash
cargo install --locked --path crates/turbo-picard-cli --bin turbo-picard --bin picard
```

This installs:

- `turbo-picard`, the explicit command.
- `picard`, the optional compatibility shim.

## First Commands

```bash
turbo-picard --help
turbo-picard MarkDuplicates --help
turbo-picard SortSam --help
```

Example:

```bash
turbo-picard MarkDuplicates \
  I=input.bam \
  O=marked.bam \
  M=metrics.txt \
  ASSUME_SORTED=true \
  VALIDATION_STRINGENCY=SILENT
```

The shim accepts Picard-style calls:

```bash
picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

## Fallback To Picard

Unsupported commands fail by default. To let them run through upstream Picard,
set a fallback command:

```bash
export TURBO_PICARD_FALLBACK_COMMAND='java -jar /opt/picard/picard.jar'
```

Use an absolute path so the fallback cannot accidentally resolve back to the
`picard` shim. See the
[fallback documentation](https://turbo-picard.readthedocs.io/en/latest/fallback.html)
for the exact rules.

## Benchmarks

The benchmark suite compares each command with Picard and checks stable output
before reporting speed. The saved benchmark run currently reports:

- `32/32` benchmarked commands passed parity checks.
- `84.46x` top speedup: `UpdateVcfSequenceDictionary`.
- `8.55x` floor speedup: `RevertSam`.
- `26.82x` median speedup.
- `26.74x` geometric mean speedup.

Saved on `2026-06-04` from
`python3 tools/bench_suite.py --repeats 1 --skip-build`.
Raw log: `docs/site/assets/bench-suite-output.txt`.

There are no benchmark exceptions right now. Every native or partly native
command in `docs/command-matrix.yml` has a saved public speedup claim.
Chart-producing metrics commands compare metrics text; their PDF
sidecars are present so Picard-style outputs still exist, not because the plots
are claimed to be pixel-identical to Picard.

| Command | Speedup | Parity |
| --- | ---: | --- |
| UpdateVcfSequenceDictionary | 84.46x | PASS |
| NormalizeFasta | 68.69x | PASS |
| GatherVcfs | 63.77x | PASS |
| MergeVcfs | 56.99x | PASS |
| CreateSequenceDictionary | 50.86x | PASS |
| CollectGcBiasMetrics | 50.25x | PASS |
| SortSam | 47.40x | PASS |
| CollectInsertSizeMetrics | 46.41x | PASS |
| BuildBamIndex | 45.49x | PASS |
| CollectAlignmentSummaryMetrics | 37.50x | PASS |
| IntervalListTools | 33.01x | PASS |
| SamToFastq | 30.44x | PASS |
| ViewSam | 28.64x | PASS |
| BedToIntervalList | 27.91x | PASS |
| SortVcf | 27.87x | PASS |
| CleanSam | 26.82x | PASS |
| AddOrReplaceReadGroups | 26.17x | PASS |
| FastqToSam | 24.80x | PASS |
| CollectQualityYieldMetrics | 24.06x | PASS |
| MarkDuplicates | 23.19x | PASS |
| CollectWgsMetrics | 22.42x | PASS |
| MeanQualityByCycle | 21.66x | PASS |
| CollectMultipleMetrics | 20.48x | PASS |
| ValidateSamFile | 19.38x | PASS |
| LiftoverVcf | 15.13x | PASS |
| FixMateInformation | 14.54x | PASS |
| MergeSamFiles | 14.31x | PASS |
| ReplaceSamHeader | 14.07x | PASS |
| CollectBaseDistributionByCycle | 12.46x | PASS |
| QualityScoreDistribution | 11.40x | PASS |
| SetNmMdAndUqTags | 10.19x | PASS |
| RevertSam | 8.55x | PASS |

Useful checks:

```bash
python3 tools/verify_benchmark_log_evidence.py
python3 tools/verify_benchmark_suite_coverage.py
python3 tools/verify_benchmark_thresholds.py
python3 tools/verify_readme_benchmark_evidence.py
python3 tools/verify_site_benchmark_evidence.py
```

## Real Data

Synthetic benchmarks are not enough. The repository also keeps small public
real-data comparisons with pinned source URLs, full Git commits, SHA-256 input
hashes, Picard/turbo versions, command outputs, and digest comparisons.

The current checked evidence is in
[`benchmarks/real-data/`](benchmarks/real-data/) and described in the
[benchmark docs](https://turbo-picard.readthedocs.io/en/latest/benchmarks.html).
It includes:

- `gatk-na12878-mito`: a public GATK NA12878 mitochondrial BAM.
- `picard-snvq`: Picard's public SNVQ metrics test BAM.

To add another pinned dataset:

```bash
python3 tools/update_real_data_manifest.py \
  --entry benchmarks/real-data/<dataset-id>/evidence/manifest-entry.json
```

Then run:

```bash
python3 tools/verify_real_data_evidence.py
python3 tools/verify_real_data_evidence.py --release-ready
```

## Citation

Cite the archived turbo-picard release with [`CITATION.cff`](CITATION.cff).
Cite benchmark and validation inputs separately, using their source URLs,
commits or accessions, and SHA-256 hashes. The
[citation docs](https://turbo-picard.readthedocs.io/en/latest/citation.html)
spell out what to record.

Zenodo archiving is enabled for this repository. After the next GitHub release
is archived there, add the Zenodo DOI badge here and cite that archived release.

## Bioconda Status

Release `v0.1.1` has been submitted to Bioconda as two recipes:

- `turbo-picard`
- `turbo-picard-picard-shim`

The main package installs `turbo-picard`. The shim package installs the
optional `picard` command name.

Release checks:

```bash
python3 tools/bioconda_release_preflight.py
python3 tools/prepare_bioconda_release.py \
  --archive ~/Downloads/turbo-picard-0.1.1.tar.gz
python3 tools/verify_bioconda_recipes.py --release-ready
```

## Contributing

Before adding or widening a native command, check
[`docs/command-matrix.yml`](docs/command-matrix.yml). Changes should include
tests, a clear command-coverage update, and documentation that says plainly what
is supported.

For the development workflow, see the
[development docs](https://turbo-picard.readthedocs.io/en/latest/development.html).
