# Algorithmic Overhaul Phase 2 Notes

Date: 2026-06-20
Branch: `perf/algorithmic-overhaul`

## Scope

This starts the shared external-sort subsystem in
`turbo_picard_core::external_sort`.

The first committed primitive sorts encoded binary keys with stable ordinal
tie-breaking and opaque payload bytes. Command adapters can encode BAM/SAM/VCF
ordering semantics into keys while the sorter owns bounded memory, temporary
runs, deterministic merging, cleanup, and metrics.

This commit does not yet integrate the sorter into `SortSam`, `MergeSamFiles`,
`FixMateInformation`, `RevertSam`, or VCF commands.

## Implemented

- record-count spill limit;
- estimated-byte spill limit;
- stable duplicate-key ordering using original ordinal;
- deterministic temporary run naming with collision-safe `create_new`;
- drop-time cleanup for partial runs;
- bounded merge fan-in with intermediate merge passes;
- metrics for spills, resident records, estimated bytes, run count, and bytes written.

## Tests

Covered in `crates/turbo-picard-core/src/external_sort.rs`:

- empty input;
- forced one-record runs;
- duplicate keys with stable ordering;
- multiple spills;
- bounded fan-in merge passes;
- byte-limit spill instrumentation;
- cleanup after dropping a sorter with partial runs.

Passing:

```bash
cargo fmt --check
cargo test -p turbo-picard-core external_sort -- --nocapture
cargo clippy -p turbo-picard-core --all-targets --all-features -- -D warnings
cargo test --workspace
```

The full workspace clippy gate remains blocked by existing CLI lint debt noted
in the Phase 1 notes.
