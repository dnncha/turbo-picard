# Workflow starter examples

This directory holds small starter files for wiring `turbo-picard` into the
workflow systems most likely to use it first.

The examples are intentionally narrow:

- they keep the existing workflow boundary;
- they swap only the Picard command inside that boundary;
- they are meant for side-by-side evaluation before a broader rollout.

Files:

- `markduplicates.wdl`: minimal `WDL` / `Cromwell` task using `turbo-picard MarkDuplicates`
- `markduplicates.nf`: minimal `Nextflow` process with a runtime toggle between `picard` and `turbo-picard`
- `fixmateinformation.wdl`: minimal `WDL` / `Cromwell` task using `turbo-picard FixMateInformation`
- `fastqtosam.wdl`: minimal `WDL` / `Cromwell` task using `turbo-picard FastqToSam`, including sequential shard mode
- `samtofastq.wdl`: minimal `WDL` / `Cromwell` task using `turbo-picard SamToFastq`, with optional per-read-group export
- `sortsam.wdl`: minimal `WDL` / `Cromwell` task using `turbo-picard SortSam`
- `fastqtosam.nf`: minimal `Nextflow` process for `FastqToSam`, including sequential shard mode
- `fixmateinformation.nf`: minimal `Nextflow` process for `FixMateInformation`
- `sortsam.nf`: minimal `Nextflow` process for `SortSam`
- `samtofastq.nf`: minimal `Nextflow` process for `SamToFastq`, including per-read-group export
- `Snakefile`: minimal `Snakemake` rule set using `turbo-picard BuildBamIndex`, `SortSam`, `MarkDuplicates`, `SamToFastq`, `FastqToSam`, and `FixMateInformation`, including a dedicated per-read-group `SamToFastq` rule and sequential-ingest toggles
- `wdl-cromwell.md`: short walkthrough for choosing and testing the `WDL` starters
- `nextflow-nf-core.md`: short walkthrough for the `Nextflow` / nf-core starters
- `snakemake.md`: short walkthrough for the `Snakemake` starter
- `trial.wdl`: tiny single-command `WDL` workflow for a `MarkDuplicates` evaluation
- `trial.nf`: tiny single-command `Nextflow` workflow for the same trial shape
- `trial-samtofastq.nf`: tiny `Nextflow` workflow for a `SamToFastq` evaluation
- `trial-samtofastq.wdl`: tiny `WDL` workflow for a `SamToFastq` evaluation
- `trial-fastqtosam.nf`: tiny `Nextflow` workflow for a paired `FastqToSam` evaluation
- `trial-fastqtosam.wdl`: tiny `WDL` workflow for a paired `FastqToSam` evaluation
- `trial-fixmateinformation.wdl`: tiny `WDL` workflow for a `FixMateInformation` evaluation
- `trial-fixmateinformation.nf`: tiny `Nextflow` workflow for a `FixMateInformation` evaluation
- `trial-config.yaml`: small config stub showing the expected trial inputs
  and the main trial toggles for per-read-group FASTQ export or sequential FASTQ ingest
- `one-command-trial.md`: short guide for running the smallest honest evaluation
- `choose-your-first-command.md`: quick guide for choosing the right first Picard step to replace

Pick a starting point:

- Use `markduplicates.wdl` if your pipeline already wraps Picard in `WDL` tasks and duplicate marking is an obvious time sink.
- Use `markduplicates.nf` if you want a `Nextflow` or `nf-core` style runtime toggle between upstream `picard` and `turbo-picard`.
- Use `samtofastq.wdl`, `fastqtosam.wdl`, `samtofastq.nf`, `fastqtosam.nf`, or `fixmateinformation.nf` when FASTQ export, FASTQ ingest, or mate repair is the more realistic first boundary in that workflow system.
- Use `Snakefile` if your `Snakemake` rules already shell out to Picard-like commands and you want starter rules for indexing, sorting, duplicate marking, FASTQ export or ingest, or mate repair.

Good first commands by workflow shape:

- `MarkDuplicates`: best first trial for broad preprocessing pipelines where duplicate marking is expensive and easy to compare.
- `SortSam`: good first trial when a workflow repeatedly reshapes BAM or CRAM order.
- `SamToFastq`: good first trial when Picard export still sits on a realignment or remap path, especially with per-read-group FASTQ output.
- `FastqToSam`: good first trial when lane-sharded FASTQ ingestion still uses Picard before alignment or archival handoff.
- `FixMateInformation`: good first trial when mate repair remains in preprocessing and the workflow already reaches a queryname-sorted boundary.
- `BuildBamIndex`: good first trial for lighter workflow glue where you want a very small, low-risk substitution.

What these examples are for:

- evaluating one command in place before changing wider workflow behavior;
- preserving the familiar Picard-style `KEY=VALUE` surface;
- giving workflow owners something small enough to test on representative data immediately.

What they are not for:

- claiming a whole pipeline is ready without side-by-side evidence;
- replacing unsupported Picard commands;
- proving that one public fixture covers every cohort or assay.

Recommended rollout:

1. Start with the explicit `turbo-picard` command instead of the `picard` shim.
2. Run `tools/audit_real_data.py` or `tools/compare_real_data.py` on a representative shard.
3. Keep upstream Picard available through `TURBO_PICARD_FALLBACK_COMMAND` while command coverage is mixed.
4. Move the shim or workflow-wide switch only after the checked command is boring on your own data.
