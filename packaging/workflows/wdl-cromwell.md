# WDL / Cromwell starter

Use this when your workflow already wraps Picard in `WDL` tasks and you want a
small first substitution instead of a broader rewrite.

Start here:

- `markduplicates.wdl` if duplicate marking is the obvious wall-time problem
- `sortsam.wdl` if your pipeline repeatedly reorders BAM or CRAM between stages

Recommended first command:

- `MarkDuplicates` for a representative coordinate-sorted BAM or CRAM shard
- `SortSam` if ordering work is the main repeated cost in the workflow

Practical rollout:

1. Replace only the command inside one task with the `turbo-picard` version.
2. Run that task beside the existing Picard task on representative data.
3. Compare the primary output, metrics, sidecars, and runtime before changing any shared task library.
4. Keep upstream Picard available for commands outside the native scope.

If you need a wider comparison bundle, run `tools/audit_real_data.py` or
`tools/compare_real_data.py` before moving the change into a shared workflow.
