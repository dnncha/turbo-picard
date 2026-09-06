# Changelog

## 0.1.0a3 — 6 September 2026

Adds a fully resident, direct PyHMMER model-major baseline with independent
result extraction and a three-engine, fresh-process study. Studies retain all
worker outputs and failure evidence, balance order, compare cross-repeat parity,
and report phase timings, CPU time, peak memory and machine limits.

Adds fixed-release full-catalogue acquisition and locks, deterministic
result-independent protein subsampling, GA/TC/NC support in the native HMMER
comparator, and explicit native-profile success/failure reports. Hosted CI builds
source and wheel distributions and contains a full-model-catalogue smoke study.

The underlying numerical kernels and annotation engine are unchanged from a2.
This release is research tooling, not a new production speedup claim.

## 0.1.0a2

Prepared-profile reuse, compact result extraction, 42 regression tests and
small-fixture native-HMMER differential checks. Historical results remain in
`docs/RESULTS.md` and `evidence/current-ci.json`.
