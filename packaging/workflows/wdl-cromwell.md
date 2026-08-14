# WDL / Cromwell starter

Use this when your workflow already wraps Picard in `WDL` tasks and you want a
small first substitution instead of a broader rewrite.

From a repository checkout, the starter smoke uses `miniwdl` to strictly
validate every checked-in WDL document:

```bash
python3 -m pip install "miniwdl==1.15.0"
bash tools/verify_wdl_starters.sh
```

To execute the checked-in `MarkDuplicates` trial as well, set
`TURBO_PICARD_WDL_IMAGE` to a Docker image containing `turbo-picard`. The CI
smoke builds a no-entrypoint derivative of the repository reference image so
the WDL command is executed inside the image rather than being doubled by its
interactive container entrypoint.

Start here:

- `markduplicates.wdl` if duplicate marking is the obvious wall-time problem
- `sortsam.wdl` if your pipeline repeatedly reorders BAM or CRAM between stages
- `samtofastq.wdl` if FASTQ export still sits in a realignment, remap, or handoff path, especially when the task splits output by read group
- `fastqtosam.wdl` if lane-sharded FASTQ pairs still flow through Picard before alignment or archival handoff
- `fixmateinformation.wdl` if mate repair remains in preprocessing and the workflow already hands off queryname-sorted inputs

Recommended first command:

- `MarkDuplicates` for a representative coordinate-sorted BAM or CRAM shard
- `SortSam` if ordering work is the main repeated cost in the workflow
- `SamToFastq` if FASTQ export is still one of the easiest command boundaries to compare in your WDL task library
- `FastqToSam` if the current task already receives `_001`, `_002`, or similar lane-sharded FASTQ inputs
- `FixMateInformation` if the workflow already expects coordinate BAM output but still shells out to Picard for mate-field repair

Practical rollout:

1. Replace only the command inside one task with the `turbo-picard` version.
2. Carry through the exact Picard-shaped options that kept the old task alive, such as `OUTPUT_PER_RG`, `RG_TAG`, or `USE_SEQUENTIAL_FASTQS`. The checked-in `samtofastq.wdl` starter defaults to single-FASTQ output and turns on per-read-group export only when you set `output_per_rg=true`.
3. Run that task beside the existing Picard task on representative data.
4. Compare the primary output, metrics, sidecars, and runtime before changing any shared task library.
5. Keep upstream Picard available for commands outside the native scope.

If you need a wider comparison bundle, run `tools/audit_real_data.py` or
`tools/compare_real_data.py` before moving the change into a shared workflow.
