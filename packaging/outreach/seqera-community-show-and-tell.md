# Seqera Community - Show & Tell

Title:

Turbo Picard 0.1.22: testing native Rust replacements for slow Picard steps in Nextflow (30x GRCh38 parity log)

Post:

Hi everyone - I am the author of `turbo-picard`, an open-source Rust toolkit
implementing 35 selected Picard commands natively while keeping Picard
`KEY=VALUE` and GATK4 `--kebab-case` flags (plus automatic fallback to upstream
Picard for the remaining 86 commands):

https://github.com/dnncha/turbo-picard

The goal is practical: let pipeline authors test one slow or memory-heavy
Picard process (`MarkDuplicates`, `CollectWgsMetrics`, `CollectMultipleMetrics`,
`SortSam`, `SamToFastq`) side by side without rewriting the surrounding
Nextflow, WDL, or Snakemake workflow.

Measured on a real 30x GRCh38 whole genome (1000 Genomes `HG02675`, 740,367,510
records, 8 vCPU / 50 GB RAM, single run, 23 November 2026):

- `MarkDuplicates`: on Host A (Intel Xeon), pre-#87 build `9a15db96` ran in
  **59m 01s / 355 MB peak RSS** vs Picard 3.4.0's **3h 30m 07s / 34.4 GB RSS**
  (**3.6x faster, 97x less memory**; optical count differed by 0.18% pre-#87).
  After fixing Java `short` tile-coordinate wrap and FR/RF optical splitting,
  the #87 build (`93216217`, measured on Host B at **43m 07s / 354 MB RSS**,
  temp disk not sampled) matched Picard metrics, duplicate count (79,289,617),
  and SAM columns 1-11 MD5 exactly; #88 matched `DT`, `DS`, `DI`, and `PG` tags
  across chr21's 11.4M records (whole genome not rerun on #88). Trade-off on
  Host A: peak temporary disk was 99.2 GB vs Picard's 23.3 GB (4.3x more,
  tracked in #86).
- `CollectWgsMetrics`: **5m 08s / 352 MB peak RSS** vs Picard 3.4.0's
  **2h 13m 55s / 4.0 GB RSS** (**26x faster, 11x less memory**), matching every
  metric field and histogram row.

(The historical 14 August 2026 32-command small-fixture suite records 84.52x
geometric mean and 272.12x maximum saved ratio vs Picard 3.4.0; JVM startup
contributes heavily there, so use the 30x WGS log for large-file sizing.)

Try one step safely using the versioned container or PyPI package:

```bash
docker run --rm ghcr.io/dnncha/turbo-picard:0.1.22 \
  trial MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

```bash
python3 -m pip install turbo-picard==0.1.22
```

Starter templates for Nextflow, WDL, Snakemake, and nf-core live in
`packaging/workflows/` and `packaging/nf-core/`. Full evidence and scope:

- https://turbo-picard.readthedocs.io/en/latest/evidence/
- https://turbo-picard.readthedocs.io/en/latest/commands/
- https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html

I would love to hear which Picard step costs the most time or memory in your
Nextflow runs, and whether the temp-disk vs RAM trade-off fits your executor.
