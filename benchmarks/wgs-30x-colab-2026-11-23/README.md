# 30x whole genome against Picard: 23 November 2026

This record covers one sample, measured on two cloud hosts of the same shape.
It is not a cohort validation.

## Inputs

- Sample: 1000 Genomes HG02675 (`ERR3242389`), NovaSeq, from
  `https://1000genomes.s3.amazonaws.com/1000G_2504_high_coverage/data/ERR3242389/HG02675.final.cram`.
- Converted to a coordinate-sorted BAM with `samtools view -b -T` (samtools 1.25).
  The BAM holds 740,367,510 records and its index puts depth at about 36x.
- Reference: `GRCh38_full_analysis_set_plus_decoy_hla.fa` from the same bucket.
- Picard 3.4.0 (Bioconda), Java 21, `-Xmx32g` for MarkDuplicates and `-Xmx8g` for CollectWgsMetrics.
- Hosts: Google Colab high-RAM CPU VMs, 8 vCPU, 50 GB RAM.
  - Host A: Intel Xeon @ 2.20 GHz.
  - Host B: AMD EPYC 7B12.
- Each command ran once. Timings come from GNU `time -v`. Temporary disk is the peak
  of `du -sb TMP_DIR`, sampled every 10 s.

Picard 3.5.0 cannot read this CRAM with its `.crai` (htsjdk throws
`CRAMException: Attempt to create a bai entry for an unmapped slice`).
turbo-picard reads it: on host A, CollectWgsMetrics took 4 min 53 s at 689 MB RSS
and MarkDuplicates 41 min 44 s at 1.06 GB, both straight from the CRAM.
That is why the Picard comparison below uses the BAM.

## MarkDuplicates

| Host | Tool | Wall | CPU | Peak RSS | Peak temp disk |
| --- | --- | ---: | ---: | ---: | ---: |
| A | Picard 3.4.0 | 3 h 30 m 07 s | 14,171 s | 34.4 GB | **23.3 GB** |
| A | turbo-picard `9a15db96` | **59 m 01 s** | 11,215 s | **355 MB** | 99.2 GB |
| B | turbo-picard `93216217` | **43 m 07 s** | 7,945 s | **354 MB** | not sampled |

On host A, turbo-picard was 3.6x faster than Picard and used 97x less memory.
It also used 4.3x more temporary disk; #86 tracks that.

Parity against Picard 3.4.0:

| Build | Metrics | SAM columns 1-11, all records | Duplicate-flagged records |
| --- | --- | --- | --- |
| `9a15db96` | READ_PAIR_OPTICAL_DUPLICATES 5,631,099 vs 5,620,745 | differ | 79,289,617 in both |
| `93216217` (#87) | exact | identical MD5 | 79,289,617 in both |

Two Picard rules caused the optical difference, and #87 fixes both:

- **16-bit coordinates.** Picard stores tile x and y as Java `short`. 12% of
  reads in this sample have y > 32767, so their coordinates wrap.
- **Mixed-orientation sets.** Picard splits a duplicate set into FR and RF
  parts before detecting optical duplicates.

Small fixtures never reach either case.

The `PG` tag on every record still differed after #87. Picard writes the
chained ID `MarkDuplicates.G` because the input header already has
`@PG ID:MarkDuplicates` from its 2016 run. #88 adds the same chaining.
After #88, MarkDuplicates on chr21 of this BAM (11,405,002 records) matches
Picard exactly in metrics and in the DT, DS, DI and PG tags of every record.
The whole genome has not been rerun on the #88 build.

## CollectWgsMetrics

Both runs used `INCLUDE_BQ_HISTOGRAM=true COUNT_UNPAIRED=true`.

| Host | Tool | Wall | CPU | Peak RSS |
| --- | --- | ---: | ---: | ---: |
| A | Picard 3.4.0 | 2 h 13 m 55 s | 8,141 s | 4.0 GB |
| A | turbo-picard `9a15db96` | **5 m 08 s** | 2,027 s | **352 MB** |

turbo-picard was 26x faster than Picard and used 11x less memory.
Every metric field and every histogram row matched Picard
(`tools/parity_compare.py wgs-metrics`).

## Reproduce

The scripts are the `wgs30x` and `p30v` runners described in
`benchmarks/duel/README.md`. They follow the same pattern: download from the
public bucket, convert to BAM, run Picard and turbo-picard, compare with
`tools/parity_compare.py`.
