# turbo-picard real-data comparison

Input BAM: `benchmarks/real-data/picard-snvq/input.bam`
Input SHA-256: `be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3`
Input size: `9451956` bytes
Input source: `https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam`
Input source commit: `fc0b08410d38a10afd08e467dab74bf5e2e71310`
Picard: `Version:3.4.0`
turbo-picard: `picard 0.1.12`

| Command | Status | Comparison | turbo-picard | Picard | Speedup |
| --- | --- | --- | ---: | ---: | ---: |
| ViewSam | PASS | SAM record digest | 0.256s | 1.052s | 4.10x |
| CleanSam | PASS | post-command SAM record digest | 0.272s | 1.332s | 4.89x |
| CollectQualityYieldMetrics | PASS | stable metrics digest | 0.026s | 0.448s | 17.51x |
| CollectAlignmentSummaryMetrics | PASS | stable metrics digest | 0.027s | 0.492s | 18.02x |
| MarkDuplicates | PASS | duplicate-marking semantic digest plus stable metrics digest | 0.306s | 1.909s | 6.24x |

A PASS means the command-specific stable digest matched Picard on this input. Keep the JSON file with the raw digests when sharing results.

## Comparison details

- `SAM record digest` compares normalized SAM records and ignores headers.
- `post-command SAM record digest` compares normalized SAM records after a BAM-writing command.
- `stable metrics digest` compares non-comment, non-blank metrics rows so generated headers do not affect parity.
- `duplicate-marking semantic digest plus stable metrics digest` compares duplicate flags, duplicate tags, duplicate-set metadata, barcode tags, key coordinates, and duplicate metrics.

## Artifact digests

| Command | turbo-picard artifact | Picard artifact | Digest | Exit codes |
| --- | --- | --- | --- | --- |
| ViewSam | `benchmarks/real-data/picard-snvq/evidence/work/ViewSam/turbo.sam` | `benchmarks/real-data/picard-snvq/evidence/work/ViewSam/picard.sam` | `791d618746fb...a4e6aa241e77` | n/a |
| CleanSam | `benchmarks/real-data/picard-snvq/evidence/work/CleanSam/turbo.bam` | `benchmarks/real-data/picard-snvq/evidence/work/CleanSam/picard.bam` | `791d618746fb...a4e6aa241e77` | n/a |
| CollectQualityYieldMetrics | `benchmarks/real-data/picard-snvq/evidence/work/CollectQualityYieldMetrics/turbo.metrics.txt` | `benchmarks/real-data/picard-snvq/evidence/work/CollectQualityYieldMetrics/picard.metrics.txt` | `c68330b7a13c...a400783106cd` | n/a |
| CollectAlignmentSummaryMetrics | `benchmarks/real-data/picard-snvq/evidence/work/CollectAlignmentSummaryMetrics/turbo.metrics.txt` | `benchmarks/real-data/picard-snvq/evidence/work/CollectAlignmentSummaryMetrics/picard.metrics.txt` | `bef3df49f2ab...5c2a6efb6fba` | n/a |
| MarkDuplicates | `benchmarks/real-data/picard-snvq/evidence/work/MarkDuplicates/turbo.bam` | `benchmarks/real-data/picard-snvq/evidence/work/MarkDuplicates/picard.bam` | `138655065cc2...c9c689f464cc` | n/a |
