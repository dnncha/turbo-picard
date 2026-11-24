# Direct maintainer note — draft, not sent

Subject: Comparing one slow Picard step in your workflow

Hi,

I maintain Turbo Picard (0.1.21), which runs 35 selected Picard commands
natively in Rust while keeping Picard `KEY=VALUE` and GATK4 `--kebab-case`
arguments intact (and falling back to upstream Picard for the remaining 86).

I'm looking for a pipeline owner with a Picard step (`MarkDuplicates`,
`CollectWgsMetrics`, `CollectMultipleMetrics`, or `SortSam`) that costs
noticeable time or memory. Would a side-by-side comparison on a representative
shard be useful? The goal is to check the outputs your pipeline actually
depends on, not to ask you to replace the entire workflow.

The code, 30x GRCh38 benchmark log, and compatibility scope are here:
https://github.com/dnncha/turbo-picard
https://turbo-picard.readthedocs.io/en/latest/evidence/

The comparison stays on your machine. A mismatch or an installation problem is
as useful to me as a speedup; private data and production changes are not needed.

Thanks,
Donncha

## Editor notes — not part of the message

Personalise the opening around a specific, observed workflow bottleneck before
sending. Do not bulk-post this note, promise support time that has not been
allocated, or imply that an integration has already been accepted.

The saved 32-command suite (14 August 2026) reports 84.52x geometric-mean and
272.12x top speedup on small fixtures; those are not predictions for the
recipient's workload, so the message does not lead with them. For large files,
point to the 23 November 2026 30x GRCh38 log (`MarkDuplicates` Host A pre-#87
59m 01s / 355 MB vs 3h 30m 07s / 34.4 GB with 4.3x more temp disk, #87 exact
metrics & cols 1-11 MD5 on Host B at 43m 07s / 354 MB, #88 `PG` tags checked on
chr21; `CollectWgsMetrics` 5m 08s / 352 MB vs 2h 13m 55s / 4.0 GB) and be
explicit about the temp-disk trade-off and per-build parity scope.
