# HMMForge: measured results

## Version and evidence

Tested implementation: **0.1.0a2**, commit
`d7ce2823839ed4cb98ace1b4fcefa405a3a0b69b`.

[Completed evidence run](https://github.com/dnncha/turbo-picard/actions/runs/34000192881)
ran on 6 September 2026. The HMMForge job completed successfully.
The unrelated Turbo Picard workflow is not evidence for HMMForge.

The package-source SHA256 in every benchmark subprocess is:
`252f8cba85dfd7c574b59f180e840976573bbd77aa0f4aa6922f39d82d188c65`.
The downloadable source package contains those same four Python source files.

## Tests

**42 passed; zero failures, errors or skipped tests.**
Tests cover model-specific GA/TC/NC cutoffs, multiple domains, multiple matching
models per protein, no-hit proteins, ambiguous amino acids, batch-size and
thread-count invariance, prepared-profile reuse, conditional domain thresholds,
duplicate FASTA names, input validation, non-ASCII rejection and atomic output.

Separately installed **HMMER 3.4** produced the same checked target/domain tables
on both workloads: zero mismatches in reported target identities, reported
domain coordinate sets, scores, bias and E-values at native table precision.
This independent check does not cover inclusion flags, alignment strings or
complete CLI behavior. Inclusion flags are covered by the PyHMMER differential
tests, not claimed as checked by the native-table script.

## Same-run end-to-end measurements

Baseline: **PyHMMER 0.12.3 in-memory optimized-profile hmmscan**. Candidate:
HMMForge's model-major execution with prepared-profile reuse. Both use the same
HMMER kernels, two workers, inputs, thresholds and output schema.

Three fresh-process repeats per engine; order alternates. Times include startup,
input hashing, model preparation, input processing, search, output serialization
and file publication. OS page caches are uncontrolled. These are small runs on
a shared hosted runner, not a dedicated performance laboratory.

| Workload | Scan median | HMMForge median | Elapsed speedup | Scan CPU-seconds | HMMForge CPU-seconds |
|---|---:|---:|---:|---:|---:|
| Synthetic: 64 models, 2,000 proteins | 1.7304 s | 1.1535 s | 1.500x | 2.3414 | 2.0245 |
| Biological fixture: 14 models, 2,100 proteins | 0.3597 s | 0.2441 s | 1.473x | 0.4638 | 0.2941 |

CPU-time reductions are approximately **13.5%** and **36.6%**, respectively.
Wall-time ratios are not CPU-time ratios and neither is a measured cloud bill.
A workload billed for a reserved node has different economics from a shared
cluster charged for actual CPU time. No monetary saving is claimed here.

Median peak RSS: synthetic scan 56,438,784 bytes vs candidate 55,152,640;
biological scan 42,147,840 bytes vs candidate 41,648,128. This is not a decisive
memory-class reduction.

All six output files within each workload had the same SHA256:

- Synthetic: `9e866aa3d40dc70cc6e76aeb42160e97eaeb4c82985c2368bda51d030a26286f`.
- Biological: `c4a79edba705fa1a2dc8eec3536d66c0805c313e71ff20de64258b979e58f68a`.

The synthetic workload returned 1,521 reported matches and 1,685 domains.
The biological fixture returned 43 reported matches and 67 domains.
Byte-identical output on these runs is an observation, not a guarantee for every
input, platform or version. The verifier's general contract uses explicit
floating-point tolerances.

## Biological fixture provenance

The small biological test uses profile and proteome fixtures shipped with pinned
PyHMMER. It is not a full Pfam database or a novel metagenomic catalogue.
Before either engine runs, the fixture-preparation script explicitly removes
2,099 terminal translation-stop markers. It removes no internal stops. Original
and prepared hashes are preserved in `biological-fixture-provenance.json`.
The annotation CLI itself rejects stop markers; it does not silently repair them.

## What these results justify

A working, tested prototype and continued performance investigation. They do
**not** establish several-fold lower production cost, superiority to an expert
model-major PyHMMER program, a new scoring kernel, full InterProScan replacement,
GPU acceleration or clinical validation. PyHMMER already documents model-major
execution; HMMForge's measured implementation adds a batch executor, statistical
reconstruction, compact result handling and reproducible verification.

The next adoption gate is a full, versioned model catalogue and representative
novel-protein collection, with an optimized model-major competitor and native
profiling. The remaining kernel work should follow those measurements rather
than assume a language rewrite will be faster.

## Raw evidence

`../evidence/current-ci.json` preserves every measured run, its provenance and
both native comparisons in one permanent bundle. Repeated identical provenance
has been factored out without changing numeric values. Individual unmodified
reports and JUnit results are also included in the standalone source archive:

- `../evidence/a2-ci-synthetic.json`
- `../evidence/a2-ci-biological.json`
- `../evidence/a2-ci-native-synthetic.json`
- `../evidence/a2-ci-native-biological.json`
- `../evidence/a2-ci-tests.xml`

The standalone source archive also retains the earlier prototype and local
biological reports for transparency. Those ran on different versions and/or
machines; they must not be used to attribute the timing difference to one code
change. The earlier local biological benchmark was approximately 1.16x, while
the initial prototype CI synthetic benchmark was approximately 1.65x.
