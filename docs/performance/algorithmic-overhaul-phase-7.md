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
- `GatherVcfs` now validates headers and streams records line-by-line from
  plain or gzip inputs into a same-directory temporary output, then renames it
  only after successful completion. Plain VCF index offsets are accumulated
  incrementally, so the command no longer builds one full output `String`.
- `MergeVcfs` now attempts a k-way streaming merge for sorted compatible inputs,
  retaining only one parsed record per input plus the merge heap. If any input is
  discovered to be unsorted, the same-directory temporary output is discarded and
  the existing shared external-sort path is used unchanged.
- `SortVcf` now scans headers and pushes records into the shared external sorter
  line-by-line from plain or gzip inputs, then emits sorted records directly to a
  same-directory temporary output. Index offsets are accumulated during output
  writes instead of from a fully materialized output string.

## Tests

Passing:

```bash
cargo fmt --check
cargo test -p turbo-picard-markdup creates_md5_sidecar -- --nocapture
cargo test -p turbo-picard-cli md5 -- --nocapture
cargo test -p turbo-picard-cli gathervcfs -- --nocapture
cargo test -p turbo-picard-cli mergevcfs -- --nocapture
cargo test -p turbo-picard-cli sortvcf -- --nocapture
cargo clippy -p turbo-picard-markdup --all-targets --all-features -- -D warnings
```

Blocked or not clean:

- `cargo clippy -p turbo-picard-cli --lib --tests --all-features -- -D warnings`
  still fails in the existing `turbo-picard-cli` lint backlog.

Remaining Phase 7 work:

- keep `LiftoverVcf` on compact external-sort records while reducing remaining
  full-document VCF materialization.
