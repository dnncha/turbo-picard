# Invention disclosure draft: ordered no-duplicate fast path for Picard-compatible MarkDuplicates

Date: 2026-06-06
Status: private working draft
Scope: engineering disclosure for counsel and prior-art review

This note is not legal advice and is not a patent application.

## Working title

Ordered no-duplicate detection and direct-output bypass for Picard-compatible
duplicate marking on coordinate-sorted BAM inputs.

## Problem

Upstream Picard `MarkDuplicates` is built to handle the fully general duplicate
marking problem. In common production workflows, that generality is expensive:

- duplicate candidates may be assumed even when a dataset contains no actual
  duplicates;
- duplicate grouping machinery can consume substantial memory;
- output may be fully rewritten even when no duplicate flags or tags need to
  change;
- the pipeline pays the full JVM and general-algorithm cost to discover that
  the input was already effectively unique.

The concrete product impact is visible in the checked benchmark evidence for a
no-duplicate, coordinate-monotonic paired-end BAM:

- Picard 3.4.0 median wall time: `2.595 s`
- `turbo-picard` median wall time: `0.127 s`
- Picard 3.4.0 median RSS: `~1.2 GB`
- `turbo-picard` median RSS: `~8.7 MB`

Evidence source:
[benchmarks/README.md](/Users/donncha/Documents/GitHub/turbo-picard/benchmarks/README.md)

## Invention summary

The candidate invention is a decision procedure and execution path that:

1. scans a single coordinate-sorted BAM forward once;
2. constructs bounded duplicate-candidate state instead of full duplicate
   groups;
3. derives an ordered duplicate key for paired candidates;
4. proves uniqueness or detects a violation using only the ordered key stream,
   same-name handling, and limited pending-pair state;
5. falls back to the general duplicate-marking algorithm immediately when the
   proof conditions are violated; and
6. on success, emits Picard-compatible metrics and optionally copies the input
   BAM directly to the output BAM without per-record duplicate rewriting.

The practical claim is not "duplicate marking in Rust." The practical claim is
"detecting when duplicate marking can be safely replaced by a bounded-state
proof of uniqueness plus direct output behavior."

## Current implementation evidence

Primary implementation:
[crates/turbo-picard-markdup/src/lib.rs](/Users/donncha/Documents/GitHub/turbo-picard/crates/turbo-picard-markdup/src/lib.rs)

Key entry point:

- `try_run_single_bam_no_duplicate_fast_path(...)` at lines 448 onward

Key supporting routines:

- `fast_pair_key_requires_fallback(...)`
- `ordered_duplicate_key_state(...)`
- `duplicate_key_seen_with_different_name(...)`
- `fast_pair_duplicate_key(...)`
- `fast_single_duplicate_key(...)`
- `can_copy_input_without_rewrite(...)`

## Preconditions for the fast path

The current implementation enables the fast path only when:

- there is exactly one BAM input;
- duplicate-set member tagging is not requested;
- the input is suitable for the native HTS container path;
- the stream remains consistent with the ordered uniqueness proof;
- no pre-existing duplicate flags are encountered.

Direct input-copy behavior is further constrained by
`can_copy_input_without_rewrite(...)`, which currently requires:

- one input;
- no per-read PG tag insertion;
- no DT tag clearing;
- no compression-level override;
- BAM input and BAM output;
- matching BAM output format;
- input and output paths not identical.

## Algorithm sketch

### A. Initialize bounded working state

The fast path initializes:

- `seen_single_keys`: map from single-fragment duplicate key to first observed
  read name;
- `last_pair_key`: last ordered paired duplicate key and read name;
- `adjacent_pending_pair`: one unresolved adjacent pair candidate;
- `pending_pairs`: map from read name to one unresolved mate record;
- lightweight metrics summary state;
- either a temporary BAM writer or a later direct-copy plan.

This state is intentionally bounded relative to the general grouping problem.

### B. Single forward scan

For each BAM record in order:

- if the record is already marked duplicate, set `should_fallback`;
- if unmapped, secondary, or supplementary, copy/emit it without duplicate work
  and update summary counters;
- otherwise classify it as pair candidate or fragment candidate.

### C. Pair handling

For pair candidates:

- convert the record into a compact `FastPairRecord`;
- resolve same-name mates either from `adjacent_pending_pair` or
  `pending_pairs`;
- when both mates are available, derive a canonical paired duplicate key using
  normalized left/right genomic ordering plus library and barcode context;
- compare the derived key against the monotone ordered stream through
  `ordered_duplicate_key_state(...)`.

If the ordered key stream is strictly increasing, the pair is treated as a
unique duplicate set of size one.

If the new key:

- equals a prior key with a different read name, the stream contains a true
  duplicate candidate and the fast path aborts to fallback;
- equals a prior key with the same read name, or is less than a prior key, the
  ordering assumptions are broken and the fast path aborts to fallback.

### D. Fragment handling

For non-pair candidates:

- derive a fragment duplicate key;
- consult `seen_single_keys`;
- if the key has already been seen with a different read name, abort to
  fallback;
- otherwise record the first seen name for that key.

### E. Deferred cleanup

After the scan:

- unresolved adjacent pair state is rechecked as a single-fragment uniqueness
  case;
- remaining pending mates are rechecked as single-fragment uniqueness cases.

Any conflict here also triggers fallback.

### F. Success behavior

If no fallback trigger occurred:

- remove any stale destination output path;
- either copy the source BAM directly to the output BAM or rename the temporary
  output;
- write Picard-compatible metrics sidecars;
- optionally write MD5 and BAI sidecars.

## Fallback boundary

This design is conservative on purpose. It does not try to force all inputs
through the optimized path. It aborts to the general algorithm when:

- an incoming record is already marked duplicate;
- a repeated paired duplicate key is seen under a different read name;
- paired-key order is non-monotonic;
- single-fragment duplicate keys repeat across different read names;
- request options imply rewriting or extended duplicate annotation.

That boundary matters because the invention candidate is not "always faster
duplicate marking." It is "bounded-state proof of a no-duplicate case, with
immediate handoff to the general path when proof conditions fail."

## Why this may be novel

The likely novelty hooks are:

- using ordered duplicate-key monotonicity as a proof of uniqueness for
  coordinate-sorted paired BAM duplicate candidates;
- combining that proof with bounded same-name pair resolution rather than full
  duplicate-group construction;
- using the proof result to skip duplicate-flag rewriting entirely and, in a
  restricted option set, copy the original BAM directly while still producing
  Picard-compatible metrics and sidecars;
- making the fast path self-invalidating, so the same command transparently
  returns to the general algorithm whenever the ordered uniqueness assumptions
  fail.

## Why this may still be rejected as obvious

Likely obviousness attacks:

- "This is just a standard streaming optimization."
- "This is just checking for duplicates while scanning sorted data."
- "This is just a fast path for a zero-duplicate case."
- "Direct copy when no mutations are needed is routine."

The patentability question is whether the specific proof method and fallback
boundary are materially different from what a skilled bioinformatics engineer
would consider an obvious implementation choice.

## Claim themes to explore with counsel

These are invention themes, not finished claims.

1. A method of processing a coordinate-sorted BAM for Picard-compatible
   duplicate marking by generating canonical duplicate keys during a single
   forward pass and determining that duplicate marking is unnecessary when the
   key stream remains strictly ordered and unique under bounded pair-resolution
   state.
2. The method above where paired reads are resolved using an adjacent-pair slot
   plus a deferred same-name table, without constructing full duplicate groups
   unless an ordering or uniqueness violation occurs.
3. The method above where, after uniqueness is established, the system emits
   metrics and sidecars while copying the original BAM as output without
   duplicate-record rewriting.
4. A hybrid duplicate-marking engine that automatically falls back to a general
   duplicate-grouping algorithm when ordered-key proof conditions are violated.

## Prior-art review targets

Before speaking confidently about patentability, review at least:

- Picard `MarkDuplicates` implementation details and any Broad patents or
  publications around duplicate marking;
- sambamba duplicate marking implementation and docs;
- samtools/htslib duplicate-related tooling and any sorting/indexing shortcuts;
- GATK or Broad workflow notes describing duplicate-free or low-duplicate
  handling;
- academic papers or patents on duplicate detection in coordinate-sorted read
  streams;
- patents on no-op/bypass detection for file-processing pipelines;
- patents on conditional direct-copy output for genomics file transforms.

## Questions counsel will ask

- What exactly is new over a competent streaming duplicate checker?
- Is the ordered-key uniqueness proof described anywhere else?
- When was this implemented?
- Who contributed to the idea?
- Was any of the enabling detail publicly disclosed before filing?
- Can the invention be claimed broadly enough to matter if competitors change
  implementation details?

## Evidence to collect next

1. First commit introducing `try_run_single_bam_no_duplicate_fast_path(...)`.
2. Benchmarks showing performance with and without the fast path on:
   no-duplicate monotonic BAMs, duplicate-heavy BAMs, and non-monotonic BAMs.
3. A flowchart of the state machine:
   adjacent pending, deferred pending, ordered-key compare, fragment-key check,
   fallback triggers, direct-copy success.
4. A side-by-side explanation of what Picard does on the same case and why it
   still pays the general algorithm cost.
5. Inventor attribution notes.

## Current recommendation

This is worth treating as a real invention candidate.

It is not yet ready for a patent filing without:

- prior-art review;
- claim shaping;
- inventor/date evidence;
- a decision on whether the already public repo disclosure is acceptable given
  target jurisdictions.
