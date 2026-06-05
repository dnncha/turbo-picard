# Snakemake starter

Use this when your rules already shell out to Picard-like commands and you want
the smallest possible substitution.

Start here:

- `Snakefile` if `BuildBamIndex` is a low-risk first replacement

Recommended next command after that:

- `SortSam` if the workflow repeatedly rewrites order
- `MarkDuplicates` if duplicate marking is the larger runtime sink

Practical rollout:

1. Swap only one rule shell command to `turbo-picard`.
2. Run the old and new rules on the same representative input.
3. Compare the exact files downstream rules consume.
4. Add a config flag or alternate rule path if you need a gradual rollout.

If the workflow uses more expensive Picard steps, use the starter files in this
directory as a command-shape reference and keep the same side-by-side evidence
discipline before switching wider parts of the DAG.
