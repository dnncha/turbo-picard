# Biostars

Suggested tags:

`tools`, `picard`, `bam`, `sam`, `workflow`, `rust`

Title:

`turbo-picard`: Rust implementations of selected Picard commands

Post:

I have released `turbo-picard`, a command-line tool that reimplements selected
Picard commands in Rust:

https://github.com/dnncha/turbo-picard

It is mainly for people who already have Picard steps in WDL, Nextflow,
Snakemake, or shell pipelines and want to test whether one of those steps can be
made faster without changing the command interface.

Example:

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

The way I suggest testing it is deliberately small:

1. pick one slow Picard step from a real workflow;
2. run Picard and `turbo-picard` on the same input;
3. compare the files your pipeline actually uses;
4. only then decide whether that command is worth switching.

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

This is not meant to imply that every Picard behavior has been reimplemented.
Unsupported commands can fall back to upstream Picard when configured, and the
docs spell out what the parity checks do and do not cover.

I would be interested to hear which Picard commands people would actually want
to try first on real data.


Independent evaluation protocol and report checklist:
https://turbo-picard.readthedocs.io/en/latest/independent-evaluation.html
