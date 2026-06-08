# QC benchmark: Picard vs turbo-picard vs riker

- sample: `gatk-na12878-mito`
- input: `/Users/donncha/Documents/GitHub/turbo-picard/benchmarks/real-data/gatk-na12878-mito/input.bam`
- input bytes: `2097008`

## wgs-bundle

| tool | label | wall (s) | max RSS (GB) | vs Picard |
| --- | --- | ---: | ---: | ---: |
| picard | CollectGcBiasMetrics | 1.237 | n/a | 1.00x |
| picard | CollectMultipleMetrics | 2.899 | n/a | 1.00x |
| picard | CollectWgsMetrics | 6.626 | n/a | 1.00x |
| riker | multi | 0.049 | n/a | 218.05x |
| turbo-picard | CollectMultipleMetrics | 0.025 | n/a | 424.62x |

- turbo-picard bundle speedup vs Picard: **424.62x**
- riker bundle speedup vs Picard: **218.05x**
- turbo-picard vs riker: **1.95x**

## wgs-only

| tool | label | wall (s) | max RSS (GB) | vs Picard |
| --- | --- | ---: | ---: | ---: |
| picard | CollectWgsMetrics | 6.627 | n/a | 1.00x |
| riker | wgs | 0.021 | n/a | 311.44x |
| turbo-picard | CollectWgsMetrics | 0.012 | n/a | 539.44x |

- turbo-picard bundle speedup vs Picard: **539.44x**
- riker bundle speedup vs Picard: **311.44x**
- turbo-picard vs riker: **1.73x**

