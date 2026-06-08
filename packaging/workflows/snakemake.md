# Snakemake starter

Use this when your rules already shell out to Picard-like commands and you want
the smallest possible substitution.

Start here:

- `Snakefile` if `BuildBamIndex` is a low-risk first replacement or you want
  copyable starter rules for `SortSam`, `MarkDuplicates`, `SamToFastq`,
  `FastqToSam`, or `FixMateInformation`

Recommended next command after that:

- `SortSam` if the workflow repeatedly rewrites order
- `MarkDuplicates` if duplicate marking is the larger runtime sink
- `SamToFastq` if export still sits on a remap or handoff path, especially when outputs are split by read group
- `FastqToSam` if the workflow ingests paired FASTQs before alignment, including sequential shard sets
- `FixMateInformation` if mate repair remains in preprocessing

Practical rollout:

1. Swap only one rule shell command to `turbo-picard`.
2. Preserve any Picard-shaped options the rule actually depends on, such as `OUTPUT_PER_RG=true` in a dedicated per-read-group rule or `USE_SEQUENTIAL_FASTQS=true`.
3. Run the old and new rules on the same representative input.
4. Compare the exact files downstream rules consume.
5. Add a config flag or alternate rule path if you need a gradual rollout.

If the workflow uses more expensive Picard steps, use the starter files in this
directory as a command-shape reference and keep the same side-by-side evidence
discipline before switching wider parts of the DAG.
