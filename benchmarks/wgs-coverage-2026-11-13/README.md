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

Each case compared every metric field and every histogram row, with
`INCLUDE_BQ_HISTOGRAM=true`, for both the single pass
(`TURBO_PICARD_REGION_WORKERS=1`) and six region workers.

| Case on the slice | Options | Result |
| --- | --- | --- |
| default | none | identical to Picard, both paths |
| count-unpaired | `COUNT_UNPAIRED=true` | identical to Picard, both paths |
| coverage-cap-8 | `COVERAGE_CAP=8 COUNT_UNPAIRED=true` | identical to Picard, both paths |
| no-quality-floor | `MINIMUM_BASE_QUALITY=0 MINIMUM_MAPPING_QUALITY=0` | identical to Picard, both paths |
| accumulation-cap | `COVERAGE_CAP=4 LOCUS_ACCUMULATION_CAP=6 COUNT_UNPAIRED=true` | differed at 131 of 139,438,517 loci; the head-of-accumulator rule was corrected afterwards and this slice has not been rerun |

The benchmark host stopped answering before the slice could be rerun with the
corrected accumulation rule or with `STOP_AFTER` and `INTERVALS`. Those cases
were instead compared on the checked-in NA12878 mitochondrial BAM
(`benchmarks/real-data/gatk-na12878-mito/input.bam`: 14,917 records, 583
supplementary, 1,848 duplicates, one reference `N`) against Picard 3.4.0 on a
second host:

| Case on the mitochondrial BAM | Options | Result |
| --- | --- | --- |
| default, count-unpaired, coverage-cap-8, no-quality-floor | as above | identical to Picard, both paths |
| accumulation-cap | `COVERAGE_CAP=4 LOCUS_ACCUMULATION_CAP=6 COUNT_UNPAIRED=true` | identical to Picard, both paths |
| stop-after | `STOP_AFTER` of 300, 5,000 and 16,000, with and without `COUNT_UNPAIRED=true` | identical to Picard, both paths |
| intervals | three intervals, `COUNT_UNPAIRED=true` | identical to Picard, both paths |
| intervals-stop-after | intervals with `STOP_AFTER` of 100, 1,040 and 3,240 | identical to Picard, both paths |

That is 54 comparisons with no differences. The whole 4x, 15x and 30x genomes
have not been rerun with the final collector.

`tools/compare_wgs_metrics_real_data.sh` reruns every case against a BAM or
CRAM of your own:

```bash
TURBO_PICARD_BIN=./target/release/turbo-picard \
  tools/compare_wgs_metrics_real_data.sh input.bam reference.fa /tmp/wgs-compare picard
```

The mitochondrial NA12878 BAM and CRAM in `benchmarks/real-data/` are compared
with Picard by the parity gate with the same exact comparator.

## Timing

Timings for the Picard-exact collector have not been recorded yet. The
31 October 2026 run measured Picard 3.5.0 at 1,033.8 s and turbo-picard 0.1.18
at 166.5 s on the 4x genome; the 0.1.18 figure is for the output that did not
match Picard and should not be quoted as a like-for-like speedup.
