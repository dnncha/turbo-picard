# Algorithmic Overhaul Phase 3 Notes

Date: 2026-06-20
Branch: `perf/algorithmic-overhaul`

## Scope

Phase 3 targets `CollectWgsMetrics` memory use. The current implementation uses
compact merged half-open interval ranges plus a sliding depth frontier. The
global coverage histogram is populated when loci are finalized, rather than
moving every locus between bins on each depth increment.

## Implemented

- `CollectWgsMetrics INTERVALS` now stores sorted merged ranges per contig
  instead of `Vec<bool>` masks sized to every reference base.
- Contigs without intervals remain excluded when interval restriction is active.
- `STOP_AFTER` clips included ranges directly rather than materializing a full
  boolean mask.
- Depths are stored only for the active coordinate span. Sparse skipped regions
  are finalized as zero-depth territory in bulk.
- Before each coordinate-sorted record is processed, loci below the record start
  are finalized because no later record can affect them.
- Coordinate regressions now produce an explicit `not coordinate-sorted` error.
- The mate-overlap cache stores an expiry coordinate and is pruned whenever the
  finalized frontier advances beyond the possible overlap span.
- Existing interval territory, `STOP_AFTER`, coverage cap, base-quality,
  duplicate, mapq, unpaired, overlap, and capped exclusion semantics are
  preserved by the focused tests.

## Tests

Passing:

```bash
cargo fmt --check
cargo test -p turbo-picard-cli wgs_ -- --nocapture
cargo test -p turbo-picard-cli collectwgsmetrics -- --nocapture
cargo test --workspace
python3 tools/verify_command_matrix.py
cargo build --release -p turbo-picard-cli --bin picard --bin turbo-picard
```

Blocked:

```bash
bash tools/verify_basic_collectwgsmetrics_parity.sh
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The parity script found `mamba`, but the configured Picard prefix
`.conda-turbo-picard` does not exist in this worktree. The clippy gate remains
blocked by existing CLI lint debt such as `items_after_test_module`,
`too_many_arguments`, and broad `collapsible_if` findings outside the new WGS
frontier code.

Release command shape:

```bash
./target/release/picard CollectWgsMetrics \
  I=input.sam \
  O=wgs.txt \
  R=ref.fa \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=0 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true
```

Synthetic input: one 4M read near the end of a 1,000,000-base contig. This
exercises bulk zero-depth finalization and a resident depth window proportional
to active read span rather than contig length.

Result:

| Contig bases | Reads | Resident covered span | Wall seconds | User seconds | Sys seconds | Max RSS bytes | Zero-depth bin | One-depth bin |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000,000 | 1 | 4 | 0.52 | 0.00 | 0.00 | 8,716,288 | 999,996 | 4 |

Remaining Phase 3 work and follow-up hardening:

- broaden randomized equivalence tests against the previous implementation;
- add more adversarial mate-overlap cases with absent and distant mates;
- run upstream Picard parity when the local `.conda-turbo-picard` environment is
  available.
