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
| ViewSam | PASS | SAM record digest | 0.035s | 0.539s | 15.20x |
| CleanSam | PASS | post-command SAM record digest | 0.103s | 0.595s | 5.75x |
| CollectQualityYieldMetrics | PASS | stable metrics digest | 0.023s | 0.476s | 20.62x |
| CollectAlignmentSummaryMetrics | PASS | stable metrics digest | 0.028s | 0.551s | 19.34x |
| MarkDuplicates | PASS | duplicate-marking semantic digest plus stable metrics digest | 0.075s | 0.748s | 9.95x |
| AddOrReplaceReadGroups | PASS | SAM record digest plus read-group header digest | 0.182s | 0.602s | 3.30x |
| BuildBamIndex | PASS | BAI binary digest | 0.020s | 0.472s | 24.20x |
| RevertSam | PASS | reverted SAM record digest | 0.106s | 0.624s | 5.88x |
| SortSam | PASS | coordinate-sorted SAM record multiset digest | 0.188s | 0.582s | 3.09x |
| SamToFastq | PASS | FASTQ trio digest | 0.030s | 0.500s | 16.53x |
| CollectInsertSizeMetrics | PASS | stable metrics digest with insert-size histogram | 0.027s | 0.766s | 28.49x |
| ValidateSamFile | PASS | summary validation histogram plus exit code | 0.026s | 0.521s | 20.04x |

A PASS means the command-specific stable digest matched Picard on this input. Keep the JSON file with the raw digests when sharing results.

## Comparison details

- `SAM record digest` compares normalized SAM records and ignores headers.
- `post-command SAM record digest` compares normalized SAM records after a BAM-writing command.
- `reverted SAM record digest` compares normalized SAM records after RevertSam rewrites aligned records to unmapped output.
- `FASTQ trio digest` compares SamToFastq first-end, second-end, and unpaired FASTQ outputs byte-for-byte.
- `SAM record digest plus read-group header digest` compares normalized SAM records and sorted @RG header fields after AddOrReplaceReadGroups.
- `coordinate-sorted SAM record multiset digest` verifies coordinate sorting while allowing tie-order differences at the same position.
- `BAI binary digest` compares the exact BAM index bytes produced by BuildBamIndex.
- `stable metrics digest` compares non-comment, non-blank metrics rows so generated headers do not affect parity.
- `duplicate-marking semantic digest plus stable metrics digest` compares duplicate flags, duplicate tags, duplicate-set metadata, barcode tags, key coordinates, and duplicate metrics.
- `summary validation histogram plus exit code` compares the ValidateSamFile summary histogram and requires the same Picard and turbo-picard exit code.

## Artifact digests

| Command | turbo-picard artifact | Picard artifact | Digest | Exit codes |
| --- | --- | --- | --- | --- |
| ViewSam | `benchmarks/real-data/gatk-na12878-mito/evidence/work/ViewSam/turbo.sam` | `benchmarks/real-data/gatk-na12878-mito/evidence/work/ViewSam/picard.sam` | `86148164fd71...01775e2a7477` | n/a |
| CleanSam | `benchmarks/real-data/gatk-na12878-mito/evidence/work/CleanSam/turbo.bam` | `benchmarks/real-data/gatk-na12878-mito/evidence/work/CleanSam/picard.bam` | `86148164fd71...01775e2a7477` | n/a |
| CollectQualityYieldMetrics | `benchmarks/real-data/gatk-na12878-mito/evidence/work/CollectQualityYieldMetrics/turbo.metrics.txt` | `benchmarks/real-data/gatk-na12878-mito/evidence/work/CollectQualityYieldMetrics/picard.metrics.txt` | `4f8432dc643b...f8574a8a55c0` | n/a |
| CollectAlignmentSummaryMetrics | `benchmarks/real-data/gatk-na12878-mito/evidence/work/CollectAlignmentSummaryMetrics/turbo.metrics.txt` | `benchmarks/real-data/gatk-na12878-mito/evidence/work/CollectAlignmentSummaryMetrics/picard.metrics.txt` | `f9f1c7f169dd...a450b3c0c5bd` | n/a |
| MarkDuplicates | `benchmarks/real-data/gatk-na12878-mito/evidence/work/MarkDuplicates/turbo.bam` | `benchmarks/real-data/gatk-na12878-mito/evidence/work/MarkDuplicates/picard.bam` | `f1de85a7b16e...b8e66882f982` | n/a |
| AddOrReplaceReadGroups | `benchmarks/real-data/gatk-na12878-mito/evidence/work/AddOrReplaceReadGroups/turbo.bam` | `benchmarks/real-data/gatk-na12878-mito/evidence/work/AddOrReplaceReadGroups/picard.bam` | `d0a69a3e8f63...1b2368f25736` | n/a |
| BuildBamIndex | `benchmarks/real-data/gatk-na12878-mito/evidence/work/BuildBamIndex/turbo.bai` | `benchmarks/real-data/gatk-na12878-mito/evidence/work/BuildBamIndex/picard.bai` | `8384a8458756...d9295ab4a57e` | n/a |
| RevertSam | `benchmarks/real-data/gatk-na12878-mito/evidence/work/RevertSam/turbo.bam` | `benchmarks/real-data/gatk-na12878-mito/evidence/work/RevertSam/picard.bam` | `42ce02c7d89c...a5e81d75d886` | n/a |
| SortSam | `benchmarks/real-data/gatk-na12878-mito/evidence/work/SortSam/turbo.bam` | `benchmarks/real-data/gatk-na12878-mito/evidence/work/SortSam/picard.bam` | `782be3d28b6b...894f82eb7b5f` | n/a |
| SamToFastq | `benchmarks/real-data/gatk-na12878-mito/evidence/work/SamToFastq/turbo-r1.fastq` | `benchmarks/real-data/gatk-na12878-mito/evidence/work/SamToFastq/picard-r1.fastq` | `552fa95376e4...ebfd905d5e9d` | n/a |
| CollectInsertSizeMetrics | `benchmarks/real-data/gatk-na12878-mito/evidence/work/CollectInsertSizeMetrics/turbo.metrics.txt` | `benchmarks/real-data/gatk-na12878-mito/evidence/work/CollectInsertSizeMetrics/picard.metrics.txt` | `4af156149bc7...b04c79d882b3` | n/a |
| ValidateSamFile | `benchmarks/real-data/gatk-na12878-mito/evidence/work/ValidateSamFile/turbo.summary.txt` | `benchmarks/real-data/gatk-na12878-mito/evidence/work/ValidateSamFile/picard.summary.txt` | `ade471ddf5d9...3045666c88f9` | turbo-picard `2`, Picard `2` |
