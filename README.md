# Turbo Picard

[![CI](https://github.com/dnncha/turbo-picard/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/dnncha/turbo-picard/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/turbo-picard.svg)](https://pypi.org/project/turbo-picard/)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.20541927.svg)](https://doi.org/10.5281/zenodo.20541927)

[Documentation](https://turbo-picard.readthedocs.io/en/latest/) · [Command coverage](docs/commands.rst) · [Evaluate on your data](docs/real-data-evaluation.rst) · [Research](https://cheerfulduck.com/research)

## Run a selected Picard step in Rust

Run selected Picard tools in Rust, using the command names and arguments your
pipelines already understand. Evaluate one command before changing a pipeline.

Turbo Picard is built for teams maintaining SAM/BAM/CRAM, VCF and sequencing-QC
steps in Nextflow, WDL, Snakemake and shell workflows. Native commands avoid the
JVM; the documented interface keeps migration local to the task you replace.

Start with one representative input. Native coverage is command- and
option-specific; this is not the full upstream suite. Keep Picard for work
outside the documented scope.

For coding agents and workflow generators, inspect the complete decision
surface in one call:

```bash
turbo-picard capabilities --json
```

The schema-versioned response contains every Picard command's native/fallback
status and trial fit together with the checked-in parity-gated benchmark
evidence. Use `turbo-picard trial --json <PicardCommand> ...` for the exact task
being considered. See the [agentic-coder guide](docs/agentic-coders.rst) for the
selection rule and safe substitution pattern.

### Compact, native-only automation

Version 0.1.13 adds `capabilities --json --command MarkDuplicates` for compact
discovery, executable argument arrays in trial JSON, and the strict
`TURBO_PICARD_REQUIRE_NATIVE=1` policy. See the
[agentic-coder guide](docs/agentic-coders.rst). Inspection reports describe
command scope, not proof that a particular input or option is validated.

## Quick Start

Install from PyPI:

```bash
python3 -m pip install turbo-picard==0.1.13
```

For a containerized trial, use the published release image:

```bash
docker run --rm ghcr.io/dnncha/turbo-picard:0.1.13 --version
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

## Compare before changing a workflow

The repository includes an evaluator that runs both implementations in separate
output paths and records versions, timings and output digests. It does not
upload your data. See the [real-data evaluation guide](docs/real-data-evaluation.rst)
for a copyable command and the interpretation of a match or mismatch.

The evaluator in this source checkout uses disk-backed sorting for large comparisons,
preserves existing evaluation directories, and retains failed runs for diagnosis.
The evaluator records command runtimes separately from comparison work;
improvements to its sorting helper do not establish faster native commands.

## Suitable first evaluations

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

## Cases that need upstream Picard or further validation

- You need an option or command that is not inside the documented native scope
  and cannot use fallback.
- You require Picard-equivalent chart rendering rather than checked metrics text.
- You have not compared the exact command, input shape, sidecars, metrics, exit
  code, and error behavior your workflow depends on.
- You need broad cohort evidence before trying a representative shard.

## Documentation

The [full documentation](https://turbo-picard.readthedocs.io/en/latest/) covers installation, command scope, evaluation, and maintenance.

Useful starting points:

- [Quickstart](https://turbo-picard.readthedocs.io/en/latest/quickstart.html)
- [Agentic coder guide](https://turbo-picard.readthedocs.io/en/latest/agentic-coders.html)
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

## Benchmark evidence

The saved suite records 32 command comparisons against Picard 3.4.0, each
checked against its specified output contract. These are small-fixture results:
startup overhead contributes substantially to the measured ratios. The saved
suite reports a 22.88x minimum, 84.52x geometric mean, and 272.12x maximum
speedup on those fixtures. In the saved
`MarkDuplicates` case, the generator used `reads=50000`; median wall times were
0.075464 seconds for Turbo Picard and 2.238986 seconds for Picard across three
runs. This is not a whole-genome performance estimate.

See the [benchmark guide](docs/benchmarks.rst) for methods and real-data scope,
the [saved summary](docs/benchmark-readme-reference.md) for all 32 measurements
and reproduction commands, and the [parity guide](docs/parity.rst) for what was
compared. Metrics-text agreement does not establish Picard-equivalent charts.

[Cheerful Duck Research](https://cheerfulduck.com/research) publishes our broader
bioinformatics software investigations, reproduction material, and corrections.

### Check the saved real-data record

The `benchmarks/real-data/` manifest records pinned inputs for
`gatk-na12878-mito`, `picard-snvq`, and `gatk-na12878-mito-cram`.
From a checkout, use the existing evidence tools:

```bash
python3 tools/update_real_data_manifest.py
python3 tools/verify_real_data_evidence.py
python3 tools/verify_real_data_evidence.py --release-ready
```

These commands check the repository's saved evidence. They do not validate a
new input file or replace a comparison of your own workflow.

## Workflow evaluation

The project publishes a [workflow validation protocol](docs/production-readiness.rst), a [compatibility contract](docs/compatibility-contract.rst), and a [production-scale benchmark format](benchmarks/production/README.md). Use these before changing a workflow. The opt-in Nextflow process candidate is documented under [packaging/nf-core](packaging/nf-core/README.md).

A command-level speedup is not a universal replacement claim. Keep upstream Picard available until representative BAM/CRAM evidence, output parity, failure behaviour, and independent review pass for the exact workflow.

## Packaging Status

The current source release is `0.1.13`. Release builds target Linux x86_64 and
ARM64, macOS Intel and Apple Silicon, plus a source distribution. Publication
is gated on artifact validation and installation smoke tests; the
[release page](https://github.com/dnncha/turbo-picard/releases/tag/v0.1.13)
and [PyPI](https://pypi.org/project/turbo-picard/0.1.13/) identify the published
artifacts. The Linux ARM64 wheel is cross-built and artifact-validated.

Read the [release notes](CHANGELOG.md) for the release scope and evidence
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
