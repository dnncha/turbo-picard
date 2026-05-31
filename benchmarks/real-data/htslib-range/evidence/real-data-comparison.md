# turbo-picard real-data comparison

Input BAM: `benchmarks/real-data/htslib-range/input.bam`
Input SHA-256: `e15d14e3994027d433431c960bf1c5f2d6939f26b5094cd5a86bc6229a5b2661`
Input size: `13337` bytes
Input source: `https://github.com/samtools/htslib/blob/5cded8325aca2f84f6c18641664893b900638086/test/range.bam`
Input source commit: `5cded8325aca2f84f6c18641664893b900638086`
Picard: `Version:3.4.0`
turbo-picard: `picard 0.1.0`

| Command | Status | Comparison | turbo-picard | Picard | Speedup |
| --- | --- | --- | ---: | ---: | ---: |
| ViewSam | PASS | SAM record digest | 0.008s | 0.809s | 99.57x |
| CleanSam | PASS | post-command SAM record digest | 0.019s | 0.826s | 43.08x |
| CollectQualityYieldMetrics | PASS | stable metrics digest | 0.008s | 0.862s | 104.39x |
| CollectAlignmentSummaryMetrics | PASS | stable metrics digest | 0.008s | 0.932s | 115.95x |
| MarkDuplicates | PASS | duplicate-marking semantic digest plus stable metrics digest | 0.040s | 0.977s | 24.67x |

A PASS means the command-specific stable digest matched Picard on this input. Keep the JSON file with the raw digests when sharing results.
