# Slack / chat message

We have been evaluating `turbo-picard`, a Rust implementation of selected
Picard commands that keeps the usual Picard-style `KEY=VALUE` command shape.

Why it looks interesting:

- it targets the Picard steps that tend to cost real wall time
- the saved public benchmark suite currently shows `32/32` parity-checked commands
- it supports side-by-side evaluation before changing a wider workflow

Best way to try it:

1. pick one expensive Picard step such as `MarkDuplicates`, `SortSam`, `SamToFastq`, or `BuildBamIndex`
2. run upstream Picard and `turbo-picard` on the same representative shard
3. compare the exact files our downstream workflow consumes

Starter material is in `packaging/workflows/`, including command-specific
examples and a `choose-your-first-command.md` guide.
