# Nextflow / nf-core starter

Use this when your workflow already shells out to Picard inside a `Nextflow`
process or nf-core-style module and you want a runtime switch between upstream
Picard and `turbo-picard`.

Start here:

- `markduplicates.nf` if duplicate marking is the first painful step to test
- `sortsam.nf` if reorder work is a better first candidate
- `samtofastq.nf` if Picard export still sits on a remap or alignment path

Recommended first command:

- `MarkDuplicates` for preprocessing-heavy pipelines
- `SortSam` for workflows that repeatedly reshape BAM or CRAM order
- `SamToFastq` for realignment or export-heavy paths

Practical rollout:

1. Add a parameter such as `params.use_turbo_picard`.
2. Switch only the process-local executable between `picard` and `turbo-picard`.
3. Run the same representative shard through both paths.
4. Keep fallback or the original module available until the command is boring on your own data.

The existing nf-core note in `../nf-core/README.md` is the better reference
once you are moving from a local trial to a module-style integration.
