# turbo-picard

![Turbo Picard starship captain accelerating genomic pipeline data streams](docs/site/assets/turbo-picard-readme-hero.png)

**Faster Rust implementations of common Picard commands, built for Picard-shaped
pipelines.**

`turbo-picard` keeps the workflow interface familiar: Picard command names,
`KEY=VALUE` arguments, and an optional `picard` compatibility shim. Accelerated
commands run natively. Commands outside the native surface can delegate to
upstream Picard when fallback is configured.

Use it when you already have Picard steps in WDL, Nextflow, Snakemake, or shell
pipelines and want to test faster execution without rewriting task interfaces.

## Quick Start

Install from PyPI:

```bash
python3 -m pip install turbo-picard
```

Installing from PyPI currently gives you both commands:

- `turbo-picard`: the explicit command for evaluation and normal use.
- `picard`: a compatibility shim for environments where you deliberately want
  existing `picard` calls to resolve to this package.

Use the explicit `turbo-picard` command while testing. Add the shim to a
pipeline environment only after the specific commands you need have been
checked.

Check the install:

```bash
turbo-picard --version
turbo-picard MarkDuplicates --help
turbo-picard doctor
```

Run one familiar command:

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

## Benchmarks

The saved public benchmark suite compares native `turbo-picard` commands against
Picard 3.4.0 and checks stable outputs before reporting speed. Current saved
results report `32/32` parity checks passing, with `94.36x` top speedup:
`UpdateVcfSequenceDictionary`, `6.86x` floor speedup: `RevertSam`, `26.72x`
median speedup, and `24.94x` geometric mean speedup.

Summary: `32/32 PASS`; `94.36x` top speedup: `UpdateVcfSequenceDictionary`;
`6.86x` floor speedup: `RevertSam`; `26.72x` median speedup; `24.94x`
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

- Date: `2026-06-13`
- Command: `python3 tools/bench_suite.py --repeats 3 --skip-build`
- Raw log: `docs/site/assets/bench-suite-output.txt`
- benchmark exceptions: `AccelerationStatus` and `explain` are utility
  commands, not Picard workload comparisons.

| Command | Speedup | Parity |
| --- | ---: | :--- |
| UpdateVcfSequenceDictionary | 94.36x | PASS |
| BuildBamIndex | 69.26x | PASS |
| NormalizeFasta | 67.11x | PASS |
| GatherVcfs | 53.80x | PASS |
| CreateSequenceDictionary | 47.83x | PASS |
| MergeVcfs | 47.23x | PASS |
| CollectInsertSizeMetrics | 40.66x | PASS |
| MeanQualityByCycle | 36.74x | PASS |
| QualityScoreDistribution | 34.04x | PASS |
| CollectBaseDistributionByCycle | 33.08x | PASS |
| SamToFastq | 29.06x | PASS |
| CollectMultipleMetrics | 28.39x | PASS |
| IntervalListTools | 27.89x | PASS |
| CollectGcBiasMetrics | 27.72x | PASS |
| ValidateSamFile | 27.62x | PASS |
| SortSam | 26.72x | PASS |
| SortVcf | 25.67x | PASS |
| CollectAlignmentSummaryMetrics | 25.63x | PASS |
| AddOrReplaceReadGroups | 24.48x | PASS |
| CleanSam | 21.61x | PASS |
| ViewSam | 21.11x | PASS |
| BedToIntervalList | 19.89x | PASS |
| MarkDuplicates | 17.68x | PASS |
| CollectQualityYieldMetrics | 17.58x | PASS |
| MergeSamFiles | 17.16x | PASS |
| CollectWgsMetrics | 15.42x | PASS |
| ReplaceSamHeader | 14.20x | PASS |
| LiftoverVcf | 14.17x | PASS |
| FixMateInformation | 10.35x | PASS |
| SetNmMdAndUqTags | 9.34x | PASS |
| FastqToSam | 7.40x | PASS |
| RevertSam | 6.86x | PASS |

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

## Packaging Status

The live PyPI release is `0.1.8`. It publishes Linux x86_64 and macOS Apple
Silicon wheels plus a source distribution.

Bioconda recipes are tracked under [`packaging/bioconda/`](packaging/bioconda/).
The main package installs `turbo-picard`; the separate shim package installs the
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
