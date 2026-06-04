# turbo-picard real-data comparison

Input BAM: `benchmarks/real-data/picard-test-bam/input.bam`
Input SHA-256: `1d499e5683479b88fad373b2de8b49f85cceae68a316e06b3cfdf60491fd7990`
Input size: `940749` bytes
Input source: `https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/test.bam`
Input source commit: `fc0b08410d38a10afd08e467dab74bf5e2e71310`
Picard: `Version:3.4.0`
turbo-picard: `picard 0.1.0`

| Command | Status | Comparison | turbo-picard | Picard | Speedup |
| --- | --- | --- | ---: | ---: | ---: |
| ViewSam | PASS | SAM record digest | 0.366s | 0.687s | 1.88x |
| CleanSam | PASS | post-command SAM record digest | 0.062s | 0.499s | 8.10x |
| CollectQualityYieldMetrics | PASS | stable metrics digest | 0.011s | 0.430s | 37.43x |
| CollectAlignmentSummaryMetrics | PASS | stable metrics digest | 0.016s | 0.455s | 28.99x |
| MarkDuplicates | PASS | duplicate-marking semantic digest plus stable metrics digest | 0.035s | 0.569s | 16.10x |

A PASS means the command-specific stable digest matched Picard on this input. Keep the JSON file with the raw digests when sharing results.

## Comparison details

- `SAM record digest` compares normalized SAM records and ignores headers.
- `post-command SAM record digest` compares normalized SAM records after a BAM-writing command.
- `stable metrics digest` compares non-comment, non-blank metrics rows so generated headers do not affect parity.
- `duplicate-marking semantic digest plus stable metrics digest` compares duplicate flags, duplicate tags, duplicate-set metadata, barcode tags, key coordinates, and duplicate metrics.

## Artifact digests

| Command | turbo-picard artifact | Picard artifact | Digest | Exit codes |
| --- | --- | --- | --- | --- |
| ViewSam | `benchmarks/real-data/picard-test-bam/evidence/work/ViewSam/turbo.sam` | `benchmarks/real-data/picard-test-bam/evidence/work/ViewSam/picard.sam` | `cb95309b0b05...80e29cd6748c` | n/a |
| CleanSam | `benchmarks/real-data/picard-test-bam/evidence/work/CleanSam/turbo.bam` | `benchmarks/real-data/picard-test-bam/evidence/work/CleanSam/picard.bam` | `cb95309b0b05...80e29cd6748c` | n/a |
| CollectQualityYieldMetrics | `benchmarks/real-data/picard-test-bam/evidence/work/CollectQualityYieldMetrics/turbo.metrics.txt` | `benchmarks/real-data/picard-test-bam/evidence/work/CollectQualityYieldMetrics/picard.metrics.txt` | `1bf7b224da8c...672bc471c391` | n/a |
| CollectAlignmentSummaryMetrics | `benchmarks/real-data/picard-test-bam/evidence/work/CollectAlignmentSummaryMetrics/turbo.metrics.txt` | `benchmarks/real-data/picard-test-bam/evidence/work/CollectAlignmentSummaryMetrics/picard.metrics.txt` | `f9c53dc9ead5...3887efa3c0e3` | n/a |
| MarkDuplicates | `benchmarks/real-data/picard-test-bam/evidence/work/MarkDuplicates/turbo.bam` | `benchmarks/real-data/picard-test-bam/evidence/work/MarkDuplicates/picard.bam` | `df9941d245b9...a08d9d555c44` | n/a |
