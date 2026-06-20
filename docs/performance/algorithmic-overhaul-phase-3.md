# Algorithmic Overhaul Phase 3 Notes

Date: 2026-06-20
Branch: `perf/algorithmic-overhaul`

## Scope

Phase 3 targets `CollectWgsMetrics` memory use. The first slice replaces
full-contig interval masks with compact merged half-open interval ranges. This
removes one whole-contig allocation from interval-restricted runs and prepares
the code for a sliding depth frontier.

This is not the full sliding-depth implementation yet. The current accumulator
still allocates `active_depths` to the full active contig length after the first
record on that contig.

## Implemented

- `CollectWgsMetrics INTERVALS` now stores sorted merged ranges per contig
  instead of `Vec<bool>` masks sized to every reference base.
- Contigs without intervals remain excluded when interval restriction is active.
- `STOP_AFTER` clips included ranges directly rather than materializing a full
  boolean mask.
- Existing interval territory and `STOP_AFTER` output semantics are preserved.

## Tests

Passing:

```bash
cargo fmt --check
cargo test -p turbo-picard-cli collectwgsmetrics -- --nocapture
cargo test -p turbo-picard-cli wgs_interval_ranges -- --nocapture
```

Remaining Phase 3 work:

- replace `active_depths: Vec<u16>` with a sliding frontier/ring buffer;
- finalize loci as the coordinate frontier advances;
- bound the mate-overlap cache by expiry coordinate;
- add long-contig sparse-read memory assertions and randomized equivalence
  tests against the previous implementation.
