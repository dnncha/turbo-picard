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
| ViewSam | PASS | SAM record digest | 0.018s | 0.583s | 32.15x |
| CleanSam | PASS | post-command SAM record digest | 0.058s | 0.630s | 10.94x |
| CollectQualityYieldMetrics | PASS | stable metrics digest | 0.012s | 0.572s | 47.00x |
| CollectAlignmentSummaryMetrics | FAIL | stable metrics digest | 0.012s | 0.548s | 44.49x |
| MarkDuplicates | PASS | duplicate-marking semantic digest plus stable metrics digest | 0.033s | 0.763s | 22.86x |

A PASS means the command-specific stable digest matched Picard on this input. Keep the JSON file with the raw digests when sharing results.
