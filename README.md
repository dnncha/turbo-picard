# turbo-picard

[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.20541928.svg)](https://doi.org/10.5281/zenodo.20541928)

![Abstract benchmark bars and sequencing-read streams](docs/site/assets/hero-pipeline.svg)

`turbo-picard` is a faster Rust implementation of selected Picard commands.
It is built for people who already trust Picard, already have it wired into
WDL, Nextflow, Snakemake, or shell scripts, and mostly want one thing: make
the slow Picard step stop dominating the run.

The command shape stays familiar: Picard command names, Picard-style
`KEY=VALUE` arguments, and the same habit of replacing one pipeline step at a
time.

It is not a full Picard replacement. Native command coverage is documented and
tested against Picard 3.4.0. Unsupported commands fail clearly, or can be sent
to upstream Picard if you configure a fallback.

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

CRAM is supported on the hot preprocessing path when you pass a reference FASTA
with Picard-compatible `REFERENCE_SEQUENCE` (or set `TURBO_PICARD_REFERENCE`):

```bash
export TURBO_PICARD_REFERENCE=/path/to/reference.fa
turbo-picard SortSam I=reads.cram O=sorted.cram SORT_ORDER=coordinate R=$TURBO_PICARD_REFERENCE
```

There is also an optional `picard` shim for environments that already call a
binary named `picard`.

## When It Helps

The best first use is one expensive Picard step that you can compare easily:
sorting, duplicate marking, FASTQ conversion, indexing, VCF housekeeping, or a
metrics command that slows down iteration. Run Picard and `turbo-picard` beside
each other on a representative file, compare the outputs that matter for that
command, then switch only that checked step.

Use the explicit `turbo-picard` command while testing. Add the optional
`picard` shim only when you deliberately want existing pipeline code to resolve
to `turbo-picard`.

## When To Stay With Picard

Stay with upstream Picard for commands or options outside the documented native
scope, for workflows that depend on exact Picard-rendered chart PDFs, and for
any step you have not compared on data that looks like your own. Fallback is
there so mixed pipelines can keep moving; it is not a reason to skip validation.

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
- [Performance notes](https://turbo-picard.readthedocs.io/en/latest/performance.html)
  for thread controls, CRAM throughput, and the accelerator preflight.
- [What parity means](https://turbo-picard.readthedocs.io/en/latest/parity.html)
  for what the comparisons prove and what they do not.
- [Trying it in a pipeline](https://turbo-picard.readthedocs.io/en/latest/adoption.html)
  for trying it safely in an existing pipeline.
- [Citation](https://turbo-picard.readthedocs.io/en/latest/citation.html)
  for citing the software and the input data correctly.
- [Packaging](https://turbo-picard.readthedocs.io/en/latest/packaging.html)
  for the main package, shim package, and Bioconda notes.

The docs source is in [`docs/`](docs/).

## Check Your Own Data

Before switching a pipeline step, run Picard and `turbo-picard` on a
representative file from that workflow and keep the comparison with the
analysis. The helper below writes the command lines, versions, input identity,
stable output digests, and timings into one directory:

```bash
python3 tools/audit_real_data.py \
  --input-bam /data/representative.bam \
  --input-source-url https://example.org/accession.bam \
  --input-source-commit <40-char-sha-or-accession> \
  --output-dir benchmarks/real-data/my-workflow/evidence \
  --dataset-id my-workflow \
  --picard-command "picard" \
  --turbo-picard-command ./target/release/picard \
  --skip-build
```

See [Trying it in a pipeline](https://turbo-picard.readthedocs.io/en/latest/adoption.html)
for the full validation protocol.

## Container image

```bash
docker build -t turbo-picard:local .
docker run --rm turbo-picard:local MarkDuplicates --help
```

nf-core and Nextflow examples live in [`packaging/nf-core/README.md`](packaging/nf-core/README.md).

## Install From PyPI

```bash
python3 -m pip install turbo-picard
```

Installing from PyPI currently gives you both commands:

- `turbo-picard`, the explicit command to use while testing.
- `picard`, the compatibility shim for existing scripts.

That means a virtual environment with this package installed may resolve
`picard` to `turbo-picard`. Use a dedicated environment if you need upstream
Picard and the shim side by side.

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
turbo-picard AccelerationStatus
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

Benchmark note: `AccelerationStatus` is listed under benchmark exceptions
because it is a status/preflight command with no Picard data-processing runtime
to benchmark. Every native or partly native data-processing command in
`docs/command-matrix.yml` has a saved public speedup claim. Chart-producing
metrics commands compare metrics text. Their lightweight PDF sidecars are there
so Picard-style outputs still exist, not because the plots are claimed to be
pixel-identical to Picard.

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

CRAM preprocessing is checked against Picard for the commands people usually
care about in alignment-preprocessing workflows:

```bash
./tools/verify_basic_cram_parity.sh
./tools/verify_markdup_cram_parity.sh
./tools/verify_gatk_preprocessing_combo_parity.sh
./tools/verify_gatk_mito_bam_parity.sh
./tools/verify_gatk_mito_cram_parity.sh
./tools/verify_gatk_preprocessing_combo_cram_parity.sh
```

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
- `gatk-na12878-mito-cram`: the same shard as CRAM with assembly38 mt-only reference.
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

The `v0.1.1` release is archived on Zenodo:
[10.5281/zenodo.20541928](https://doi.org/10.5281/zenodo.20541928).
Use the DOI for the archived release you actually used.

A short JOSS-style software paper draft is in [`paper/`](paper/). It is kept in
the repository so the software is ready to cite properly later, but it should
not be submitted to JOSS yet. The project needs more public development history
before a submission would be fair to reviewers. Check the paper with:

```bash
python3 tools/verify_joss_paper.py
```

The submission checklist is in [`docs/joss-submission.md`](docs/joss-submission.md).

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

Bug reports, parity evidence, documentation fixes, and small command-coverage
improvements are welcome. Please start with [`CONTRIBUTING.md`](CONTRIBUTING.md).

Before adding or widening a native command, check
[`docs/command-matrix.yml`](docs/command-matrix.yml). Changes should include
tests, a clear command-coverage update, and documentation that says plainly what
is supported.

For the development workflow, see the
[development docs](https://turbo-picard.readthedocs.io/en/latest/development.html).
Support expectations are in [`SUPPORT.md`](SUPPORT.md), and security reporting
is in [`SECURITY.md`](SECURITY.md).
