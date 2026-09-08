# Saved benchmark summary

This preserves the detailed README benchmark record. Start with the [benchmark guide](benchmarks.rst) for scope and interpretation.

## Measurements

The saved public benchmark suite compares native `turbo-picard` commands against
Picard 3.4.0 and checks stable outputs before reporting speed. Current saved
results report `32/32` parity checks passing, with `272.12x` top speedup:
`NormalizeFasta`, `22.88x` floor speedup: `SetNmMdAndUqTags`, `99.51x`
median speedup, and `84.52x` geometric mean speedup.

**Read these as small-fixture measurements, not whole-genome speedups.**
For example, the saved `MarkDuplicates` case used the generator's `reads=50000`
setting, with median wall times of **0.075464 seconds** for Turbo Picard and
**2.238986 seconds** for Picard across three runs. Startup overhead matters at
this scale. The reported speedup is the median of paired-run ratios, which need
not equal the ratio of those independent medians. The machine-readable evidence
now preserves timings, repeat counts and generator parameters alongside ratios.

Summary: `32/32 PASS`; `272.12x` top speedup: `NormalizeFasta`;
`22.88x` floor speedup: `SetNmMdAndUqTags`; `99.51x` median speedup; `84.52x`
geometric mean speedup.

Benchmark details, scope notes, real-data evidence, and reproduction commands
are in the [benchmark docs](https://turbo-picard.readthedocs.io/en/latest/benchmarks.html).
The [parity guide](https://turbo-picard.readthedocs.io/en/latest/parity.html)
explains what the comparisons do and do not prove.
For `CollectBaseDistributionByCycle`, `CollectGcBiasMetrics`,
`CollectInsertSizeMetrics`, `MeanQualityByCycle`, and
`QualityScoreDistribution`, metrics text is the parity target; chart outputs are
lightweight PDF summaries, not Picard-equivalent rendered plots.

Saved benchmark run:

- Date: `2026-08-14`
- Command: `python3 tools/bench_suite.py --repeats 3 --skip-build`
- Raw log: `docs/site/assets/bench-suite-output.txt`
- benchmark exceptions: `AccelerationStatus`, `capabilities`, `doctor`,
  `explain`, and `trial` are utility commands, not Picard workload
  comparisons. `CollectHsMetrics` has separate ALL_READS and sidecar parity
  coverage, plus a real-data comparator path for pinned WES/capture intervals;
  representative capture-data performance evidence is still pending.

| Command | Speedup | Parity |
| --- | ---: | :--- |
| NormalizeFasta | 272.12x | PASS |
| BuildBamIndex | 243.53x | PASS |
| UpdateVcfSequenceDictionary | 207.52x | PASS |
| CollectGcBiasMetrics | 196.45x | PASS |
| CreateSequenceDictionary | 152.62x | PASS |
| GatherVcfs | 130.70x | PASS |
| LiftoverVcf | 127.28x | PASS |
| CollectMultipleMetrics | 122.66x | PASS |
| CollectInsertSizeMetrics | 117.34x | PASS |
| CleanSam | 115.42x | PASS |
| MergeVcfs | 112.13x | PASS |
| MeanQualityByCycle | 108.66x | PASS |
| CollectQualityYieldMetrics | 108.64x | PASS |
| QualityScoreDistribution | 107.17x | PASS |
| ReplaceSamHeader | 101.84x | PASS |
| ValidateSamFile | 99.56x | PASS |
| IntervalListTools | 99.46x | PASS |
| CollectBaseDistributionByCycle | 98.01x | PASS |
| SortVcf | 89.01x | PASS |
| CollectAlignmentSummaryMetrics | 83.49x | PASS |
| BedToIntervalList | 79.78x | PASS |
| ViewSam | 79.58x | PASS |
| AddOrReplaceReadGroups | 77.78x | PASS |
| SamToFastq | 77.74x | PASS |
| CollectWgsMetrics | 50.81x | PASS |
| MergeSamFiles | 35.49x | PASS |
| SortSam | 35.15x | PASS |
| FixMateInformation | 31.98x | PASS |
| FastqToSam | 31.29x | PASS |
| MarkDuplicates | 28.70x | PASS |
| RevertSam | 24.19x | PASS |
| SetNmMdAndUqTags | 22.88x | PASS |

Release evidence checks:

```bash
python3 tools/update_real_data_manifest.py
python3 tools/verify_benchmark_log_evidence.py
python3 tools/verify_benchmark_suite_coverage.py
python3 tools/verify_benchmark_thresholds.py
python3 tools/verify_real_data_evidence.py
python3 tools/verify_real_data_evidence.py --release-ready
```

Real-data evidence lives in `benchmarks/real-data/` and records pinned input
sources, command scopes, and input SHA-256 hashes. Current release-candidate
dataset IDs are `gatk-na12878-mito`, `picard-snvq`, and
`gatk-na12878-mito-cram`.
