# Biostars

Suggested tags:

`tools`, `picard`, `bam`, `sam`, `workflow`, `rust`

Title:

`turbo-picard` 0.1.21: native Rust implementations of 35 Picard commands (30x WGS parity tested)

Post:

I am the author of `turbo-picard`, a Rust toolkit that implements 35 selected
Picard commands natively while keeping Picard `KEY=VALUE` and GATK4
`--kebab-case` CLI arguments:

https://github.com/dnncha/turbo-picard

It is built for pipelines that already call Picard in WDL, Nextflow, Snakemake,
or shell scripts and want to speed up one bottleneck step without rewriting the
surrounding workflow.

Example:

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

Recent 30x whole-genome measurements on 1000 Genomes `HG02675` (GRCh38,
740,367,510 records, 8 vCPU / 50 GB RAM, single run, 23 November 2026):

- `MarkDuplicates`: on Host A (same Intel Xeon host), pre-#87 build `9a15db96`
  ran in 59m 01s and 355 MB peak RSS vs Picard 3.4.0 at 3h 30m 07s and 34.4 GB
  RSS (3.6x faster, 97x less RAM; trade-off: 99.2 GB vs 23.3 GB peak temporary
  disk, tracked in #86; optical count differed by 0.18% before #87). On Host B,
  the #87 build (`93216217`, 43m 07s / 354 MB RSS, temp disk not sampled)
  matched Picard metrics, duplicate count (79,289,617), and SAM columns 1-11
  MD5 exactly; #88 matched `DT`/`DS`/`DI`/`PG` tags on chr21's 11.4M records
  (whole genome not rerun on #88).
- `CollectWgsMetrics`: 5m 08s and 352 MB peak RSS vs Picard 3.4.0 at 2h 13m 55s
  and 4.0 GB RSS (26x faster, 11x less RAM), matching every metric field and
  histogram row.

The historical 14 August 2026 small-fixture suite also covers 32 parity-checked
commands against Picard 3.4.0 (84.52x geometric mean, 272.12x maximum saved
ratio, where JVM startup contributes heavily and does not predict whole-genome
ratios).

Suggested way to test it:

1. pick one slow or memory-heavy Picard step from your workflow;
2. run Picard and `turbo-picard` side by side on the same input (or inspect the
   trial contract with `turbo-picard trial <Command> ...`);
3. compare the BAM/CRAM, metrics, and sidecar files your pipeline reads;
4. switch only that command if the outputs and resource profile fit.

Install:

```bash
python3 -m pip install turbo-picard==0.1.21
# or container:
docker run --rm ghcr.io/dnncha/turbo-picard:0.1.21 --version
```

Unsupported commands (86 of 127) and unsupported options fall back cleanly to
upstream Picard when configured. Full evidence, limits, and command scope:

- Evidence & 30x WGS log: https://turbo-picard.readthedocs.io/en/latest/evidence/
- Command coverage: https://turbo-picard.readthedocs.io/en/latest/commands/
- Evaluation playbook: https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html
