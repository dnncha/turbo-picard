# Evaluation checklist

Use this when you need a quick internal answer to: should we trial
`turbo-picard`, and if so, on what first?

Try it now if most of these are true:

- one Picard step shows up repeatedly in wall-time complaints
- the workflow already shells out to Picard in a stable place
- you can get one representative BAM or CRAM shard without operational drama
- the downstream files you care about are easy to compare
- you are willing to switch one command first instead of arguing about a full replacement

Pick the first command like this:

- `MarkDuplicates` if duplicate marking is a visible cost and the input is already coordinate-sorted
- `SortSam` if reorder work keeps happening between stages
- `SamToFastq` if FASTQ export is still on a realignment or remap path
- `BuildBamIndex` if you want a very small, low-risk first substitution

Do not start with:

- commands outside the documented native scope
- workflows that depend on exact Picard chart PDF rendering
- tiny toy fixtures that do not look like your real data

Minimum trial standard:

1. run upstream Picard and `turbo-picard` on the same representative shard
2. compare the exact files your downstream workflow consumes
3. keep the command lines, timings, metrics, and output paths together
4. decide on that one command before widening the conversation

Useful repo entry points:

- `packaging/workflows/choose-your-first-command.md`
- `packaging/workflows/one-command-trial.md`
- `docs/adoption.rst`
