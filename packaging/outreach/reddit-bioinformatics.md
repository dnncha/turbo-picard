# r/bioinformatics

Check the pinned "before you post" thread before submitting. If tool-release
posts are not welcome, skip Reddit or ask the mods first.

Suggested flair:

`programming` or `discussion`, depending on available options.

Title:

I released `turbo-picard`, Rust implementations of selected Picard commands

Post:

Hi r/bioinformatics,

I have been working on `turbo-picard`, a Rust command-line tool for selected
Picard commands:

https://github.com/dnncha/turbo-picard

It is for workflows that already call Picard and have one or two steps where
runtime or memory are annoying. The command interface is kept close to Picard:

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

I would test it one command at a time, not by replacing Picard across a whole
pipeline. Pick a real input, run both tools, compare the outputs you care about,
and only switch that command if the comparison is boring.

The repo has parity and benchmark checks against Picard 3.4.0. Current numbers:
32 checked commands, 24.94x geometric mean speedup, and 94.36x top speedup. The
`MarkDuplicates` benchmark also shows much lower memory use than Picard on the
same input.

Install:

```bash
python3 -m pip install turbo-picard
```

PyPI has Linux x86_64 and macOS Apple Silicon wheels plus a source tarball. The
existing Bioconda PR (#65922) covers older `0.1.10` metadata, so Bioconda is not
yet a current `0.1.12` install path. Use the published PyPI package or
container until the new candidate is tagged, reviewed, and published.

Docs:

- Quickstart: https://turbo-picard.readthedocs.io/en/latest/quickstart.html
- Evaluation notes: https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html
- Benchmarks: https://turbo-picard.readthedocs.io/en/latest/benchmarks.html
- Command coverage: https://turbo-picard.readthedocs.io/en/latest/commands.html

I am interested in practical criticism from people who run Picard in real
pipelines:
which command would be worth testing first, and what comparison would you need
before trusting it?
