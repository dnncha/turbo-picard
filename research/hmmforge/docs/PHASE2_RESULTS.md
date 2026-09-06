# Phase-two evidence: HMMForge 0.1.0a3

Code commit: `825607c3976035cf1ef41fd1dae66b1d0586c998`. Package-source SHA256:
`1c8b4f0574bc96f3057acf4265480a71d676ac2a6ef9e50a9b5a8912c8ed457d`.

## Completed regression and packaging gate

GitHub Actions run **34026780616**, package job **101469049073**, completed
successfully. **65 tests passed**, with no failures, errors or skipped tests.
The job also passed two three-engine studies, two independent native-HMMER
checks, and a built-wheel installation and verification check. The tested wheel
is in GitHub artifact **9987312491**. All six package Python files in that wheel
were compared with the local tested source and match byte-for-byte.

## Stronger baseline results

Three fresh-process repetitions per engine. Rows below are separate experiments;
do not compare absolute runtimes between the hosted runner and local container.
The direct baseline holds all proteins in memory and has independently written
extraction, but is authored in this project and shares the upstream HMMER kernels.
It is not an externally reviewed expert implementation.

| Workload | Optimized scan | Direct model-major | HMMForge | Speedup vs scan | Speedup vs direct |
|---|---:|---:|---:|---:|---:|
| Hosted: 14 models / 2,100 biological proteins | 0.415 s | 0.264 s | 0.264 s | 1.57x | 1.00x |
| Hosted: 64 models / 2,000 synthetic proteins | 1.617 s | 1.066 s | 1.066 s | 1.52x | 1.00x |
| Local: 256 models / 10,000 synthetic proteins | 28.058 s | 14.206 s | 13.645 s | 2.06x | 1.04x |

**All three experiments passed structural/numeric parity.** All nine outputs
within each experiment also had identical SHA256s. Exact measurements, source
hashes, CPU time, peak memory and phase timings are retained in the evidence.
On the larger local synthetic case, HMMForge used 49.094 CPU-seconds versus
51.445 for direct model-major (about 4.6% less), but its observed peak RSS was
208.3 MB versus 189.1 MB (about 10.1% more). There is no demonstrated memory-class
advantage. The local container had a four-CPU cgroup quota and 4 GiB memory limit.

**Interpretation:** the current execution layer is essentially tied with a
straightforward optimized model-major baseline on the small hosted tests, and
only modestly ahead on the larger synthetic case. Scan-only gains must not be
presented as gains over best-practice model-major execution. This is a useful
feasibility result, not the several-fold production cost reduction sought.

The workload stage timers place most of the larger local run in combined
digitization/search/extraction, not initial model preparation. That combined
phase does not distinguish native filters, Forward/Backward, scheduling or
Python extraction. A native profile is necessary to choose between them.

The hosted sub-second measurements are especially sensitive to process startup
and subprocess timeout-wait observation overhead. Differences near 1.00x should
not be interpreted as statistically established wins. OS caches were not flushed.

## Native-HMMER validation scope

Native HMMER 3.4 checks on the synthetic and small biological fixtures reported
zero mismatches in the checked hit identities, domain coordinate sets, scores,
bias and E-values at printed precision. Inclusion flags and alignment strings
are excluded from those independent table checks. Inclusion flags are covered
by PyHMMER differential tests, not by the native-table verifier.

## Full-catalogue gate

The separate `catalogue` job in run 34026780616 acquired the fixed Pfam 38.0
catalogue and began a three-engine study using 512 hash-selected proteins.
Completion and native-profile status must be read from that job's retained
artifact before being reported as successful. This is a full MODEL-library
smoke test, not the 100,000-novel-protein metagenomic adoption gate.

## Evidence and continuation

`evidence/phase2-summary.json` retains all elapsed/CPU/RSS observations and
provenance for these studies. The standalone source archive additionally carries
`evidence/phase2.json`, full original study reports and the JUnit results.
The original hosted raw outputs and installable distributions are in artifact
9987312491. `docs/STUDY.md` defines the next benchmark and profiling steps.

No new scoring kernel, GPU implementation, production validation or monetary
cost saving is claimed. Turbo Picard main is not part of this research change.
