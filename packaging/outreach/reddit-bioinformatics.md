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

The historical 14 August 2026 suite covers 32 small-fixture comparisons
against Picard 3.4.0, with an 84.52x geometric mean and 272.12x maximum
saved ratio. JVM startup contributes to these short-task results; they
do not predict a whole-genome or current-release speedup. Required
outputs and full resource costs need a representative trial.

Install:

```bash
python3 -m pip install turbo-picard==0.1.16
```

PyPI publishes platform wheels and a source distribution. Bioconda accepted
the initial recipes; the 25 October 2026 channel check lists `0.1.15`. The
`0.1.16` main-package and shim updates are open upstream. Use a pinned
PyPI wheel or container for `0.1.16`; check the current channel before posting.

Docs:

- Quickstart: https://turbo-picard.readthedocs.io/en/latest/quickstart.html
- Evaluation notes: https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html
- Benchmarks: https://turbo-picard.readthedocs.io/en/latest/benchmarks.html
- Command coverage: https://turbo-picard.readthedocs.io/en/latest/commands.html

I am interested in practical criticism from people who run Picard in real
pipelines:
which command would be worth testing first, and what comparison would you need
before trusting it?
