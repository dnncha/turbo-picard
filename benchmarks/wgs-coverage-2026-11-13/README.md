# CollectWgsMetrics on real whole-genome data: 13 November 2026

This record covers one sample on one host. It shows where turbo-picard 0.1.18
differed from Picard on real data and what was checked after the rewrite. It is
not a cohort validation.

## Inputs

- Sample: 1000 Genomes HG02675, from
  `https://1000genomes.s3.amazonaws.com/1000G_2504_high_coverage/data/ERR3242389/HG02675.final.cram`,
  transcoded to BAM and downsampled to 4x, 15x and 30x on 31 October 2026.
- Reference: GRCh38 with decoys and HLA (`grch38_decoyhla.fa`).
- Slice used for option coverage: chr20, chr21, chr22, chrUn_KI270742v1, chrEBV
  and chrUn_JTFH01000001v1_decoy of the 4x BAM, re-encoded against a reference
  holding only those contigs. 5,346,958 records, 58,325 supplementary,
  545,619 duplicates, territory 139,438,517 after excluding reference `N`.
- Picard: 3.5.0 from Bioconda. Host: Intel Core i7-8700, 12 threads, 16 GB.

## What 0.1.18 got wrong

On the whole 4x genome with `COUNT_UNPAIRED=true`:

| Field | Picard 3.5.0 | turbo-picard 0.1.18 |
| --- | ---: | ---: |
| GENOME_TERRITORY | 3,043,453,562 | 3,217,346,917 |
| MEAN_COVERAGE | 3.815218 | 3.600677 |
| MEDIAN_COVERAGE | 4 | 3 |
| PCT_EXC_MAPQ | 0.069538 | 0.056303 |
| PCT_EXC_DUPE | 0.104666 | 0.109566 |
| PCT_1X | 0.914596 | 0.864041 |
| HET_SNP_SENSITIVITY | 0.554231 | 0.522494 |

The causes were reference `N` bases counted as territory, supplementary
alignments dropped, exclusion categories applied in a different order and
counted per base, and het sensitivity computed from the filtered histograms.
The parity comparator allowed a territory difference of one and overlap
differences of 0.05, and the fixtures were too small to exceed either.

## What was checked after the rewrite

Each case compared every metric field and every histogram row for both the
single pass (`TURBO_PICARD_REGION_WORKERS=1`) and six region workers.

Whole 4x genome, `COUNT_UNPAIRED=true`, against the Picard 3.5.0 output saved
on 31 October 2026: identical for both paths.

Slice of the 4x genome, with `INCLUDE_BQ_HISTOGRAM=true`, against Picard 3.5.0:

| Case | Options | Result |
| --- | --- | --- |
| default | none | identical, both paths |
| count-unpaired | `COUNT_UNPAIRED=true` | identical, both paths |
| coverage-cap-8 | `COVERAGE_CAP=8 COUNT_UNPAIRED=true` | identical, both paths |
| no-quality-floor | `MINIMUM_BASE_QUALITY=0 MINIMUM_MAPPING_QUALITY=0` | identical, both paths |
| accumulation-cap | `COVERAGE_CAP=4 LOCUS_ACCUMULATION_CAP=6 COUNT_UNPAIRED=true` | identical, both paths |
| stop-after | `STOP_AFTER=3000000 COUNT_UNPAIRED=true` | identical, both paths |
| stop-after, small | `STOP_AFTER=70000` | identical, both paths |
| intervals | four intervals, `COUNT_UNPAIRED=true` | identical, both paths |
| intervals with stop-after | four intervals, `STOP_AFTER=2500000` | identical, both paths |

The same cases were run on the checked-in NA12878 mitochondrial BAM
(`benchmarks/real-data/gatk-na12878-mito/input.bam`: 14,917 records, 583
supplementary, 1,848 duplicates, one reference `N`) against Picard 3.4.0 on a
second host, with `STOP_AFTER` of 300, 5,000 and 16,000: 54 comparisons, no
differences.

Checked later: the full 30x genome matched Picard 3.4.0 field for field on
23 November 2026 ([`../wgs-30x-colab-2026-11-23/README.md`](../wgs-30x-colab-2026-11-23/README.md)).
Still not checked: 15x, and CRAM, RNA or long-read input at genome scale
(Picard 3.5.0 cannot open the 1000 Genomes CRAM with its index).

`tools/compare_wgs_metrics_real_data.sh` reruns the cases against a BAM or CRAM
of your own:

```bash
TURBO_PICARD_BIN=./target/release/turbo-picard \
  tools/compare_wgs_metrics_real_data.sh input.bam reference.fa /tmp/wgs-compare picard
```

The mitochondrial NA12878 BAM and CRAM in `benchmarks/real-data/` are compared
with Picard by the parity gate with the same exact comparator.

## Timing

One host, local disk, one run per row unless a range is given, input page cache
dropped between runs. `riker wgs` 0.5.0 is included as the nearest native tool;
its metrics differ from Picard's by design, so it is not a like-for-like
replacement.

| Input | Tool | Threads | Wall seconds | Peak RSS |
| --- | --- | ---: | ---: | ---: |
| 4x | Picard 3.5.0 (31 October run) | 1 | 1,033.8 | 1,468 MB |
| 4x | turbo-picard, single pass | 1 | 76.1 | 50 MB |
| 4x | turbo-picard, region workers | 6 | 21.8 to 22.6 | 125 MB |
| 4x | riker wgs | 6 | 29.7 | 504 MB |
| 15x | turbo-picard, region workers | 6 | 65.4 to 82.6 | 274 MB |
| 15x | riker wgs | 6 | 78.2 to 79.7 | 518 MB |
| 30x | turbo-picard, region workers | 6 | 193.5 | 412 MB |
| 30x | riker wgs | 6 | 147.4 | 510 MB |

On the 4x genome the output is identical to Picard's in about one forty-sixth
of the time. At 30x turbo-picard is slower than riker: this sample has a contig
whose pileup reaches the default `LOCUS_ACCUMULATION_CAP` of 100,000 reads, and
that contig is reprocessed in a single pass to keep Picard's result. Picard was
not timed at 15x or 30x.
