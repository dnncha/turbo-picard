# Hacker News - Show HN

Use this only on a day when you can answer comments. Submit the GitHub repo as
the URL.

Title:

Show HN: turbo-picard - Rust implementations of selected Picard genomics commands

Initial comment:

Hi HN,

I built `turbo-picard`, a Rust implementation of selected Broad Picard commands
used in genomics pipelines:

https://github.com/dnncha/turbo-picard

Picard is common in sequencing workflows, but some commands are slow or memory
heavy when they run across many samples or shards. `turbo-picard` keeps the
Picard command style and replaces selected commands with native Rust code:

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

The historical 14 August 2026 suite covers 32 small-fixture comparisons
against Picard 3.4.0, with an 84.52x geometric mean and 272.12x maximum
saved ratio. JVM startup contributes to these short-task results; they
do not predict a whole-genome or current-release speedup. Required
outputs and full resource costs need a representative trial.

The safe way to use it is one command at a time: run Picard and `turbo-picard`
on the same representative input, compare the files your workflow depends on,
and only then switch that command.

Install:

```bash
python3 -m pip install turbo-picard==0.1.16
```

PyPI publishes platform wheels and a source distribution. Bioconda accepted
the initial recipes; the 25 October 2026 channel check lists `0.1.15`. The
`0.1.16` main-package and shim updates are open upstream. Use a pinned
PyPI wheel or container for `0.1.16`; check the current channel before posting.

I would be interested in feedback on the parity checks, HPC packaging, and the
fallback model for unsupported Picard commands.
