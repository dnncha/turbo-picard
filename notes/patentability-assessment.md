# turbo-picard patentability assessment

Date: 2026-06-06

This note is a working engineering assessment, not legal advice.

## Bottom line

`turbo-picard` clearly has commercially meaningful speed and memory wins, but
the current public repo does not yet show a broad set of obviously patentable
inventions.

Most of the visible gains currently read as a mix of:

- native Rust implementation instead of the JVM;
- direct HTSlib-backed BAM/CRAM I/O;
- streaming instead of materializing entire files where possible;
- reduced allocation pressure in metrics code;
- explicit fallback to upstream Picard outside native coverage.

Those are good product decisions. They are not, by themselves, strong utility
patent material unless they rest on specific technical methods that are novel
and non-obvious over Picard, HTSlib/samtools, sambamba, and related prior art.

## Strongest candidate in the current code

### 1. `MarkDuplicates` ordered no-duplicate fast path

The strongest current candidate is the single-BAM no-duplicate fast path in
[crates/turbo-picard-markdup/src/lib.rs](/Users/donncha/Documents/GitHub/turbo-picard/crates/turbo-picard-markdup/src/lib.rs).

Relevant implementation points:

- `try_run_single_bam_no_duplicate_fast_path(...)` starts at roughly line 448.
- It applies only when the request shape allows a non-rewriting or minimal
  rewriting path.
- It tracks pair ordering with `last_pair_key`.
- It tracks a small amount of unresolved pair state with
  `adjacent_pending_pair` and `pending_pairs`.
- It detects whether duplicate keys are globally unique or whether execution
  must fall back to the general duplicate-grouping algorithm.
- On success it can copy the input BAM directly instead of rewriting records if
  `can_copy_input_without_rewrite(...)` holds.

Why this is interesting:

- It is not just "Rust is faster."
- It is a specific decision procedure for proving that a BAM can bypass the
  heavy duplicate-marking machinery.
- The saved benchmark evidence ties this path to the strongest observed memory
  drop: about `1.2 GB` RSS in Picard vs about `8.7 MB` in `turbo-picard` for
  the high-confidence `MarkDuplicates` smoke target in
  [benchmarks/README.md](/Users/donncha/Documents/GitHub/turbo-picard/benchmarks/README.md).

Why it is not patent-ready yet:

- The repo does not clearly state how this differs from prior duplicate-marking
  optimizations in other tooling.
- The implementation is visible, but the invention boundary is not articulated
  as claims.
- It is not yet shown that the decision procedure itself, rather than simple
  engineering reduction, is novel.

## Weaker candidates

### 2. Sorted-input stream fast paths

Documented in [docs/performance.rst](/Users/donncha/Documents/GitHub/turbo-picard/docs/performance.rst):

- `SortSam` streams BAM/CRAM without loading the full input when the header sort
  order already matches.
- `MergeSamFiles` uses a similar streaming merge decision when inputs are
  already sorted.

Assessment:

- Useful and product-relevant.
- Likely too close to ordinary streaming optimization unless there is a more
  specific invariant-preserving algorithm hidden underneath.

### 3. Metrics accumulation allocation reductions

Documented in [docs/performance.rst](/Users/donncha/Documents/GitHub/turbo-picard/docs/performance.rst):

- per-cycle buffers resize once per read rather than per base;
- quality histograms use fixed `[u64; 256]` arrays;
- optional tags are scanned in one pass instead of building a tag vector.

Assessment:

- Good performance engineering.
- Very unlikely to be strong patent material in their current form.

### 4. Fallback-preserving native/Picard dispatch

Visible in [crates/turbo-picard-cli/src/lib.rs](/Users/donncha/Documents/GitHub/turbo-picard/crates/turbo-picard-cli/src/lib.rs):

- native execution first;
- explicit delegation to upstream Picard for unsupported surfaces;
- guardrails around JVM-style leading args and unsupported native errors.

Assessment:

- Operationally valuable.
- Probably weak for utility patent claims unless tied to a very specific
  workflow-auditability mechanism that is new over existing wrapper systems.

### 5. Accelerator policy preflight

`AccelerationStatus` and `gpu-required` policy are useful operational features.

Assessment:

- Not a meaningful patent candidate in current form.
- Mostly product hardening.

## What is publicly disclosed already

The following are already public:

- repo source code;
- README;
- benchmark documentation and evidence;
- JOSS paper draft;
- Read the Docs site.

That means some invention detail may already have been publicly disclosed.

Official USPTO references reviewed:

- Provisional application basics:
  [USPTO provisional application page](https://www.uspto.gov/patents-getting-started/patent-basics/types-patent-applications/provisional-application-patent)
- U.S. one-year grace period / first-inventor-to-file:
  [USPTO AIA patents examination page](https://www.uspto.gov/patent/laws-and-regulations/america-invents-act-aia/patents-examination)
- International filing caution:
  [USPTO international protection page](https://www.uspto.gov/patents/basics/international-protection/filing-patents-abroad)

Practical consequence:

- In the U.S., inventor-originated public disclosure can leave a one-year grace
  period.
- Outside the U.S., public disclosure before filing can damage rights much more
  severely.
- Anything newly invented should be documented privately before more detail is
  published.

## What would make this patentable in practice

The current best shot is to define one or more concrete inventions around the
`MarkDuplicates` fast path, for example:

1. A method for deciding, during a single forward scan of a coordinate-sorted
   BAM, that all duplicate candidate keys are unique and therefore that
   duplicate marking can be replaced with direct output or direct file copy.
2. A bounded-state pair-resolution method that combines adjacent-pair handling,
   deferred same-name matching, and ordered duplicate-key monotonicity checks to
   avoid general duplicate grouping unless a violation occurs.
3. A conditional no-rewrite output method that preserves Picard-compatible
   `MarkDuplicates` behavior while replacing full duplicate processing with
   sidecar-only output generation when rewrite-triggering options are absent.

These are not legal claims. They are the invention themes most worth testing.

## Evidence gaps before talking to counsel

We still need:

- a prior-art comparison against Picard, sambamba, samtools/htslib, GATK, and
  any published duplicate-marking acceleration work;
- a diagram of the fast-path decision procedure and fallback boundary;
- proof that the ordered uniqueness test is not already an obvious or published
  optimization;
- commit/date evidence showing when the fast path was first implemented;
- clear inventor attribution for each candidate invention;
- a private write-up that is fuller than what is already public.

## Recommended next actions

1. Treat all future invention-specific notes as private until patent counsel has
   reviewed them.
2. Write a proper invention disclosure around the `MarkDuplicates` fast path:
   algorithm, invariants, failure conditions, why Picard needs the heavier
   general path, and why this path is safe.
3. Run a prior-art sweep focused on duplicate-marking acceleration and sorted
   uniqueness detection.
4. Consider filing a provisional quickly if counsel agrees there is a real
   claimable method and the public disclosure clock matters.
5. Keep public messaging focused on validated product outcomes, not on detailed
   implementation novelty, until the IP decision is made.

## Current conclusion

Today the answer is:

- yes, there is at least one potentially claimable technical direction in the
  codebase;
- no, the repo as it stands does not prove that we already "have patentable
  innovations" in a filing-ready sense;
- the `MarkDuplicates` no-duplicate fast path is the first place to press.
