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

The repo has parity and benchmark checks against Picard 3.4.0. Current numbers:
32 checked commands, 24.94x geometric mean speedup, 94.36x top speedup.

Evaluation notes and workflow examples:

- https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html
- https://github.com/dnncha/turbo-picard/tree/main/packaging/workflows

I am trying to work out which nf-core pipeline/module would make the best first
real-world test case. Any suggestions?

Packaging note: PyPI has Linux x86_64 and macOS Apple Silicon wheels plus a
source tarball. A Bioconda recipe is in progress for Linux/HPC use.
