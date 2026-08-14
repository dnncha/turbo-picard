# Changelog

## 0.1.12 — release candidate

This is a local release candidate. It is not tagged or published yet.

Highlights:

- Expanded the bounded native `MarkDuplicates` path to cover explicit-reference
  CRAM, globally coordinate-ordered multiple inputs, primary and mate-specific
  barcode grouping, optical-family parsing, `REMOVE_SEQUENCING_DUPLICATES`,
  and DS/DI duplicate-set tags.
- Added a record-count-bounded compact plan for small single-BAM/CRAM
  `MarkDuplicates` inputs, while retaining the external plan for larger and
  multi-input shapes.
- Added bounded reference-window slices to `SetNmMdAndUqTags` so ordinary
  in-window CIGAR segments avoid repeated per-base cache checks; oversized and
  window-crossing segments retain the existing fallback.
- Added workflow-owner trial reporting, redacted shareable comparison reports,
  adoption-signal auditing, and fail-closed release and evidence validators.
- Hardened PyPI and container publication checks so artifacts are built and
  published only from the exact version tag.
- Rebuilt the arm64 wheel and source distribution with install, doctor, trial,
  compatibility-shim, real-data, and mate-specific barcode smoke coverage.
- Corrected `CollectAlignmentSummaryMetrics` so no-reference runs do not infer
  mismatch rates from NM/MD tags when Picard leaves those fields at zero.

Evidence boundaries:

- The current three-repeat 32-command local suite is 32/32 parity-pass, with a
  geometric-mean speedup of 81.35x, an 18.93x floor on `FastqToSam`, and a
  264.26x maximum.
- The refreshed 1M synthetic and reference-backed CRAM MarkDuplicates
  guardrails pass exact parity. These are fixture-level evidence only; they do
  not establish 30x WGS production readiness, universal replacement, or
  independent reproduction.
- Keep upstream Picard available for unsupported or not-yet-reviewed workflow
  shapes, and compare the exact inputs, outputs, sidecars, metrics, failures,
  runtime, and memory behavior before rollout.

The matching release tag, production-scale evidence, independent reproduction,
PyPI/container publication, and Bioconda submission remain separate gates.
