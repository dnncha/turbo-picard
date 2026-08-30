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
32 checked commands, 84.52x geometric mean speedup, 272.12x top speedup.

Evaluation notes and workflow examples:

- https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html
- https://github.com/dnncha/turbo-picard/tree/main/packaging/workflows

I am trying to work out which nf-core pipeline/module would make the best first
real-world test case. Any suggestions?

Packaging note: PyPI has Linux x86_64 and macOS Apple Silicon wheels plus a
source tarball. The existing Bioconda PR (#65922) covers `0.1.11`
metadata, so it is not yet a current `0.1.12` install path. Use the published
PyPI package or container until the new candidate is tagged, reviewed, and
published.
