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

The repo has parity and benchmark checks against Picard 3.4.0. The current
benchmark table covers 32 checked commands, with an 84.52x geometric mean speedup
and a 272.12x top speedup. There is also a `MarkDuplicates` run showing much
lower memory use than Picard on the same input.

Install:

```bash
python3 -m pip install turbo-picard
```

PyPI has Linux x86_64 and macOS Apple Silicon wheels plus a source tarball. The
existing Bioconda PR (#65922) covers `0.1.11` metadata, so Bioconda is not
yet a current `0.1.12` install path. Use the published PyPI package or
container until the new candidate is tagged, reviewed, and published.

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
