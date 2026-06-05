# Common objections

Use this when a workflow owner or teammate is interested but keeps circling the
same adoption concerns.

`Is this trying to replace all of Picard?`

No. The intended path is one command at a time, with side-by-side checks on
representative data before any wider switch.

`How do we know where to start?`

Start with the command that hurts most and is easy to compare. In practice that
usually means `MarkDuplicates`, `SortSam`, `SamToFastq`, or `BuildBamIndex`.

`What if our workflow still needs upstream Picard?`

That is normal. The project already treats fallback as part of the practical
adoption path while coverage is mixed.

`What if the benchmark numbers do not match our workflow?`

That is exactly why the repo keeps command-level trials and real-data
comparison tooling. The public benchmark suite is useful context, not a reason
to skip checking your own representative shard.

`What if we depend on exact Picard-rendered plots?`

Stay with upstream Picard for that step. The project does not claim that every
Picard chart PDF is reproduced byte-for-byte.

`What is the smallest honest evaluation?`

Pick one representative shard, run upstream Picard and `turbo-picard` on the
same command, compare the exact downstream-consumed outputs, and keep the
command lines and timings together.
