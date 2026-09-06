# Changelog

## 0.1.13 — 2026-09-06

### Native execution and sorting

- Use a heap for BAM external-merge selection instead of scanning every active
  run. Preserve the existing comparator and cross-run tie order.
- Track owned spill files through intermediate merges and failures; reject
  truncated spill record headers and account for sorting metadata in memory
  budgets.
- Reduce optical-coordinate parser allocations and read-name cloning; protect
  coordinate-distance and neighbouring-grid arithmetic against overflow.
- Add `TURBO_PICARD_REQUIRE_NATIVE` to prohibit both explicit and automatically
  discovered Java fallback during native-only evaluation.

### Workflow and agent interfaces

- Add command-scoped `capabilities --json --command <Command>` discovery.
- Provide executable argument arrays and command-specific output roles in trial
  JSON, with explicit syntax, option-support and input-inspection boundaries.
- Stream or externally sort alignment comparisons without sampling or dropping
  records. Preserve failed evaluation work and refuse to replace existing
  evidence; `--discard-work` cleans only a successful current run.
- Reject NaN-versus-number metric mismatches and handle missing optional tags
  deterministically in duplicate-semantic comparisons.

### Documentation and evidence

- Clarify the README, landing page, agent guidance and real-data evaluation
  instructions; improve keyboard, mobile and reduced-motion behaviour.
- Retain absolute timing, repeat-count and workload metadata in benchmark JSON;
  correct the even-count suite median from 99.56x to 99.51x using unchanged
  August 14 saved logs. This release does not claim a new production-scale
  benchmark campaign or universal superiority over other tools.
- Include the reproducible validation-helper memory benchmark (one million
  synthetic SAM records); its results do not measure native MarkDuplicates.
- Fetch tags for public adoption audits, discover new Python regressions in CI,
  and fix strict Sphinx heading validation.

Bioconda acceptance and version-specific archival DOI assignment are separate
from the GitHub/PyPI/container release. Cite the exact software version and
input-specific evidence used.


## 0.1.12 — 2026-08-30

This is the release source for `v0.1.12`. Tag, package, container, and
downstream-provider verification remain separate release gates.

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

- The current exact-commit three-repeat 32-command local suite is 32/32
  parity-pass, with a geometric-mean speedup of 84.52x, a 22.88x floor on
  `SetNmMdAndUqTags`, and a 272.12x maximum on `NormalizeFasta`.
- The refreshed 1M synthetic and reference-backed CRAM MarkDuplicates
  guardrails pass exact parity. These are fixture-level evidence only; they do
  not establish 30x WGS production readiness, universal replacement, or
  independent reproduction.
- Keep upstream Picard available for unsupported or not-yet-reviewed workflow
  shapes, and compare the exact inputs, outputs, sidecars, metrics, failures,
  runtime, and memory behavior before rollout.

The matching release tag, production-scale evidence, independent reproduction,
PyPI/container publication, and Bioconda submission remain separate gates.
