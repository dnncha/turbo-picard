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

The first command integrations are `SortVcf`, `MergeVcfs`, sorted
`LiftoverVcf` lifted output, and `SortSam`. VCF and SAM-text adapters feed
compact ordering keys and raw record-line payloads through the shared sorter.
Existing header validation is preserved. Full streaming VCF parsing is still
deferred to the later VCF streaming phase.

`SortSam` BAM/CRAM input uses a safe temporary-BAM run-file adapter rather than
opaque sorter payloads, because rust-htslib does not expose raw BAM record bytes
as a public safe API. The adapter reads input once, detects monotonicity during
that pass, stable-sorts bounded chunks with an input ordinal, writes temporary
BAM runs under `TMP_DIR`, concatenates runs for monotonic input, and performs
bounded-fan-in k-way merge for unsorted input.

## Implemented

- record-count spill limit;
- estimated-byte spill limit;
- stable duplicate-key ordering using original ordinal;
- deterministic temporary run naming with collision-safe `create_new`;
- drop-time cleanup for partial runs;
- bounded merge fan-in with intermediate merge passes;
- metrics for spills, resident records, estimated bytes, run count, and bytes written.
- safe temporary-BAM run sorting for `SortSam` BAM/CRAM paths, bounded by
  `MAX_RECORDS_IN_RAM`.

## Tests

Covered in `crates/turbo-picard-core/src/external_sort.rs`:

- empty input;
- forced one-record runs;
- duplicate keys with stable ordering;
- multiple spills;
- bounded fan-in merge passes;
- byte-limit spill instrumentation;
- cleanup after dropping a sorter with partial runs.

Additional `SortVcf`, `MergeVcfs`, `LiftoverVcf`, and `SortSam` CLI coverage
forces one-record external runs with `MAX_RECORDS_IN_RAM=1`, uses custom
`TMP_DIR` values, verifies stable/sorted output ordering, and asserts temporary
run cleanup. The `SortSam` BAM test includes an inversion at the final record and
duplicate equal sort keys to check stable tie-breaking.

Passing:

```bash
cargo fmt --check
cargo test -p turbo-picard-core external_sort -- --nocapture
cargo test -p turbo-picard-cli sortvcf -- --nocapture
cargo test -p turbo-picard-cli mergevcfs -- --nocapture
cargo test -p turbo-picard-cli liftovervcf -- --nocapture
cargo test -p turbo-picard-cli sortsam -- --nocapture
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
bash tools/verify_basic_mergevcfs_parity.sh
bash tools/verify_basic_liftovervcf_parity.sh
bash tools/verify_basic_sortsam_parity.sh
```

This found `mamba`, but the configured Picard prefix
`.conda-turbo-picard` does not exist in this worktree.

## Local VCF Smoke

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

Release command shape:

```bash
./target/release/picard MergeVcfs \
  I=first.vcf \
  I=second.vcf \
  O=merged.vcf \
  MAX_RECORDS_IN_RAM=1 \
  TMP_DIR=merge-tmp \
  QUIET=true \
  VALIDATION_STRINGENCY=SILENT
```

Synthetic input: 5,000 reverse-position records split across two input VCFs.

Result:

| Records | MAX_RECORDS_IN_RAM | Wall seconds | Temp cleanup |
| ---: | ---: | ---: | --- |
| 5000 | 1 | 1.788471 | PASS |

`LiftoverVcf` has forced-run CLI test coverage for lifted output sorting and
temp cleanup, but no release timing was recorded for this slice.

Release command shape:

```bash
./target/release/picard SortSam \
  I=input.sam \
  O=sorted.sam \
  SO=coordinate \
  MAX_RECORDS_IN_RAM=1 \
  TMP_DIR=sort-tmp \
  QUIET=true
```

Synthetic input: 5,000 reverse-position SAM text records on one contig.

Result:

| Records | MAX_RECORDS_IN_RAM | Wall seconds | Max RSS bytes | Temp cleanup |
| ---: | ---: | ---: | ---: | --- |
| 5000 | 1 | 1.00 | 9,781,248 | PASS |

Release command shape:

```bash
./target/release/picard SortSam \
  I=queryname.bam \
  O=coordinate.sam \
  SO=coordinate \
  MAX_RECORDS_IN_RAM=1 \
  TMP_DIR=sort-tmp \
  QUIET=true
```

Synthetic input: 5,000 reverse-position records converted to queryname-sorted
BAM, then coordinate-sorted from BAM to SAM with forced one-record temporary BAM
runs.

Result:

| Records | MAX_RECORDS_IN_RAM | Wall seconds | Max RSS bytes | Temp cleanup |
| ---: | ---: | ---: | ---: | --- |
| 5000 | 1 | 77.44 | 11,485,184 | PASS |

This is an adversarial bounded-memory stress run, not a normal performance
claim. The one-record cap intentionally forces thousands of tiny BAM files and
merge-pass overhead.
