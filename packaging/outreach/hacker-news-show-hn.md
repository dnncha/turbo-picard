# Hacker News - Show HN

Use this only on a day when you can answer comments. Submit the GitHub repo as
the URL.

Title:

Show HN: Turbo Picard – Rust implementations of 35 Broad Picard genomics commands

Initial comment:

Hi HN,

I built `turbo-picard`, an MIT-licensed Rust implementation of 35 selected Broad
Picard genomics commands (plus transparent fallback to Java Picard for the
remaining 86):

https://github.com/dnncha/turbo-picard

Genomics pipelines call Picard constantly for duplicate marking, BAM sorting,
and alignment QC, often burning tens of gigabytes of Java heap and hours of
wall time. `turbo-picard` keeps Picard's `KEY=VALUE` and GATK4 `--kebab-case`
flags and checks output parity against Picard 3.4.0/3.5.0:

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

On a real 30x GRCh38 whole genome (1000 Genomes `HG02675`, 740M reads, 8 vCPU /
50 GB RAM, single run, 23 November 2026):

- `MarkDuplicates`: 59m 01s at 355 MB peak RSS vs Picard 3.4.0's 3h 30m 07s at
  34.4 GB RSS (3.6x faster, 97x less memory). After matching Picard's 16-bit
  signed tile coordinate wrap and FR/RF optical split, metrics, duplicate count
  (79,289,617), and SAM columns 1-11 MD5 matched Picard exactly. Honest
  trade-off: our spill path currently uses 4.3x more temp disk (99.2 GB vs
  23.3 GB, tracked in #86).
- `CollectWgsMetrics`: 5m 08s at 352 MB RSS vs Picard's 2h 13m 55s at 4.0 GB
  RSS (26x faster, 11x less memory), matching every metric field and histogram
  row.

On small inputs, the historical 14 August 2026 32-command fixture suite reports
84.52x geometric mean and 272.12x maximum saved ratio vs Picard 3.4.0 (JVM
startup dominates tiny inputs, so those ratios are not whole-genome
predictions).

Version 0.1.21 also ships an MCP stdio server (`turbo-picard mcp`) with an
isolated `verify` tool that runs native `turbo-picard` and upstream Picard side
by side in a temporary directory and checks output parity structurally.

Install:

```bash
python3 -m pip install turbo-picard==0.1.21
```

Happy to answer questions on HTSlib/Rust integration, Picard's surprising
edge cases at genome scale (like Java `short` tile coordinate overflow), or
the external-spill temp-disk trade-off.
