# One-command trial

Use this when you want the fastest honest answer about whether `turbo-picard`
belongs in your workflow.

Best first commands:

- `MarkDuplicates` for broad preprocessing pipelines
- `SortSam` for repeated BAM or CRAM reshaping
- `SamToFastq` for export-heavy alignment or remap paths
- `BuildBamIndex` for a very small, low-risk first substitution

Recommended flow:

1. Pick one representative shard, not your smallest toy file.
2. Run the upstream Picard step and the `turbo-picard` step on that same shard.
3. Compare the exact files the downstream workflow consumes.
4. Keep the outputs, metrics, timings, and command lines together.
5. Only widen the rollout after that command is boring on your own data.

Files in this directory:

- `trial.wdl`: tiny `WDL` workflow for a single `MarkDuplicates` trial
- `trial.nf`: tiny `Nextflow` workflow for the same trial shape
- `trial-config.yaml`: small config stub showing the expected inputs

If you need a reviewable evidence bundle instead of a quick local trial, use
`tools/audit_real_data.py` or `tools/compare_real_data.py`.
