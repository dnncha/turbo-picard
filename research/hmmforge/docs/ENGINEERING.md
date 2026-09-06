# Engineering decisions and release gates

## Current implementation

The product hypothesis is lower cost per finished novel-protein annotation, not
an attractive kernel microbenchmark. Version 0.1.0a2 uses PyHMMER 0.12.3's existing
HMMER kernels. It does not claim a new search algorithm.

The first implemented improvements are model-major execution, once-per-run
profile preparation, bounded sequence batches, compact per-model result
extraction, and reconstruction of per-protein scan statistics. Both competing
engines receive the same input, thresholds, CPUs and output schema. Neither
caches prior results.

## Statistical contract

For an amino-acid protein query and a database with M models:

- Whole-hit E-value = whole-hit P-value * M.
- Independent domain E-value = domain P-value * M.
- Conditional domain E-value = domain P-value * R, where R is the number of
  reported MODELS for that protein, not the number of matching proteins for a
  model and not the size of the sequence batch.

Candidate searches explicitly set Z=M and domZ=1 while collecting a broad domain
set. They preserve HMMER's reported-domain suppression of duplicate alignments,
then restore conditional E-values and threshold domains after R is known.
Model-specific GA/TC/NC decisions remain with upstream kernels. Missing cutoffs
are errors; they are never replaced with permissive defaults.

The reference path uses actual PyHMMER hmmscan E-values. Tests must cover more
than one reported model per protein, multidomain sequences, no-hit sequences,
ambiguous amino acids, threshold boundaries, repeated names, single/multithread
runs, batch size changes and repeated reuse of prepared profiles.

## What the current validation does not establish

The synthetic regression suite is not Pfam/InterPro sensitivity validation.
Small biological test data do not establish metagenomic-scale performance.
Native table comparison does not validate full alignment strings, inclusion
flags, all HMMER CLI options, every platform or every threshold configuration.
The package is not clinical software and does not interpret pathogenicity.

Floating-point tolerances are explicit: 1e-6 relative; 1e-5 absolute for scores
and bias; ZERO absolute floor for P/E-values. A permissive absolute floor would
silently equate 1e-80 with 1e-30. Native text comparisons instead use the exact
printed precision. Never relax tolerances simply to turn a red test green.

## Required next experiments

1. Run a versioned full profile catalogue against at least 100,000 representative
   novel proteins, plus curated low-complexity, fragmentary, multidomain and
   near-threshold cases. Pin source releases/hashes and obtain appropriate data
   permissions. Keep private data out of public CI.
2. Compare the complete annotation job against current native HMMER, optimised
   PyHMMER scan AND an expert-written model-major PyHMMER pipeline. Report all
   variants, including losses. HMMForge is not novel merely because scan is slower.
3. Record model preparation, parse/digitise, MSV, Viterbi, Forward/Backward,
   posterior/domain definition, extraction and output costs. Use native sampling
   (perf on Linux), not just Python cProfile. Retain raw counters/traces.
4. Measure one, two, eight and larger CPU budgets, profile-library size, batch
   sizes, peak RSS, high-hit-density memory and worker utilisation. Distinguish
   core-seconds from reserved-node seconds and process-cold from storage-cold.

## Kernel-development path

Only after native profiling identifies a dominant kernel should a compiled
acceleration module be added. A Rust/C++ layer should own explicit aligned
buffers and batch descriptors; Python remains the packaging/control surface.
SIMD dispatch must support the baseline architecture and runtime feature checks.
Do not replace an already-vectorised C routine with scalar Rust and call it fast.

A GPU experiment must include resident-model memory, transfers, packing, length
bucketing and all surviving downstream CPU work. A saturated or numerically
uncertain computation falls back to the canonical kernel. A threshold fallback
window needs an error bound or empirical qualification; "looks close" is not a
sensitivity guarantee. Keep the unchanged scientific models and original filter
settings. No lossy clustering, relaxed thresholds or shorter optimisation runs
masquerading as equivalent computation.

## Adoption gate

Recruit three design partners with an expensive recurring annotation workload.
Collect representative data, actual resource costs and the outputs they require.
A new release must preserve the validated result contract and demonstrate a
material full-job saving. The commercial target is at least 5x lower total cost
or a decisive RAM-class reduction on the chosen workload; it is not an achieved
result. If the headroom against expert model-major code is small, stop expanding
the wrapper and reconsider the bottleneck rather than adding marketing claims.

Before broad distribution: native differential CI on Linux x86_64 and Apple
Silicon, larger biological regression corpus, complete JSON schema, malformed
input/failure testing, wheel/container reproducibility and supported workflow
modules. Add domtblout only when every required field has a defined source;
never fabricate alignment accuracy to fill a column.

## Project boundary

This is an independent package temporarily parked in Turbo Picard's isolated
research branch because repository creation was not exposed by the connected
GitHub tools. It is not a new Turbo Picard command and should not be merged into
its production branch. Extract it into its own repository before release.
