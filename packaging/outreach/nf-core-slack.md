# nf-core Slack post

We have been evaluating `turbo-picard` on a single Picard-shaped step instead
of treating it as a full replacement story.

The useful part so far is narrow: a command such as `MarkDuplicates` or
`SortSam` can be tested on representative data, compared directly against
upstream Picard, and kept behind a stable workflow boundary before anyone talks
about a wider switch.

Why it may be relevant here:

- the command shape stays familiar
- the repo includes starter material for `Nextflow` and `nf-core`-style trials
- the evaluation path is explicitly command-by-command, with fallback still
  available for unsupported surfaces

Good first commands are usually `MarkDuplicates`, `SortSam`, `SamToFastq`, or
`BuildBamIndex`, depending on where the workflow actually hurts.
