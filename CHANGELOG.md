# Changelog

## Unreleased

## 0.1.18 — 2026-10-31

### Agent integration

- `doctor` prints `fallback_ready`, `fallback_status` and, when delegation is
  unavailable, a `fallback_hint` install command. `explain` reports the same
  state, and `explain --json` adds `fallback_ready`, `fallback_status` and
  `fallback_install_hint`. Readiness requires a resolved fallback command whose
  program is executable; `TURBO_PICARD_REQUIRE_NATIVE` reports
  `disabled-by-require-native`.
- Add the `turbo-picard-full` Bioconda metapackage recipe, which pins
  `turbo-picard`, upstream `picard` 3.4.0 and `openjdk` so one environment
  covers every Picard 3.4.0 command name. It is prepared in the repository and
  not yet submitted to Bioconda.
- Add `turbo-picard mcp`, a Model Context Protocol server on stdio with
  `capabilities`, `explain`, `trial`, `doctor` and `run` tools. The report
  tools return the existing JSON contracts; `run` executes without a shell and
  refuses outputs that name an input or an existing file unless `overwrite` is
  set. `packaging/mcp/server.json` is the prepared MCP Registry listing.
- Add the agent kit under `packaging/agents/`: a Claude Code skill and plugin
  (with the MCP server and a repository marketplace), a Codex `AGENTS.md`
  snippet and a Cursor rule. `tools/test_agent_kit.py` keeps them consistent.
- Add `benchmarks/agent-selection/`, an eval of which tool a coding agent picks
  for ten Picard-shaped prompts with turbo-picard and Picard both installed.
  Claude Sonnet 5.5 chose turbo-picard in 0/20 baseline runs and 18/20 runs
  with the skill and MCP server installed. The MarkDuplicates prompt exposed a
  representative tie-break difference from Picard 3.4.0 (8 of 6,900 reads,
  metrics identical) that predates this work.

### Scientific correctness

- MarkDuplicates now breaks equal-score ties on the tile, x and y parsed from
  the read name before file order, as Picard 3.4.0's `ReadEndsMDComparator`
  does, in the in-memory pair and fragment paths and the external plan. This
  fixes the representative divergence above; the `pair-score-tie-reverse-r1`
  fixture pins Picard 3.4.0 output.

### Engineering

- Add a per-run collector plan to the single-pass metrics pipeline. The plan
  records which collector families are active, and the reader evaluates only
  those gate families, so unused quality, mate and insert-size predicates are
  not computed per record. The unplanned path is retained and tested for
  identical gates.
- Filter paired fragment entries through a lazily allocated 4 MiB bloom filter
  over the fragment keys an unpaired candidate owns. A false positive adds a
  pair to a group that marks nothing, so duplicate decisions are unchanged; a
  regression test pins the retained and dropped cases. A synthetic native-path
  mix (200,000 pair-only read pairs plus 5,000 tainted loci, one host, one
  warm-up and three alternating repetitions) moved from 0.33 s / 157.1 MB to
  0.28 s / 91.4 MB with identical BAM and metrics hashes; at 400,000 pairs it
  moved from 1.22 s / 198.0 MB to 0.79 s / 125.2 MB. This is not a Picard
  comparison or a WGS guarantee. Reproduce with
  ``tools/bench_fragment_filter.py``.
- Record the CLI and core contract with expect tests so a changed default,
  ordering rule, or error message appears as a reviewed diff.
- Harden command obligations: an accelerated matrix entry must name a parity
  script that exists, fails closed, and is not reused by another command.
- Add ``tools/retro-trace.sh`` and document retroactive tail tracing with
  magic-trace for one slow record or batch.
- Strip a Python-version dependency from the evidence comparator: explicit-zone
  read-group DT timestamps normalize on Python versions that reject a colon-less
  offset or a trailing ``Z``.


## 0.1.17 — 2026-10-25

- Reuse CollectHsMetrics interval lookups within alignment blocks, including
  off-target gaps, while preserving metrics and coverage outputs.
- Use shared SAM/BAM/CRAM duplicate selection, validate multi-input dictionaries,
  preserve colliding read-group identities, and check mixed fragment/pair
  candidates before accepting the no-duplicate fast path.
- Preserve sequence yield with absent qualities, paired and unpaired alignment
  metrics/histograms, and noise counts in standalone and combined collectors.
- Verify actual sort order, use references in SAM summary validation, check
  common structural SAM errors, merge VCF field definitions, and reject invalid
  gather ordering before writing output.
- Share Picard sort tie rules across SAM and BAM, classify validation exits by
  errors and warnings, and repair primary/supplementary mate groups natively,
  including missing-primary groups and tandem insert sizes.
- Introduce the independently mutation-tested `markduplicates-v2` comparison
  contract for all scientific SAM fields, typed tags except PG, SQ/RG headers,
  metrics and histograms. Run the adversarial gate against both Picard versions
  in CI with native-only execution and retained failure evidence. Historical
  benchmark evidence keeps its original comparator and measurements.
- Execute the selected Picard version on the BAM fixtures with identical
  arguments, replacing comparisons to stored outputs from different options;
  require exact validation exit categories in parity helpers.

## 0.1.16 — 2026-10-01

## Correctness and bounded memory

- Correct MarkDuplicates numeric histograms: retain single-library ROI when
  optical detection is disabled, omit pooled multi-library ROI, and normalize
  sparse numeric bins according to the declared Picard comparison contract.
- Bound the speculative mate cache to 4,096 records / 4 MiB of owned payload,
  with disk spill for unresolved mates. Validate mate flags and frame read-group
  and read-name identity together.
- Account for actual reserved record-vector capacity in external-sort budgets.
  Spill an oversized singleton immediately. These are buffer budgets, not hard
  process RSS limits.
- Reduce duplicate-processing allocations and skip fragment sorting when no
  mapped primary unpaired candidate exists.
- Include the already-merged CollectHsMetrics interval-lookup allocation
  reduction and streamed binary parity comparisons.

## Measured performance and scientific validation

- Paired synthetic spill fixtures at approximately 600,000 and 1,200,000
  records, one CPU, compression level 1, warm cache, one warm-up plus three
  alternating measured
  repetitions: runtime fell from 2.8091 to 2.2947 seconds (18.3%) and from
  5.44834 to 4.58571 seconds (15.8%) in [PR #39’s comparison run](https://github.com/dnncha/turbo-picard/tree/140fd90848e3f60675244925f77712139cd337aa/benchmarks/review-evidence/2026-10-01/ci-pr39)
  against that run’s recorded baseline. Peak RSS fell from
  287.588 to 251.032 MB (12.7%) and 288.776 to 264.298 MB (8.5%). MB denotes 1,000,000 bytes.
- Eleven bounded whole-command cases pass the Picard 3.4.0 contract for ordered
  mandatory alignment fields, typed tags except PG, SQ/RG headers, numeric
  DuplicationMetrics, and numeric histograms. Explicitly zoned read-group DT
  timestamps compare as exact instants; other fields retain their comparison
  rules. The prior baseline's nine histogram failures remain in the evidence.
- Six core-sort cases retained all 96 output hashes. Final core timings were
  near neutral; an earlier ordered-case regression is retained in the evidence.
- Native competitor measurements include required preparation and final sort.
  FastDup, samtools, and dupblaster were faster on the clean paired fixture;
  their full output contracts differ. Real NA12878 mitochondrial input matched
  the full Picard contract for Turbo Picard. No broad WGS/WES/UMI/cohort or
  downstream variant-calling superiority is claimed.
- Validation covers the Rust and Python tooling suites, Picard parity,
  Linux/macOS packaging, containers, workflow starters, and strict docs builds.

[Retained evidence and raw summaries](https://github.com/dnncha/turbo-picard/tree/03a655fb33df8df6b2e65bafeb5e93a8ea4a2fea/benchmarks/review-evidence/2026-10-01)

Install: `python3 -m pip install turbo-picard==0.1.16`

Container: `ghcr.io/dnncha/turbo-picard:0.1.16`


## 0.1.15 — 2026-09-29

### Correctness

- `CollectHsMetrics`: count base qualities only while a target position is
  below `COVERAGE_CAP`. The previous comparison stayed true after the stored
  depth saturated and could skew `HET_SNP_SENSITIVITY` on deep capture data.

### Memory

- `SortSam` and other BAM external sorts now spill a run before an incoming
  record would push its estimated payload and record-vector allocation past a
  256 MiB default budget, alongside the existing record-count limit. A single
  oversized record spills immediately. This is an approximate run-buffer
  budget, not a hard process RSS bound.
- `CollectHsMetrics` streams `PER_BASE_COVERAGE` rows through a buffered writer
  instead of building the whole sidecar in memory.
- `IntervalListTools` reads input rows incrementally and streams its output.

### Validation

- 419 Rust tests, Picard 3.4.0 parity, nf-core, Snakemake, and Docker CI
  gates pass. `SortSam` queryname and coordinate outputs on the retained
  NA12878 mitochondrial BAM are byte-identical to 0.1.14 with no measured
  time or peak-RSS regression. No speedup is claimed for this release.

## 0.1.14 — 2026-09-25

### Bounded MarkDuplicates correctness and memory

- Keep coordinate-tied and unplaced single-input reads eligible for the native
  external plan while rejecting genuine coordinate reversals.
- Frame read-group and read-name identity together, require complementary
  first/second flags for primary mates, and bypass single-identity shortcuts
  when multiple read groups are present.
- Limit the speculative no-duplicate scan to 100,000 records.
- Reuse first-pass BAM allocations, visit each duplicate family once, and buffer
  compact decision records during replay.

### Validation and measured trade-off

- All nine adversarial synthetic whole-command cases matched Picard 3.4.0 and
  the prior `62ccb0c` checkpoint on three measured repetitions. Comparisons
  covered ordered alignment fields and tags except `PG`, `SQ`/`RG` headers, and
  normalized `DuplicationMetrics` tables. The candidate reported external-plan
  selection in every case.
- On the warm-cache spill fixtures, the previous checkpoint and 0.1.14 measured:

  | Input | Median wall time | Median peak RSS |
  | --- | ---: | ---: |
  | 600k records | 0.77 s → 1.37 s | 254 MiB → 162 MiB |
  | 1.2m records | 1.57 s → 2.52 s | 503 MiB → 163 MiB |

  The same 0.1.14 runs were 3.5× and 2.3× faster than Picard 3.4.0 on those
  fixtures. These are synthetic, warm-cache measurements with one CPU pinned
  on a shared runner; they do not establish WGS/cohort performance or a
  process-wide hard-RSS guarantee. See the [retained run](https://github.com/dnncha/turbo-picard/actions/runs/36176829841)
  for inputs, hashes, commands, raw logs, and the measured repetitions.

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
