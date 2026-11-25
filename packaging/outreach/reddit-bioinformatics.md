# r/bioinformatics

Check the subreddit rules and pinned thread before posting. Keep the tone
matter-of-fact and answer parity and packaging questions directly.

Title:

Turbo Picard 0.1.22: native Rust implementations of 35 Picard commands (tested on a 30x GRCh38 WGS against Picard 3.4.0)

Post:

I am the maintainer of `turbo-picard`, an open-source Rust toolkit implementing
35 Picard commands natively with Picard `KEY=VALUE` and GATK4 `--kebab-case`
flags (and transparent fallback to upstream Picard for the remaining 86):

https://github.com/dnncha/turbo-picard

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

Recent 30x GRCh38 whole-genome benchmark (1000 Genomes `HG02675`, 740,367,510
records, 8 vCPU / 50 GB RAM, single run, 23 November 2026):

- `MarkDuplicates`: on Host A (same Intel Xeon host), pre-#87 build `9a15db96`
  ran in 59m 01s at 355 MB peak RSS vs Picard 3.4.0's 3h 30m 07s at 34.4 GB RSS
  (3.6x faster, 97x less memory; optical count differed by 0.18% before #87).
  After fixing two genome-scale edge cases (Java `short` tile coordinate wrap
  and FR/RF optical split), build `93216217` (#87, measured on Host B at
  43m 07s / 354 MB RSS, temp disk not sampled) matched Picard 3.4.0 exactly on
  metrics, duplicate count (79,289,617), and SAM columns 1-11 MD5; #88 matched
  `DT`/`DS`/`DI`/`PG` tags across chr21's 11.4M records (whole genome not rerun
  on #88). Trade-off: peak temporary disk on Host A was 99.2 GB vs Picard's
  23.3 GB (tracked in issue #86).
- `CollectWgsMetrics`: 5m 08s at 352 MB RSS vs Picard's 2h 13m 55s at 4.0 GB
  RSS (26x faster, 11x less memory), matching every metric field and histogram
  row.

For small tasks, the historical 14 August 2026 32-command fixture suite reports
84.52x geometric mean and 272.12x maximum saved ratio against Picard 3.4.0 (JVM
startup dominates tiny inputs, so do not extrapolate those ratios to WGS runs).

Recommended way to evaluate it safely:

1. Pick one slow or memory-bound Picard step in a local Nextflow, WDL,
   Snakemake, or shell script.
2. Run `turbo-picard trial <Command> ...` (or the `verify` tool in
   `turbo-picard mcp`) to compare native and upstream Picard outputs side by
   side on the same read-only input.
3. Switch only that step if flags, tags, metrics, and resource costs fit.

Install:

```bash
python3 -m pip install turbo-picard==0.1.22
# or container:
docker run --rm ghcr.io/dnncha/turbo-picard:0.1.22 --version
```

Evidence, limits, and full command matrix:
- https://turbo-picard.readthedocs.io/en/latest/evidence/
- https://turbo-picard.readthedocs.io/en/latest/commands/
- https://turbo-picard.readthedocs.io/en/latest/compare/

Feedback on missing Picard options, temp-disk budgets, or workflow modules is
very welcome.
