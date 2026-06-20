# Algorithmic Overhaul Phase 4 Notes

Date: 2026-06-20
Branch: `perf/algorithmic-overhaul`

## Scope

Phase 4 targets `MarkDuplicates` memory use and duplicate-decision structure.
The first slice removes the speculative single-BAM no-duplicate fast path. That
path could stream or copy a near-complete output, discover a duplicate or
out-of-order duplicate key near EOF, delete the output, and restart through the
full in-memory engine.

This commit does not yet implement the compact candidate pipeline. The current
general engine still materializes full `bam::Record` values, parallel metadata
vectors, and duplicate-group maps for the input.

## Implemented

- Removed the late-fallback no-duplicate fast path and its private compact-key
  probe helpers.
- `MarkDuplicates` now enters the single general engine path for HTS container
  inputs, avoiding speculative output files that may be discarded.
- The no-duplicate test now verifies observable behavior, record flags, and
  metrics rather than byte-for-byte copying from the removed optimization.
- Added an explicit `DuplicateCandidate` extraction layer for eligible BAM
  records. Duplicate grouping, representative scoring, qname identity, barcode
  lookup, unclipped five-prime positions, and pair orientation now use cached
  candidate fields instead of repeatedly deriving those values from retained
  `bam::Record` objects.
- Pair and fragment duplicate groups now store candidate indices and map back to
  full records only when applying flags, DS/DI tags, optical duplicate tags, or
  output policies.
- Qnames and BAM barcode values are interned once during candidate extraction.
  Candidate records and BAM duplicate keys now carry compact integer IDs for
  those identities instead of repeated byte-vector copies.
- Optical duplicate detection now tracks seen optical read names with an
  interned-ID hash set, avoiding linear membership scans while preserving the
  existing unique-read-name metric and per-record SQ tagging behavior.
- Duplicate flags, optical duplicate state, and DS/DI duplicate-set tags now
  flow through an ordinal-indexed `RecordDecision` stream. Full BAM records are
  mutated only in the final output application pass.
- Duplicate metric updates now use the compact candidate `library_id` directly,
  eliminating the previous full-record-length `record_libraries` side vector.
- Pair and fragment duplicate grouping now emit compact `(duplicate key,
  candidate index)` rows, stable-sort them by `BamDuplicateKey`, and scan the
  sorted rows into groups. This removes the duplicate-key hash-grouping step and
  matches the shape needed for a later external fixed-width key sorter.
- Optical tile/x/y coordinates are now parsed once during BAM candidate
  extraction and stored on `DuplicateCandidate`; optical duplicate detection no
  longer reparses qnames while scanning duplicate groups.
- Pair collation now consumes adjacent qname runs before using the displaced
  qname map, preserving the previous map-only pairing order while avoiding hash
  lookups for queryname-grouped candidates.
- `MAX_RECORDS_IN_RAM` is now retained in `MarkDuplicatesConfig` and caps the
  displaced-pair cache. When unresolved distant mates exceed that cap, the
  native engine falls back to compact qname-id collation instead of silently
  growing the cache without bound.
- `TMP_DIR` is now retained in `MarkDuplicatesConfig` and used by the native
  MarkDuplicates duplicate-key sorter.
- Pair and fragment duplicate key rows now sort through the shared external
  sorter with compact binary duplicate keys and candidate-index payloads. Forced
  one-record runs spill to `TMP_DIR`, merge deterministically, and clean up temp
  files after sorting.
- Sorted pair and fragment duplicate-key rows are now streamed from the external
  sorter into duplicate-group scanners instead of being collected back into a
  full sorted row vector before grouping.
- Pair and fragment duplicate-group decisions are now applied as each sorted
  group is completed. The production path no longer retains the complete list of
  duplicate groups before updating the ordinal-indexed decision stream.
- Duplicate-group scans now compute unique read-name counts, paired set size,
  and duplicate representative selection in one qname-score pass per group
  rather than rebuilding separate maps and rescanning qnames for each metric.
- Single-input BAM/CRAM output now uses a sequential reread pass driven by the
  ordinal-indexed decision stream. The first pass retains compact candidates but
  no longer holds every full `bam::Record` just to apply final flags and tags.
- Distant-mate qname fallback collation now sorts compact `(qname_id,
  candidate_index)` rows through the shared external sorter. Very displaced
  mate workloads no longer require an in-memory compact qname sort once the
  bounded pending-mate cache overflows.

## Tests

Passing:

```bash
cargo fmt --check
cargo test -p turbo-picard-markdup
cargo test -p turbo-picard-cli markduplicates -- --nocapture
cargo test --workspace
python3 tools/verify_command_matrix.py
cargo build --release -p turbo-picard-cli --bin picard --bin turbo-picard
bash tools/verify_basic_picard_parity.sh
```

Additional adversarial MarkDuplicates coverage now includes duplicate pairs
first appearing at EOF and large duplicate-pair families forced through low
`MAX_RECORDS_IN_RAM` temp-run paths.

Blocked or not clean:

- `bash tools/verify_markdup_cram_parity.sh` could not start because `samtools`
  was not installed in the local environment.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` still
  fails in the existing `turbo-picard-cli` lint backlog.

Remaining Phase 4 work:

- spill compact duplicate candidates instead of retaining every full record;
- extend the sequential reread/output pass to multi-input MarkDuplicates output
  once its merge-order application can be driven by compact decisions;
- add more adversarial tests for distant/missing mates, barcodes, optical
  duplicates, multi-input sorting, and randomized equivalence.
