# turbo-picard real-data comparison

Input BAM: `benchmarks/real-data/gatk-na12878-mito-cram/input.cram`
Input SHA-256: `68931e7cea6e9a35029cfed3638d0d8ea2c4bb662b4d83232968da247b68f7bc`
Input size: `910668` bytes
Input source: `https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam`
Input source commit: `e8c49f600b06c658e0fa9bf67256340ebb46bc48`
Picard: `Version:3.4.0`
turbo-picard: `picard 0.1.1`

| Command | Status | Comparison | turbo-picard | Picard | Speedup |
| --- | --- | --- | ---: | ---: | ---: |
| CleanSam | PASS | post-command SAM record digest | 0.587s | 2.727s | 4.64x |
| CollectQualityYieldMetrics | PASS | stable metrics digest | 0.032s | 2.307s | 73.07x |
| CollectInsertSizeMetrics | PASS | stable metrics digest with insert-size histogram | 0.064s | 2.224s | 35.01x |
| MarkDuplicates | PASS | duplicate-marking semantic digest plus stable metrics digest | 0.390s | 2.769s | 7.10x |
| SortSam | PASS | coordinate-sorted SAM record multiset digest | 0.254s | 1.501s | 5.91x |
| AddOrReplaceReadGroups | PASS | SAM record digest plus read-group header digest | 0.216s | 1.330s | 6.16x |

A PASS means the command-specific stable digest matched Picard on this input. Keep the JSON file with the raw digests when sharing results.

## Comparison details

- `post-command SAM record digest` compares normalized SAM records after a BAM-writing command.
- `SAM record digest plus read-group header digest` compares normalized SAM records and sorted @RG header fields after AddOrReplaceReadGroups.
- `coordinate-sorted SAM record multiset digest` verifies coordinate sorting while allowing tie-order differences at the same position.
- `stable metrics digest` compares non-comment, non-blank metrics rows so generated headers do not affect parity.
- `duplicate-marking semantic digest plus stable metrics digest` compares duplicate flags, duplicate tags, duplicate-set metadata, barcode tags, key coordinates, and duplicate metrics.

## Artifact digests

| Command | turbo-picard artifact | Picard artifact | Digest | Exit codes |
| --- | --- | --- | --- | --- |
| CleanSam | `benchmarks/real-data/gatk-na12878-mito-cram/evidence/work/CleanSam/turbo.cram` | `benchmarks/real-data/gatk-na12878-mito-cram/evidence/work/CleanSam/picard.cram` | `b49969e6a0b1...97246825c965` | n/a |
| CollectQualityYieldMetrics | `benchmarks/real-data/gatk-na12878-mito-cram/evidence/work/CollectQualityYieldMetrics/turbo.metrics.txt` | `benchmarks/real-data/gatk-na12878-mito-cram/evidence/work/CollectQualityYieldMetrics/picard.metrics.txt` | `4f8432dc643b...f8574a8a55c0` | n/a |
| CollectInsertSizeMetrics | `benchmarks/real-data/gatk-na12878-mito-cram/evidence/work/CollectInsertSizeMetrics/turbo.metrics.txt` | `benchmarks/real-data/gatk-na12878-mito-cram/evidence/work/CollectInsertSizeMetrics/picard.metrics.txt` | `4af156149bc7...b04c79d882b3` | n/a |
| MarkDuplicates | `benchmarks/real-data/gatk-na12878-mito-cram/evidence/work/MarkDuplicates/turbo.cram` | `benchmarks/real-data/gatk-na12878-mito-cram/evidence/work/MarkDuplicates/picard.cram` | `f1de85a7b16e...b8e66882f982` | n/a |
| SortSam | `benchmarks/real-data/gatk-na12878-mito-cram/evidence/work/SortSam/turbo.cram` | `benchmarks/real-data/gatk-na12878-mito-cram/evidence/work/SortSam/picard.cram` | `3f1dedc33cad...b9400f08267e` | n/a |
| AddOrReplaceReadGroups | `benchmarks/real-data/gatk-na12878-mito-cram/evidence/work/AddOrReplaceReadGroups/turbo.cram` | `benchmarks/real-data/gatk-na12878-mito-cram/evidence/work/AddOrReplaceReadGroups/picard.cram` | `4dab7d93d528...89331174ca8c` | n/a |
