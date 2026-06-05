# Community post

We have been evaluating `turbo-picard`, a Rust implementation of selected
Picard commands that keeps familiar Picard-style command names and `KEY=VALUE`
arguments.

The interesting part is not a claim to replace all of Picard at once. The
useful adoption path is smaller: pick one expensive Picard step, run upstream
Picard and `turbo-picard` on the same representative shard, compare the exact
outputs the workflow consumes, and only widen the rollout after that command is
boring on real data.

The repository now includes:

- starter examples for `WDL`, `Nextflow`, and `Snakemake`
- a guide for choosing the right first Picard command to test
- a small one-command trial bundle
- benchmark and parity evidence tied to the checked-in claims

Good first commands to evaluate are usually `MarkDuplicates`, `SortSam`,
`SamToFastq`, or `BuildBamIndex`, depending on where the workflow actually
hurts.
