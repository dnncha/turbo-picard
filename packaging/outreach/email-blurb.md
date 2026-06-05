# Email blurb

I have been testing `turbo-picard`, which is a faster Rust implementation of
selected Picard commands. It keeps Picard-style command names and `KEY=VALUE`
arguments, so the adoption path is mostly about swapping one command at a time
instead of redesigning a workflow.

The current checked-in public evidence includes a saved benchmark suite with
`32/32` parity-checked commands, and the repository now has starter examples
for `WDL`, `Nextflow`, and `Snakemake` plus a small one-command trial bundle.

If we want a low-risk evaluation, the sensible first step is to pick one
expensive Picard command from our own workflow, run upstream Picard and
`turbo-picard` on the same representative shard, and compare the exact outputs
we depend on before discussing any wider switch.

Relevant repo entry points:

- `packaging/workflows/choose-your-first-command.md`
- `packaging/workflows/one-command-trial.md`
- `docs/adoption.rst`
