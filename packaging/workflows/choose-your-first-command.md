# Choose your first command

Use this when you know you want to try `turbo-picard`, but you do not yet know
which Picard step is the right first substitution.

Start with `MarkDuplicates` if:

- duplicate marking is one of the longest steps in your preprocessing run
- the input is already coordinate-sorted
- you want an easy side-by-side comparison with BAM plus metrics output

Start with `SortSam` if:

- the workflow repeatedly changes BAM or CRAM order between stages
- you want a straightforward file-to-file comparison
- sorting time is noticeable across many shards

Start with `SamToFastq` if:

- Picard export still sits on a realignment, remap, or handoff path
- FASTQ generation is adding wall time in a loop you run often
- you want to compare plain downstream FASTQ outputs directly

Start with `BuildBamIndex` if:

- you want the smallest possible first substitution
- the workflow has many light Picard glue steps
- you want a low-risk test before moving to heavier commands

What to avoid as a first trial:

- commands outside the documented native scope
- workflows that depend on exact Picard-rendered chart PDFs
- tiny toy files that do not look anything like the data you actually process

Recommended next step after choosing:

1. Pick the matching starter file from this directory.
2. Run upstream Picard and `turbo-picard` on the same representative shard.
3. Compare the exact files the downstream workflow consumes.
4. Keep the command lines, timings, and outputs together so the trial is reviewable.
