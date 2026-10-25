# Rust community forum

Category:

Announcements

Title:

`turbo-picard`: Rust implementations of selected Picard genomics commands

Post:

I have released `turbo-picard` 0.1.12:

https://github.com/dnncha/turbo-picard

It is a Rust implementation of selected Broad Picard commands used in genomics
pipelines. The project keeps Picard-style command names and `KEY=VALUE`
arguments, so existing workflow tasks can be tested with minimal command-line
changes.

Example:

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

The historical 14 August 2026 suite covers 32 small-fixture comparisons
against Picard 3.4.0, with an 84.52x geometric mean and 272.12x maximum
saved ratio. JVM startup contributes to these short-task results; they
do not predict a whole-genome or current-release speedup. Required
outputs and full resource costs need a representative trial.

The Rust side is a workspace with CLI/core crates plus a dedicated
`MarkDuplicates` crate. HTS file I/O uses `rust-htslib`.

Install:

```bash
python3 -m pip install turbo-picard==0.1.16
```

or from source:

```bash
cargo install --locked --path crates/turbo-picard-cli --bin turbo-picard --bin picard
```

Docs:

- https://turbo-picard.readthedocs.io/en/latest/
- https://turbo-picard.readthedocs.io/en/latest/benchmarks.html
- https://turbo-picard.readthedocs.io/en/latest/parity.html

I would especially appreciate feedback on the Rust packaging story, the
`rust-htslib` boundary, and how to make the accelerated command code easier to
review.
