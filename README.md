# turbo-picard

[![CI](https://github.com/dnncha/turbo-picard/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/dnncha/turbo-picard/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/turbo-picard.svg)](https://pypi.org/project/turbo-picard/)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.20541927.svg)](https://doi.org/10.5281/zenodo.20541927)

**Evaluate selected Picard commands in Rust when a known preprocessing or QC
step is a runtime or memory bottleneck.**

`turbo-picard` keeps the workflow interface familiar: Picard command names,
`KEY=VALUE` arguments, and an optional `picard` compatibility shim. Accelerated
commands run natively. Commands outside the native surface can delegate to
upstream Picard when fallback is configured.

It is for bioinformatics teams maintaining Picard steps in WDL, Nextflow,
Snakemake, or shell pipelines. Start with one representative command, compare
the outputs your downstream workflow consumes, and keep upstream Picard for
commands or options outside the documented native scope.

## Quick Start

Install from PyPI:

```bash
python3 -m pip install turbo-picard
```

For a containerized trial, use the published release image:

```bash
docker run --rm ghcr.io/dnncha/turbo-picard:0.1.11 --version
```

Installing from PyPI currently gives you both commands:

- `turbo-picard`: the explicit command for evaluation and normal use.
- `picard`: a compatibility shim for environments where you deliberately want
  existing `picard` calls to resolve to this package.

Use the explicit `turbo-picard` command while testing. Add the shim to a
pipeline environment only after the specific commands you need have been
checked.

Check the install and print a trial contract before changing a workflow:

```bash
turbo-picard --version
turbo-picard MarkDuplicates --help
turbo-picard doctor
turbo-picard trial MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

The trial command prints matching Picard and turbo-picard invocations, declared
outputs, fallback state, and comparison notes. Then run the chosen command on a
representative input:

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

From a repository checkout:

```bash
cargo install --locked --path crates/turbo-picard-cli --bin turbo-picard --bin picard
```

## When It Helps

- You already run Picard commands and want to trial one slow step first.
- You need Picard-style command names and `KEY=VALUE` arguments to stay stable.
- You want a command-by-command rollout with upstream Picard available for
  unsupported or unchecked behavior.
- You can compare outputs on a representative BAM, CRAM, FASTQ, VCF, or metrics
  file before changing the workflow.

Good first trials are usually `MarkDuplicates`, `SortSam`, `SamToFastq`,
`FastqToSam`, `FixMateInformation`, `BuildBamIndex`, and repeated metrics
commands.
Use `turbo-picard trial <PicardCommand> ...` to print a side-by-side Picard and
turbo-picard evaluation contract before changing a workflow.

## When To Stay With Picard

- You need an option or command that is not inside the documented native scope
  and cannot use fallback.
- You require Picard-equivalent chart rendering rather than checked metrics text.
- You have not compared the exact command, input shape, sidecars, metrics, exit
  code, and error behavior your workflow depends on.
- You need broad cohort evidence before trying a representative shard.

## Documentation

The full docs are on Read the Docs:

**https://turbo-picard.readthedocs.io/en/latest/**

Useful starting points:

- [Quickstart](https://turbo-picard.readthedocs.io/en/latest/quickstart.html)
- [Is this for you?](https://turbo-picard.readthedocs.io/en/latest/is-this-for-you.html)
- [Choose your first command](https://turbo-picard.readthedocs.io/en/latest/first-command.html)
- [Evaluation playbook](https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html)
- [Command coverage](https://turbo-picard.readthedocs.io/en/latest/commands.html)
- [Picard alternatives](https://turbo-picard.readthedocs.io/en/latest/picard-alternatives.html)
- [Trying it in a pipeline](https://turbo-picard.readthedocs.io/en/latest/adoption.html)
- [Parity guide](https://turbo-picard.readthedocs.io/en/latest/parity.html)
- [Fallback to Picard](https://turbo-picard.readthedocs.io/en/latest/fallback.html)
- [Benchmarks](https://turbo-picard.readthedocs.io/en/latest/benchmarks.html)
- [Citation](https://turbo-picard.readthedocs.io/en/latest/citation.html)
- [Packaging](https://turbo-picard.readthedocs.io/en/latest/packaging.html)
- [Troubleshooting](https://turbo-picard.readthedocs.io/en/latest/troubleshooting.html)

Starter workflow files live in [`packaging/workflows/`](packaging/workflows/).
The smallest trial shape is
[`packaging/workflows/one-command-trial.md`](packaging/workflows/one-command-trial.md).
Migration patterns that usually keep the surrounding workflow stable include
per-read-group `SamToFastq`, sequential-shard `FastqToSam`, and
mate-repair boundaries around `FixMateInformation`.
If you run a real one-command evaluation, share the result through the
[trial report issue form](https://github.com/dnncha/turbo-picard/issues/new?template=trial-report.yml);
successful matches, mismatches, and adoption blockers are all useful evidence.
If GitHub does not offer new-issue creation, add the same redacted report as a
comment on the [public trial report thread](https://github.com/dnncha/turbo-picard/issues/4).
From a repository checkout, `tools/compare_real_data.py --shareable-report`
can create a reviewed, privacy-conscious starting point for that report.

## Benchmarks

The saved public benchmark suite compares native `turbo-picard` commands against
Picard 3.4.0 and checks stable outputs before reporting speed. Current saved
results report `32/32` parity checks passing, with `261.75x` top speedup:
`NormalizeFasta`, `22.17x` floor speedup: `SetNmMdAndUqTags`, `100.27x`
median speedup, and `87.47x` geometric mean speedup.

Summary: `32/32 PASS`; `261.75x` top speedup: `NormalizeFasta`;
`22.17x` floor speedup: `SetNmMdAndUqTags`; `100.27x` median speedup; `87.47x`
geometric mean speedup.

Benchmark details, scope notes, real-data evidence, and reproduction commands
are in the [benchmark docs](https://turbo-picard.readthedocs.io/en/latest/benchmarks.html).
The [parity guide](https://turbo-picard.readthedocs.io/en/latest/parity.html)
explains what the comparisons do and do not prove.
For `CollectBaseDistributionByCycle`, `CollectGcBiasMetrics`,
`CollectInsertSizeMetrics`, `MeanQualityByCycle`, and
`QualityScoreDistribution`, metrics text is the parity target; chart outputs are
lightweight PDF summaries, not Picard-equivalent rendered plots.

Saved benchmark run:

- Date: `2026-08-14`
- Command: `python3 tools/bench_suite.py --repeats 3 --skip-build`
- Raw log: `docs/site/assets/bench-suite-output.txt`
- benchmark exceptions: `AccelerationStatus`, `doctor`, `explain`, and `trial`
  are utility commands, not Picard workload comparisons.

| Command | Speedup | Parity |
| --- | ---: | :--- |
| NormalizeFasta | 261.75x | PASS |
| UpdateVcfSequenceDictionary | 239.57x | PASS |
| BuildBamIndex | 224.90x | PASS |
| ReplaceSamHeader | 205.88x | PASS |
| CollectGcBiasMetrics | 183.75x | PASS |
| CreateSequenceDictionary | 161.06x | PASS |
| GatherVcfs | 131.01x | PASS |
| CleanSam | 126.59x | PASS |
| AddOrReplaceReadGroups | 123.47x | PASS |
| CollectMultipleMetrics | 118.62x | PASS |
| CollectInsertSizeMetrics | 116.25x | PASS |
| LiftoverVcf | 115.03x | PASS |
| MergeVcfs | 110.53x | PASS |
| QualityScoreDistribution | 105.14x | PASS |
| MeanQualityByCycle | 104.88x | PASS |
| CollectQualityYieldMetrics | 100.27x | PASS |
| ValidateSamFile | 97.57x | PASS |
| IntervalListTools | 96.69x | PASS |
| CollectBaseDistributionByCycle | 94.74x | PASS |
| SortVcf | 82.23x | PASS |
| BedToIntervalList | 79.70x | PASS |
| ViewSam | 78.72x | PASS |
| CollectAlignmentSummaryMetrics | 78.25x | PASS |
| SortSam | 66.98x | PASS |
| SamToFastq | 66.11x | PASS |
| CollectWgsMetrics | 51.54x | PASS |
| MergeSamFiles | 38.84x | PASS |
| FixMateInformation | 31.27x | PASS |
| MarkDuplicates | 30.46x | PASS |
| FastqToSam | 25.23x | PASS |
| RevertSam | 23.69x | PASS |
| SetNmMdAndUqTags | 22.17x | PASS |

Release evidence checks:

```bash
python3 tools/update_real_data_manifest.py
python3 tools/verify_benchmark_log_evidence.py
python3 tools/verify_benchmark_suite_coverage.py
python3 tools/verify_benchmark_thresholds.py
python3 tools/verify_real_data_evidence.py
python3 tools/verify_real_data_evidence.py --release-ready
```

Real-data evidence lives in `benchmarks/real-data/` and records pinned input
sources, command scopes, and input SHA-256 hashes. Current release-candidate
dataset IDs are `gatk-na12878-mito`, `picard-snvq`, and
`gatk-na12878-mito-cram`.


## Workflow evaluation

The project publishes a [workflow validation protocol](docs/production-readiness.rst), a [compatibility contract](docs/compatibility-contract.rst), and a [production-scale benchmark format](benchmarks/production/README.md). Use these before changing a workflow. The opt-in Nextflow process candidate is documented under [packaging/nf-core](packaging/nf-core/README.md).

A command-level speedup is not a universal replacement claim. Keep upstream Picard available until representative BAM/CRAM evidence, output parity, failure behaviour, and independent review pass for the exact workflow.

## Packaging Status

The latest published PyPI release is `0.1.11`. It publishes Linux x86_64 and
macOS Apple Silicon wheels plus a source distribution. The current source
release is `0.1.12`; it remains a release candidate until the matching tag,
package, container, and evidence checks pass.

Read the [release notes](CHANGELOG.md) for the candidate scope and evidence
boundaries.

The submitted [Bioconda recipe PR](https://github.com/bioconda/bioconda-recipes/pull/65922)
covers the main package and an optional shim. Use PyPI or the container image
until Bioconda accepts the PR and the packages appear in its indexes. The main
package installs `turbo-picard`; the separate shim package installs the
`picard` command only for environments that choose it.

## Citation

Cite the archived `turbo-picard` release you used with [`CITATION.cff`](CITATION.cff).
Benchmark and validation inputs should be cited separately with immutable source
URLs, commits or accessions, and input SHA-256 hashes.

Docs source lives in [`docs/`](docs/). JOSS submission notes are tracked in
[`docs/joss-submission.rst`](docs/joss-submission.rst).

## Contributing

Bug reports, parity evidence, documentation fixes, and small command-coverage
improvements are welcome. Start with [`CONTRIBUTING.md`](CONTRIBUTING.md) and
the [development docs](https://turbo-picard.readthedocs.io/en/latest/development.html).

Support: [`SUPPORT.md`](SUPPORT.md). Security: [`SECURITY.md`](SECURITY.md).
