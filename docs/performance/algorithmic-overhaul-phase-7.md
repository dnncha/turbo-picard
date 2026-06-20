# Algorithmic Overhaul Phase 7 Notes

Date: 2026-06-20
Branch: `perf/algorithmic-overhaul`

## Scope

Phase 7 targets streaming VCF and sidecar operations. This slice addresses MD5
sidecar generation across the native CLI helpers and the MarkDuplicates crate.

## Implemented

- MD5 sidecar generation now streams completed outputs through a fixed 64 KiB
  buffer instead of reading the entire output into memory with `fs::read`.
- Sidecar filenames and digest contents are unchanged: native commands still
  write `{output}.md5` after the primary output is complete.
- No new runtime dependency was added; the existing `md5` crate contexts are
  used in each crate.

## Tests

Passing:

```bash
cargo fmt --check
cargo test -p turbo-picard-markdup creates_md5_sidecar -- --nocapture
cargo test -p turbo-picard-cli md5 -- --nocapture
cargo clippy -p turbo-picard-markdup --all-targets --all-features -- -D warnings
```

Blocked or not clean:

- `cargo clippy -p turbo-picard-cli --lib --tests --all-features -- -D warnings`
  still fails in the existing `turbo-picard-cli` lint backlog.

Remaining Phase 7 work:

- stream `GatherVcfs` header/body handling;
- implement sorted `MergeVcfs` k-way streaming where compatible;
- keep `SortVcf`/`LiftoverVcf` on compact external-sort records while reducing
  remaining full-document VCF materialization.
