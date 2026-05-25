# Jeanluc Picard-Compatible Replacement Design

Date: 2026-05-25

## Goal

Build `jeanluc`, a much faster Picard-compatible command-line toolkit for genomics pipelines. The first release should not replace the `picard` executable directly. It should install a separate `jeanluc` command that can run Picard-shaped commands, starting with an accelerated native implementation of `MarkDuplicates`.

The long-term goal is a Bioconda package that users can opt into without breaking existing environments. A future package may add an optional `picard` compatibility shim after the native behavior and fallback behavior are proven.

## Initial Scope

The first accelerated command is:

- `jeanluc MarkDuplicates`

This is the highest-impact starting point because duplicate marking is a common wall-time bottleneck in real WGS and WES preprocessing pipelines, and Picard's Java implementation is often used as the compatibility reference.

The first native implementation targets coordinate-sorted SAM/BAM inputs. CRAM support is desirable, but it should depend on mature library behavior and test fixture coverage rather than being claimed before compatibility is proven.

Unsupported Picard commands should fail clearly in the first implementation. The command-dispatch layer should be designed so a later release can delegate unsupported tools to upstream Picard when a jar path is configured.

## Non-Goals For The First Implementation

- Installing or shadowing a `picard` executable.
- Reimplementing every Picard command.
- Supporting every `MarkDuplicates` option on day one if doing so would risk silent incompatibility.
- Claiming bit-for-bit BAM equality with Picard. Record order, duplicate flags, metrics values, and semantically equivalent headers matter more than byte-identical compressed output.
- Optimizing rare edge cases before the common coordinate-sorted BAM path is correct and benchmarked.

## Command-Line Compatibility

`jeanluc` should accept Picard-style command invocation:

```bash
jeanluc MarkDuplicates I=input.bam O=output.bam M=metrics.txt
```

It should also accept common long-option forms:

```bash
jeanluc MarkDuplicates --INPUT input.bam --OUTPUT output.bam --METRICS_FILE metrics.txt
```

The parser should normalize all supported aliases into one internal configuration:

- `I`, `INPUT`
- `O`, `OUTPUT`
- `M`, `METRICS_FILE`
- Common Picard booleans such as `REMOVE_DUPLICATES`, `CREATE_INDEX`, `CREATE_MD5_FILE`, and `ASSUME_SORTED`
- Common duplicate behavior options such as `DUPLICATE_SCORING_STRATEGY`, `TAGGING_POLICY`, `CLEAR_DT`, `READ_NAME_REGEX`, and `OPTICAL_DUPLICATE_PIXEL_DISTANCE`

Unsupported options must produce explicit diagnostics unless delegation is enabled in a later release. Silent ignores are not acceptable for Picard compatibility.

## Architecture

The repository should be a Rust workspace with these crates:

- `jeanluc-cli`: executable crate for command dispatch, Picard-style argument parsing, diagnostics, and process exit behavior.
- `jeanluc-core`: shared domain types, configuration normalization, metrics models, and error types.
- `jeanluc-markdup`: native duplicate-marking engine.

The CLI layer should stay thin. It should parse arguments, normalize configuration, dispatch to the native implementation, and format errors. Duplicate marking logic belongs in `jeanluc-markdup`.

Use `rust-htslib` for the first implementation because mature BAM and CRAM handling are more important than a pure-Rust dependency graph at this stage. `noodles` can be evaluated later for lower-level control or pure-Rust parsing where it does not weaken compatibility.

## MarkDuplicates Behavior

The native implementation should:

- Read coordinate-sorted SAM/BAM input.
- Preserve input headers and add a `@PG` program record for `jeanluc` without dropping existing header records.
- Identify duplicate sets using Picard-compatible duplicate keys for paired and unpaired reads.
- Select the representative read using the configured duplicate scoring strategy.
- Mark duplicate reads with the SAM duplicate flag unless removal is requested.
- Optionally remove duplicate reads when `REMOVE_DUPLICATES=true`.
- Write a metrics file compatible with downstream tools that consume Picard duplicate metrics.
- Support clear failure modes for unsupported sort orders, malformed records, unsupported options, and missing required files.

Optical duplicate detection should be implemented after core duplicate marking is validated. Until then, options related to optical duplicates should either be rejected with a clear message or routed through a compatibility fallback in a later release.

## Performance Strategy

The first implementation should focus on correctness and measurable speed on the common hot path:

- Coordinate-sorted BAM.
- Large WGS/WES inputs.
- Local temporary storage.
- Multi-core hosts typical of production bioinformatics pipelines.

Implementation should use streaming and bounded-memory grouping where possible. Expensive record-key extraction and duplicate scoring should be designed for parallelism, but correctness should come before speculative concurrency.

Benchmarks should track:

- Wall-clock time.
- Peak RSS.
- Output duplicate-flag concordance against Picard.
- Metrics-file differences.
- Throughput by input size and read layout.

## Testing

The test suite should include:

- Unit tests for Picard-style argument normalization.
- Unit tests for duplicate-key generation and duplicate scoring.
- Golden tests comparing small fixture outputs against Picard.
- Metrics compatibility tests with expected Picard metrics files.
- Error tests for unsupported commands and unsupported options.
- Smoke benchmarks for larger synthetic fixtures.

Compatibility tests should prefer semantic comparison over byte comparison. BAM compression differences are acceptable if SAM flags, read identity, representative selection, metrics, and required headers match expected behavior.

## Packaging

The first package should be a separate Bioconda candidate named `jeanluc`. It should install the `jeanluc` executable only.

The package should not declare itself as a direct replacement for Bioconda `picard` until:

- `MarkDuplicates` compatibility is validated on representative fixtures.
- Unsupported command behavior is predictable.
- Fallback/delegation behavior is implemented and tested, if direct replacement is still desired.
- Users can opt into a `picard` shim without surprising environments that already depend on Picard.

## Release Milestones

1. Scaffold Rust workspace, CLI dispatch, Picard-style parser, and unsupported-command diagnostics.
2. Implement `MarkDuplicates` configuration parsing and validation.
3. Implement coordinate-sorted BAM duplicate marking without optical duplicate classification.
4. Add Picard golden tests for small fixtures.
5. Add metrics-file compatibility tests.
6. Add performance benchmarks and large smoke fixtures.
7. Package `jeanluc` for local conda build.
8. Evaluate upstream Picard delegation and optional `picard` shim.

## Open Risks

- Exact Picard duplicate-set behavior has edge cases around mate information, secondary/supplementary reads, UMIs, optical duplicates, and malformed records.
- CRAM support may require reference handling that complicates drop-in behavior.
- Bioconda acceptance may require careful naming and dependency choices.
- Performance gains must be demonstrated against representative real data, not only synthetic fixtures.

## Acceptance Criteria For First Working Release

- `jeanluc MarkDuplicates I=in.bam O=out.bam M=metrics.txt` runs successfully on coordinate-sorted BAM input.
- The output is semantically concordant with Picard on curated fixtures.
- The metrics file is compatible with downstream Picard metrics consumers.
- Unsupported commands and options fail clearly.
- Benchmarks show meaningful speed or memory improvement on at least one realistic workload.
- The package installs a `jeanluc` command without shadowing `picard`.
