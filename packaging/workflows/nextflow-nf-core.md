# Nextflow / nf-core starter

Use this when your workflow already shells out to Picard inside a `Nextflow`
process or nf-core-style module and you want a runtime switch between upstream
Picard and `turbo-picard`.

Start here:

- `markduplicates.nf` if duplicate marking is the first painful step to test
- `sortsam.nf` if reorder work is a better first candidate
- `samtofastq.nf` if Picard export still sits on a remap or alignment path, especially when the module writes per-read-group FASTQs
- `fastqtosam.nf` if lane-sharded FASTQ ingest is the first workflow boundary you want to switch
- `fixmateinformation.nf` if mate repair remains in preprocessing and you want the same runtime toggle pattern

Recommended first command:

- `MarkDuplicates` for preprocessing-heavy pipelines
- `SortSam` for workflows that repeatedly reshape BAM or CRAM order
- `SamToFastq` for realignment or export-heavy paths, especially with per-read-group FASTQ output
- `FastqToSam` for sequential FASTQ shard ingest before alignment
- `FixMateInformation` for mate repair paths that already hand off queryname-sorted inputs

Practical rollout:

1. Add a parameter such as `params.use_turbo_picard`.
2. Keep the same process boundary and switch only the process-local executable between `picard` and `turbo-picard`.
3. If your current module depends on Picard-specific behavior, carry those options through directly: `OUTPUT_PER_RG=true` and `RG_TAG=PU|ID` for `SamToFastq`, or `USE_SEQUENTIAL_FASTQS=true` for lane-sharded `FastqToSam`.
4. Run the same representative shard through both paths.
5. Keep fallback or the original module available until the command is boring on your own data.

The existing nf-core note in `../nf-core/README.md` is the better reference
once you are moving from a local trial to a module-style integration.
