# Algorithmic Overhaul Phase 8 Notes

Date: 2026-06-20
Branch: `perf/algorithmic-overhaul`

## Scope

Phase 8 introduces command-level resource planning so BGZF, application worker,
indexing, and batching choices can be reasoned about together instead of as
separate local defaults.

## Implemented

- Added a `resource_plan` module in the CLI crate. The plan reports detected CPU
  count, global thread ceiling, BGZF reader/writer/index/pipeline-reader thread
  counts, an application worker budget, and CMM batch/queue settings.
- The resolver has deterministic tests for reported CPU counts 1, 2, 4, 8, 16,
  and 64, plus tests for role overrides, global thread overrides, `auto`/invalid
  override fallback, and CMM batch/queue environment overrides.
- `turbo-picard doctor` now prints `resource_plan_*` lines so the resolved
  resource plan is visible without running a data-processing command.

## Tests

Passing:

```bash
cargo fmt --check
cargo test -p turbo-picard-cli resource_plan -- --nocapture
cargo test -p turbo-picard-cli doctor -- --nocapture
cargo test -p turbo-picard-cli acceleration_status -- --nocapture
```

Remaining Phase 8 work:

- Integrate the resource plan into HTS reader/writer/index configuration so
  command execution uses the same resolved budget that `doctor` reports.
- Add command-specific planning for merge commands, sorter buffers, and
  simultaneous input readers.
