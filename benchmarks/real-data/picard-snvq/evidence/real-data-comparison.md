# turbo-picard real-data comparison

Input BAM: `benchmarks/real-data/picard-snvq/input.bam`
Input SHA-256: `be0daa7cb8e9ce11f2f68ac3db8c229d530736aaf7b80df3669fdb00779c06b3`
Input size: `9451956` bytes
Input source: `https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam`
Input source commit: `fc0b08410d38a10afd08e467dab74bf5e2e71310`
Picard: `Version:3.4.0`
turbo-picard: `picard 0.1.0`

| Command | Status | Comparison | turbo-picard | Picard | Speedup |
| --- | --- | --- | ---: | ---: | ---: |
| ViewSam | FAIL | SAM record digest | 0.195s | 1.088s | 5.58x |
| CleanSam | FAIL | post-command SAM record digest | 1.050s | 1.742s | 1.66x |
| CollectQualityYieldMetrics | PASS | stable metrics digest | 0.089s | 0.834s | 9.37x |
| CollectAlignmentSummaryMetrics | FAIL | stable metrics digest | 0.083s | 0.737s | 8.87x |
| MarkDuplicates | PASS | duplicate-marking semantic digest plus stable metrics digest | 0.597s | 2.107s | 3.53x |

A PASS means the command-specific stable digest matched Picard on this input. Keep the JSON file with the raw digests when sharing results.
