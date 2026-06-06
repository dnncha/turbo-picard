# turbo-picard

[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.20541928.svg)](https://doi.org/10.5281/zenodo.20541928)

![Abstract benchmark bars and sequencing-read streams](docs/site/assets/hero-pipeline.svg)

`turbo-picard` is a faster, more pipeline-scalable Rust implementation of
selected Picard commands. It is for people who already run Picard in WDL,
Nextflow, Snakemake, or shell pipelines and want the same command shape with a
lot less waiting and a lot less JVM-era memory pressure on the steps that hurt
most.

The saved public benchmark suite currently shows `32/32` parity-checked
commands passing, a `26.74x` geometric mean speedup, an `84.46x` top speedup,
and an `8.55x` floor speedup versus Picard 3.4.0. The checked MarkDuplicates
performance run in this repo also dropped median RSS from about `1.2 GB` in
Picard to about `8.7 MB` in `turbo-picard`, which is the kind of difference
that matters when the same workflow step fans out across many samples.

The command shape stays familiar: Picard command names, Picard-style
`KEY=VALUE` arguments, and a practical migration path where you swap one step,
check it, then move on.

It is not a full Picard replacement. Native command coverage is documented and
tested against Picard 3.4.0. Unsupported commands fail clearly, or can still go
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

## Why People Switch

- The command line still looks like Picard, so existing pipeline code does not
  need a conceptual rewrite.
- The current saved benchmark suite shows `32/32` parity-checked commands with
  a `26.74x` geometric mean speedup, an `84.46x` top speedup, and an `8.55x`
  floor speedup.
- The current checked `MarkDuplicates` performance run cut median RSS from
  about `1.2 GB` in Picard to about `8.7 MB`, which makes high-fanout pipeline
  runs easier to schedule.
- You can prove one command on your own data before changing a whole workflow.
- Unsupported commands do not get guessed at. They fail clearly or go through
  upstream Picard when fallback is configured.

## Good First Targets

These are usually the best places to start if you want a fast answer about
whether `turbo-picard` is worth adopting in your environment:

- `MarkDuplicates` when duplicate marking is dragging a preprocessing run.
- `SortSam` when you are repeatedly reordering BAM or CRAM between stages.
- `SamToFastq` when Picard export is still sitting in an alignment or remap path.
- `BuildBamIndex` and small VCF utilities when pipeline glue work keeps adding up.
- Metrics commands when iteration speed matters more than Picard's exact plot rendering.

The right first trial is usually one slow, easy-to-compare command on one
representative shard.

## Choose An Install Path

- `PyPI`: fastest local try, especially on macOS Apple Silicon.
- `Source`: best when you are already working in the repo or want the current checkout.
- `Container`: useful for pinned runtime behavior in cloud jobs and workflow profiles.
- `Bioconda`: best target for Linux clusters and shared scientific environments once accepted.

## Quickstart

```bash
python3 -m pip install turbo-picard
turbo-picard --version
turbo-picard MarkDuplicates --help
```

Then try one command on a representative file:

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

PyPI currently has a macOS Apple Silicon wheel and a source distribution. If
`pip` builds from source, you will need Rust and the native build dependencies
available. For Linux clusters, Bioconda will be the cleaner install path once
the recipe is accepted.

Installing from PyPI currently gives you both commands:

- `turbo-picard`, the explicit command to use while testing.
- `picard`, the compatibility shim for existing scripts.

Use a dedicated virtual environment if you need upstream Picard and the shim
side by side.

## Who This Fits

- `WDL` and `Cromwell` users who want to replace one expensive Picard task without rewriting the task interface.
- `Nextflow` and `nf-core` maintainers who want a pinned binary or container with familiar command shape.
- `Snakemake` users who already shell out to Picard and want a faster command in the same slot.
- Shell pipeline owners who want explicit side-by-side checks before changing production behavior.

## Workflow Examples

`WDL` task command:

```wdl
command <<<
  turbo-picard MarkDuplicates \
    I=~{input_bam} \
    O=~{sample_id}.marked.bam \
    M=~{sample_id}.metrics.txt \
    ASSUME_SORTED=true
>>>
```

`Nextflow` process script:

```nextflow
def picard = params.use_turbo_picard ? 'turbo-picard' : 'picard'
"""
${picard} SortSam I=${bam} O=${meta.id}.sorted.bam SORT_ORDER=coordinate
"""
```

`Snakemake` shell step:

```python
shell:
    "turbo-picard BuildBamIndex I={input.bam} O={output.bai}"
```

More detailed Nextflow and nf-core notes live in
[`packaging/nf-core/README.md`](packaging/nf-core/README.md).
Starter files for `WDL`, `Nextflow`, and `Snakemake` live in
[`packaging/workflows/`](packaging/workflows/).
That starter bundle also explains which file to begin with and which command is
usually the best first trial for each workflow shape.
It now includes starter examples for `MarkDuplicates`, `SortSam`,
`SamToFastq`, and `BuildBamIndex`.
If you are not sure where to begin, start with
[`choose-your-first-command.md`](packaging/workflows/choose-your-first-command.md).
There are also short walkthroughs for
[`WDL / Cromwell`](packaging/workflows/wdl-cromwell.md),
[`Nextflow / nf-core`](packaging/workflows/nextflow-nf-core.md), and
[`Snakemake`](packaging/workflows/snakemake.md).
For the smallest honest evaluation flow, see
[`one-command-trial.md`](packaging/workflows/one-command-trial.md) plus the
tiny [`trial.wdl`](packaging/workflows/trial.wdl) and
[`trial.nf`](packaging/workflows/trial.nf) workflows.
## When It Helps

The best first use is one expensive Picard step that you can compare easily:
sorting, duplicate marking, FASTQ conversion, indexing, VCF housekeeping, or a
metrics command that keeps slowing down iteration. Run Picard and
`turbo-picard` beside each other on a representative file, compare the outputs
that matter for that command, then switch only that checked step.

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

- [Is this for you?](https://turbo-picard.readthedocs.io/en/latest/is-this-for-you.html)
  for a quick fit / not-fit decision before you spend time evaluating it.
- [Choose your first command](https://turbo-picard.readthedocs.io/en/latest/first-command.html)
  for picking the best first Picard step to test.
- [Evaluation playbook](https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html)
  for the shortest path from first interest to trial, review, and rollout.
- [Quickstart](https://turbo-picard.readthedocs.io/en/latest/quickstart.html)
  for installation and first commands.
- [Command coverage](https://turbo-picard.readthedocs.io/en/latest/commands.html)
  for what is native, partly native, or delegated.
- [Picard vs turbo-picard](https://turbo-picard.readthedocs.io/en/latest/picard-vs-turbo-picard.html)
  for a plain comparison of what stays familiar, what changes, and when not to switch.
- [FAQ](https://turbo-picard.readthedocs.io/en/latest/faq.html)
  for direct answers to common evaluation and rollout questions.
- [What parity means](https://turbo-picard.readthedocs.io/en/latest/parity.html)
  for what the comparisons prove and what they do not.

More guides, including workflow use cases, benchmarks, performance notes,
packaging, and citation, are listed on the docs index:

- [Docs index](https://turbo-picard.readthedocs.io/en/latest/)

The docs source is in [`docs/`](docs/).

If you are deciding whether to try this in a real workflow, start with
[Evaluation playbook](https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html),
then read
[Quickstart](https://turbo-picard.readthedocs.io/en/latest/quickstart.html),
[Trying it in a pipeline](https://turbo-picard.readthedocs.io/en/latest/adoption.html).

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
Workflow starter files live in [`packaging/workflows/`](packaging/workflows/).

## Install From PyPI

```bash
python3 -m pip install turbo-picard
```

Use this for a quick local try. For the full install notes, including the
`picard` shim and Linux source-build caveat, see the
[quickstart](https://turbo-picard.readthedocs.io/en/latest/quickstart.html).

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
