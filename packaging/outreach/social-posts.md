# Social posts

## LinkedIn

I just released `turbo-picard` 0.1.21 — native Rust implementations of 35 Broad
Picard genomics commands with Picard `KEY=VALUE` and GATK4 `--kebab-case` flags,
plus transparent fallback to upstream Picard for the remaining 86 commands.

On a real 30x GRCh38 whole genome (1000 Genomes HG02675, 740M records, 8 vCPU):
- `MarkDuplicates`: 59m 01s at 355 MB peak RSS vs Picard 3.4.0's 3h 30m 07s at
  34.4 GB RSS (3.6x faster, 97x less memory, exact metrics & SAM columns 1-11
  MD5; current trade-off is 99.2 GB vs 23.3 GB temp disk, tracked in #86).
- `CollectWgsMetrics`: 5m 08s at 352 MB RSS vs Picard's 2h 13m 55s at 4.0 GB
  RSS (26x faster, 11x less memory, exact field-for-field and histogram parity).

(For small jobs, the historical 14 August 2026 32-command fixture suite reports
84.52x geometric mean and 272.12x top saved ratio where JVM startup dominates.)

Version 0.1.21 also adds bare GATK4 boolean flags and an isolated `verify` tool
in `turbo-picard mcp` so coding agents can run native vs Picard parity checks
in a temporary directory before touching workflow files.

Try one slow Picard step side by side on your own BAM/CRAM:

```bash
python3 -m pip install turbo-picard==0.1.21
turbo-picard trial MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

Repo & evidence:
https://github.com/dnncha/turbo-picard
https://turbo-picard.readthedocs.io/en/latest/evidence/

## Bluesky / Mastodon

Released turbo-picard 0.1.21: 35 Broad Picard genomics commands in native Rust,
with Picard KEY=VALUE + GATK4 --kebab-case flags and transparent Picard fallback
for the other 86.

30x GRCh38 WGS (740M reads, 8 vCPU) vs Picard 3.4.0:
- MarkDuplicates: 59m / 355 MB vs 3h 30m / 34.4 GB (3.6x faster, 97x less RAM,
  exact metrics & MD5; 4.3x more temp disk tracked in #86)
- CollectWgsMetrics: 5m 08s / 352 MB vs 2h 14m / 4.0 GB (26x faster, 11x less
  RAM, exact parity)
(Saved 14 Aug 2026 small-fixture suite: 84.52x geomean, 272.12x top, startup-heavy.)

pip install turbo-picard==0.1.21
https://github.com/dnncha/turbo-picard
https://turbo-picard.readthedocs.io/en/latest/evidence/
