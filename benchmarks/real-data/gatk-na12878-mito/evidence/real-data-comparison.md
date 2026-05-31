# turbo-picard real-data comparison

Input BAM: `benchmarks/real-data/gatk-na12878-mito/input.bam`
Input SHA-256: `70ea2e429805a75ce6007a32ba176ea7c697a398e0c39a9d58aaaa30e1ed86c3`
Input size: `2097008` bytes
Input source: `https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam`
Input source commit: `e8c49f600b06c658e0fa9bf67256340ebb46bc48`
Picard: `Version:3.4.0`
turbo-picard: `picard 0.1.0`

| Command | Status | Comparison | turbo-picard | Picard | Speedup |
| --- | --- | --- | ---: | ---: | ---: |
| ViewSam | PASS | SAM record digest | 0.615s | 4.187s | 6.81x |
| CleanSam | PASS | post-command SAM record digest | 0.234s | 2.082s | 8.90x |
| CollectQualityYieldMetrics | PASS | stable metrics digest | 0.046s | 1.361s | 29.77x |
| CollectAlignmentSummaryMetrics | FAIL | stable metrics digest | 0.046s | 1.654s | 35.96x |
| MarkDuplicates | FAIL | duplicate-marking semantic digest plus stable metrics digest | 0.114s | 4.283s | 37.45x |

A PASS means the command-specific stable digest matched Picard on this input. Keep the JSON file with the raw digests when sharing results.
