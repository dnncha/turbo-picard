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

The first command integration is `SortVcf`, which now feeds compact
dictionary-rank/position keys and raw record-line payloads through the shared
sorter. Existing header validation is preserved. Full streaming VCF parsing is
still deferred to the later VCF streaming phase.

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

Additional `SortVcf` CLI coverage forces one-record external runs with
`MAX_RECORDS_IN_RAM=1`, uses a custom `TMP_DIR`, verifies stable duplicate
position ordering, and asserts temporary run cleanup.

Passing:

```bash
cargo fmt --check
cargo test -p turbo-picard-core external_sort -- --nocapture
cargo test -p turbo-picard-cli sortvcf -- --nocapture
cargo clippy -p turbo-picard-core --all-targets --all-features -- -D warnings
cargo test --workspace
python3 tools/verify_command_matrix.py
cargo build --release -p turbo-picard-cli --bin picard --bin turbo-picard
```

The full workspace clippy gate remains blocked by existing CLI lint debt noted
in the Phase 1 notes.

Blocked:

```bash
bash tools/verify_basic_sortvcf_parity.sh
```

This found `mamba`, but the configured Picard prefix
`.conda-turbo-picard` does not exist in this worktree.

## Local SortVcf Smoke

Release command shape:

```bash
./target/release/picard SortVcf \
  I=input.vcf \
  O=sorted.vcf \
  MAX_RECORDS_IN_RAM=1 \
  TMP_DIR=sort-tmp \
  QUIET=true \
  VALIDATION_STRINGENCY=SILENT
```

Synthetic input: 5,000 reverse-position records across two contigs.

Result:

| Records | MAX_RECORDS_IN_RAM | Wall seconds | Temp cleanup |
| ---: | ---: | ---: | --- |
| 5000 | 1 | 1.584821 | PASS |
