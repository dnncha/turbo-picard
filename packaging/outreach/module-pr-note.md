# Workflow or module PR note

This change adds evaluation material for trying `turbo-picard` on a single
Picard-shaped step before a wider workflow switch.

Why this is useful:

- `turbo-picard` keeps familiar Picard-style arguments
- the repository includes public benchmark and parity evidence
- the workflow starters are small enough to test on representative data before
  changing shared pipeline behavior

Suggested first targets are `MarkDuplicates`, `SortSam`, `SamToFastq`, and
`BuildBamIndex`, depending on which step is the real bottleneck in the
workflow.
