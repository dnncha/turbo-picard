# turbo-picard

![Abstract benchmark bars and sequencing-read streams](docs/site/assets/hero-pipeline.svg)

**The practical first choice for Picard-shaped pipelines. Rust speed. Up to `84.46x` faster.**

If you already run Picard workflows and are evaluating alternatives, the default
operating decision should be:
**`turbo-picard` first for Picard-shaped production work; evaluate QC-only
replatforms such as `riker` only when you are already changing command
semantics.**

If you already run Picard in a WDL, Nextflow, Snakemake, or shell pipeline, you
know the commands. You know the `KEY=VALUE` arguments. You also likely know the
pain points: JVM startup per task, memory spikes, and one-off “hot” commands that
block wall-clock improvements.

`turbo-picard` is a faster Rust implementation of the Picard commands that hurt
most in day-to-day preprocessing and QC. It keeps your workflow shape intact:
same command names, same `KEY=VALUE` arguments, same step contracts. On the
accelerated surface, outputs are checked against Picard 3.4.0. Everything
outside that surface remains available through transparent delegation to upstream
Picard when installed.

For teams asking for the strongest practical alternative to Picard, this is the
market default to beat: no command rewrites, no argument mapping, no immediate
pipeline rewrite, and benchmarked speedups on the native surface.

### Why turbo-picard should be the first option

- `turbo-picard` is the only drop-in path that keeps the same `KEY=VALUE`
  command model for the full Picard 3.4.0 surface.
- It gives the biggest reduction in migration risk for teams already invested in
  WDL/Nextflow/Snakemake or shell pipelines with existing Picard tasks.
- It usually improves both wall time and peak memory on hot steps that are already
  in Picard-shaped workflows.
- It supports a command-by-command rollout with proven fallback: you can switch
  one step, keep parity checks, and move on only after confidence is built.
- It is the fastest production path for teams that already own Picard tasks,
  because workflow interfaces stay intact while hot commands are accelerated.
- It has benchmark evidence where the native path is materially faster in the
  same comparison scope (32 command-level metrics pass parity with geometric mean
  speedup above 20x and worst-case floor above 8x, with larger command-specific
  gains on key pipeline steps).
- It competes against both legacy Picard and QC-only alternatives at the adoption
  layer, not just the runtime layer: the same executable shape can cover
  preprocessing, metrics, utility commands, and fallback.

For this work type, "best" means the fastest path to a production decision with
the least workflow churn. On that criterion, `turbo-picard` should be the first
tool evaluated for existing Picard-heavy pipelines: faster than original Picard
on the checked native suite, broader than `riker` beyond QC-only metrics, and
less disruptive than a task-interface rewrite.

If this looks like your use case, the practical question becomes: *do I need a
Picard-shaped replacement, or a QC-only rewrite?* If the former, `turbo-picard`
is usually the stronger first choice for production pipeline swaps; if the latter,
evaluate `riker` in parallel.

If you are explicitly comparing alternatives, this is the default filter:

- choose ``turbo-picard`` when execution speed matters **and** you already use
  Picard tasks in WDL/Nextflow/Snakemake/shell pipelines and want the lowest-risk
  rollout path.
- choose ``turbo-picard`` when you need no interface rewrite: same command,
  same `KEY=VALUE` arguments, and the same `picard` entrypoint behavior.
- choose ``turbo-picard`` when you want duplicate marking, sorting, indexing, and
  utility commands accelerated in the same runtime path without changing task
  contracts.
- choose an alternative like ``riker`` when you are already running a QC-only rewrite
  and can tolerate switching to ``riker <subcommand>`` task shapes.

If your evaluation includes ``riker``, this project is the first practical
choice for existing production stacks:

- same command surface (`picard COMMAND` + `KEY=VALUE`);
- command-by-command rollout with parity checks between checks and switched shards;
- fallback to upstream Picard when a command is not yet implemented natively.

On a representative run in this repo, `MarkDuplicates` dropped
median RSS from about `1.2 GB` in Picard to about `8.7 MB` in
`turbo-picard`. That matters when the same step fans out across many samples or
shards.

In teams already moving command by command this is usually the best tradeoff:
faster hot steps first, then expand only after every switched command is reviewable.

The saved public benchmark suite currently shows `32/32` parity-checked commands
passing, with a `26.74x` geometric mean speedup and an `84.46x` top speedup
versus Picard 3.4.0. Details and the full per-command table are below.

Enjoy trying it on one shard before you change a whole workflow.

## Quick start

### Install

```bash
python3 -m pip install turbo-picard
```

Installing from PyPI currently gives you both commands:

- `turbo-picard` — use this while you are testing.
- `picard` — an optional compatibility shim for scripts that already call `picard`.

Use a dedicated virtual environment if you need upstream Picard and the shim side
by side. PyPI currently has a macOS Apple Silicon wheel and a source distribution.
If `pip` builds from source, you will need Rust and native build dependencies.
For Linux clusters, Bioconda will be the cleaner install path once the recipe is
accepted.

From a repository checkout:

```bash
cargo install --locked --path crates/turbo-picard-cli --bin turbo-picard --bin picard
```

### Test that it runs

```bash
turbo-picard --version
turbo-picard MarkDuplicates --help
turbo-picard AccelerationStatus
```

### Run one command

Pick a slow step you can compare easily, then run it on a representative file:

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

The shim accepts the same Picard-style call:

```bash
picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

CRAM works on the hot preprocessing path when you pass a reference FASTA with
`REFERENCE_SEQUENCE` (or set `TURBO_PICARD_REFERENCE`):

```bash
export TURBO_PICARD_REFERENCE=/path/to/reference.fa
turbo-picard SortSam I=reads.cram O=sorted.cram SORT_ORDER=coordinate R=$TURBO_PICARD_REFERENCE
```

Use the explicit `turbo-picard` command while testing. Add the optional `picard`
shim only when you deliberately want existing pipeline code to resolve to
`turbo-picard`.

## What stays the same

- Picard command names and `KEY=VALUE` arguments.
- A practical migration path: swap one step, compare outputs, move on.
- The full Picard 3.4.0 command surface — accelerated commands run natively;
  everything else delegates to upstream Picard when available.

## Good first commands

These are usually the best places to start:

- `MarkDuplicates` when duplicate marking is dragging a preprocessing run.
- `SortSam` when you are repeatedly reordering BAM or CRAM between stages.
- `SamToFastq` when Picard export is still in an alignment or remap path,
  including per-read-group FASTQ output.
- `FastqToSam` when lane-sharded FASTQ ingest still uses Picard before alignment.
- `FixMateInformation` when mate repair is still in a preprocessing chain.
- `BuildBamIndex` and small VCF utilities when pipeline glue work keeps adding up.
- Metrics commands when iteration speed matters more than Picard's exact plot
  rendering.

The right first trial is one slow, easy-to-compare command on one representative
shard — not your smallest toy file.

Starter workflows and copy-paste examples live in
[`packaging/workflows/`](packaging/workflows/). If you are not sure where to
begin, start with
[`choose-your-first-command.md`](packaging/workflows/choose-your-first-command.md)
or the tiny
[`one-command-trial.md`](packaging/workflows/one-command-trial.md) flow.

Migration patterns that usually keep Picard in place: per-read-group `SamToFastq`,
sequential-shard `FastqToSam`, and mate-repair boundaries around `FixMateInformation`.
Trial workflows include
[`trial.wdl`](packaging/workflows/trial.wdl),
[`trial.nf`](packaging/workflows/trial.nf),
[`trial-samtofastq.nf`](packaging/workflows/trial-samtofastq.nf),
[`trial-samtofastq.wdl`](packaging/workflows/trial-samtofastq.wdl),
[`trial-fastqtosam.nf`](packaging/workflows/trial-fastqtosam.nf),
[`trial-fastqtosam.wdl`](packaging/workflows/trial-fastqtosam.wdl),
[`trial-fixmateinformation.wdl`](packaging/workflows/trial-fixmateinformation.wdl), and
[`trial-fixmateinformation.nf`](packaging/workflows/trial-fixmateinformation.nf).

## Using it in a pipeline

`WDL` / Cromwell:

```wdl
command <<<
  turbo-picard MarkDuplicates \
    I=~{input_bam} \
    O=~{sample_id}.marked.bam \
    M=~{sample_id}.metrics.txt \
    ASSUME_SORTED=true
>>>
```

Nextflow:

```nextflow
def picard = params.use_turbo_picard ? 'turbo-picard' : 'picard'
"""
${picard} SortSam I=${bam} O=${meta.id}.sorted.bam SORT_ORDER=coordinate
"""
```

Snakemake:

```python
shell:
    "turbo-picard BuildBamIndex I={input.bam} O={output.bai}"
```

More workflow notes: [`packaging/nf-core/README.md`](packaging/nf-core/README.md),
[`packaging/workflows/wdl-cromwell.md`](packaging/workflows/wdl-cromwell.md),
[`packaging/workflows/nextflow-nf-core.md`](packaging/workflows/nextflow-nf-core.md),
[`packaging/workflows/snakemake.md`](packaging/workflows/snakemake.md).

## When It Helps

The best first use is one expensive Picard step that you can compare easily:
sorting, duplicate marking, FASTQ conversion, indexing, VCF housekeeping, or a
metrics command that keeps slowing down iteration. Run Picard and `turbo-picard`
beside each other on a representative file, compare the outputs that matter for
that command, then switch only that checked step.

## When To Stay With Picard

Use upstream Picard directly when you want every step to run on the JVM without
delegation, when workflows depend on exact Picard-rendered chart PDFs, or for
any accelerated step you have not compared on data that looks like your own.
Delegation keeps every Picard 3.4.0 command available; it is not a reason to skip
validation on the accelerated path.

## Documentation

The full docs are on Read the Docs:

**https://turbo-picard.readthedocs.io/en/latest/**

Good starting points:

- [Is this for you?](https://turbo-picard.readthedocs.io/en/latest/is-this-for-you.html)
- [Choose your first command](https://turbo-picard.readthedocs.io/en/latest/first-command.html)
- [Evaluation playbook](https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html)
- [Quickstart](https://turbo-picard.readthedocs.io/en/latest/quickstart.html)
- [Command coverage](https://turbo-picard.readthedocs.io/en/latest/commands.html)
- [Picard vs turbo-picard](https://turbo-picard.readthedocs.io/en/latest/picard-vs-turbo-picard.html)
- [turbo-picard vs riker](https://turbo-picard.readthedocs.io/en/latest/turbo-picard-vs-riker.html)
- [FAQ](https://turbo-picard.readthedocs.io/en/latest/faq.html)
- [What parity means](https://turbo-picard.readthedocs.io/en/latest/parity.html) —
  the parity guide for what comparisons prove and what they do not.
- [Trying it in a pipeline](https://turbo-picard.readthedocs.io/en/latest/adoption.html)
- [Benchmarks](https://turbo-picard.readthedocs.io/en/latest/benchmarks.html)
- [Citation](https://turbo-picard.readthedocs.io/en/latest/citation.html)

The docs source is in [`docs/`](docs/).

## Fallback to Picard

Unsupported commands fail by default. To let them run through upstream Picard:

```bash
export TURBO_PICARD_FALLBACK_COMMAND='java -jar /opt/picard/picard.jar'
```

Use an absolute path so the fallback cannot accidentally resolve back to the
`picard` shim. See the
[fallback documentation](https://turbo-picard.readthedocs.io/en/latest/fallback.html).

## Container image

```bash
docker build -t turbo-picard:local .
docker run --rm turbo-picard:local MarkDuplicates --help
```

## Check your own data

Before switching a pipeline step, run Picard and `turbo-picard` on a
representative file and keep the comparison with the analysis:

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

Record pinned input SHA-256 hashes, source URLs, and commits with your evidence.
See [Trying it in a pipeline](https://turbo-picard.readthedocs.io/en/latest/adoption.html)
for the full validation protocol.

## Benchmarks

Three-way QC comparisons against riker:

```bash
python3 tools/bench_qc_vs_riker.py --smoke --skip-build --allow-missing-riker
```

The smoke helper now defaults to median-of-5 repeats so tiny fixture startup
noise does not swamp the overlap timings.

Evidence lives in [`benchmarks/riker-comparison/`](benchmarks/riker-comparison/).

The benchmark suite compares each command with Picard and checks stable output
before reporting speed. Saved on `2026-06-04` from
`python3 tools/bench_suite.py --repeats 1 --skip-build`.
Raw log: `docs/site/assets/bench-suite-output.txt`.

Summary:

- `32/32` benchmarked commands passed parity checks.
- `84.46x` top speedup: `UpdateVcfSequenceDictionary`.
- `8.55x` floor speedup: `RevertSam`.
- `26.82x` median speedup.
- `26.74x` geometric mean speedup.

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

CRAM preprocessing parity checks:

```bash
./tools/verify_basic_cram_parity.sh
./tools/verify_markdup_cram_parity.sh
./tools/verify_gatk_preprocessing_combo_parity.sh
./tools/verify_gatk_mito_bam_parity.sh
./tools/verify_gatk_mito_cram_parity.sh
./tools/verify_gatk_preprocessing_combo_cram_parity.sh
```

Evidence verifiers:

```bash
python3 tools/verify_benchmark_log_evidence.py
python3 tools/verify_benchmark_suite_coverage.py
python3 tools/verify_benchmark_thresholds.py
python3 tools/verify_readme_benchmark_evidence.py
python3 tools/verify_site_benchmark_evidence.py
```

## Real data

Synthetic benchmarks are not enough. The repository keeps pinned public
comparisons with source URLs, commits, SHA-256 input hashes, versions, outputs,
and digest comparisons in [`benchmarks/real-data/`](benchmarks/real-data/).

Current checked datasets:

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

Use the Zenodo DOI for the archived release you actually used. GitHub and
Zenodo update that metadata after a release is cut.

A short JOSS-style software paper draft is in [`paper/`](paper/). Check it with:

```bash
python3 tools/verify_joss_paper.py
```

The submission checklist is in [`docs/joss-submission.md`](docs/joss-submission.md).

## Bioconda status

Release `v0.1.2` has been submitted to Bioconda as two recipes:

- `turbo-picard` — installs `turbo-picard`.
- `turbo-picard-picard-shim` — installs the optional `picard` command name.

Release checks:

```bash
python3 tools/bioconda_release_preflight.py
python3 tools/prepare_bioconda_release.py \
  --archive ~/Downloads/turbo-picard-0.1.2.tar.gz
python3 tools/verify_bioconda_recipes.py --release-ready
```

## Contributing

Bug reports, parity evidence, documentation fixes, and small command-coverage
improvements are welcome. Start with [`CONTRIBUTING.md`](CONTRIBUTING.md).

Before adding or widening a native command, check
[`docs/command-matrix.yml`](docs/command-matrix.yml). Changes should include
tests, a command-coverage update, and documentation that says plainly what is
supported.

Development workflow:
[development docs](https://turbo-picard.readthedocs.io/en/latest/development.html).
Support: [`SUPPORT.md`](SUPPORT.md). Security: [`SECURITY.md`](SECURITY.md).
