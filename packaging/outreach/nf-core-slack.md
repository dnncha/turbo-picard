# nf-core Slack

Use this only in a relevant channel. Keep it short and make the ask concrete.

Message:

Hi all - I am working on `turbo-picard`, Rust implementations of selected Picard
commands:
https://github.com/dnncha/turbo-picard

It keeps the Picard command style (`MarkDuplicates I=... O=... M=...`), so the
most sensible test is probably one slow Picard step rather than a broad workflow
change. The commands I would expect to be most relevant are `MarkDuplicates`,
`SortSam`, `SamToFastq`, `FastqToSam`, `FixMateInformation`, and
`BuildBamIndex`.

The historical 14 August 2026 suite covers 32 small-fixture comparisons
against Picard 3.4.0, with an 84.52x geometric mean and 272.12x maximum
saved ratio. JVM startup contributes to these short-task results; they
do not predict a whole-genome or current-release speedup. Required
outputs and full resource costs need a representative trial.

Evaluation notes and workflow examples:

- https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html
- https://github.com/dnncha/turbo-picard/tree/main/packaging/workflows

I am trying to work out which nf-core pipeline/module would make the best first
real-world test case. Any suggestions?

Packaging note: PyPI has Linux x86_64 and macOS Apple Silicon wheels plus a
source tarball. Bioconda accepted the initial recipes; the 25 October 2026 channel check lists
`0.1.15`. Updates to `0.1.16` are open upstream. Use pinned PyPI or the
container for `0.1.16`, and verify channel status before posting.

Independent evaluation protocol and report checklist:
https://turbo-picard.readthedocs.io/en/latest/independent-evaluation.html
