# QC benchmark: Picard vs turbo-picard vs riker

- sample: `gatk-na12878-mito`
- input: `benchmarks/real-data/gatk-na12878-mito/input.bam`
- input bytes: `2097008`

## wgs-bundle

| tool | label | wall (s) | max RSS (GB) | vs Picard |
| --- | --- | ---: | ---: | ---: |
| picard | CollectGcBiasMetrics | 1.412 | n/a | 1.00x |
| picard | CollectMultipleMetrics | 3.293 | n/a | 1.00x |
| picard | CollectWgsMetrics | 7.096 | n/a | 1.00x |
| riker | multi | 0.053 | n/a | 222.79x |
| turbo-picard | CollectMultipleMetrics | 0.025 | n/a | 477.44x |

- turbo-picard profile speedup vs Picard: **477.44x**
- riker profile speedup vs Picard: **222.79x**
- turbo-picard vs riker: **2.14x**

## wgs-only

| tool | label | wall (s) | max RSS (GB) | vs Picard |
| --- | --- | ---: | ---: | ---: |
| picard | CollectWgsMetrics | 7.302 | n/a | 1.00x |
| riker | wgs | 0.024 | n/a | 301.22x |
| turbo-picard | CollectWgsMetrics | 0.012 | n/a | 633.74x |

- turbo-picard profile speedup vs Picard: **633.74x**
- riker profile speedup vs Picard: **301.22x**
- turbo-picard vs riker: **2.10x**

