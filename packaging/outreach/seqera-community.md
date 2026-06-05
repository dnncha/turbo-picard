# Seqera Community / Nextflow forum post

We have been testing `turbo-picard` as a command-by-command upgrade path for
Picard-heavy `Nextflow` workflows.

The useful adoption path is small:

1. choose one expensive Picard step
2. run upstream Picard and `turbo-picard` on the same representative shard
3. compare the exact downstream-consumed outputs
4. only widen the rollout if that command is boring on real data

The point is not to claim a full Picard replacement. The point is to make a
single workflow boundary faster without changing the surrounding process model
until the evidence is good enough to justify it.
