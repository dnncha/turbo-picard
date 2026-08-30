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

The repo includes parity fixtures, real-data checks, benchmark logs, and
examples for WDL, Nextflow, and Snakemake. The current benchmark table covers 32
commands against Picard 3.4.0, with an 84.52x geometric mean speedup and 272.12x
top speedup.

The safe way to use it is one command at a time: run Picard and `turbo-picard`
on the same representative input, compare the files your workflow depends on,
and only then switch that command.

Install:

```bash
python3 -m pip install turbo-picard
```

PyPI has Linux x86_64 and macOS Apple Silicon wheels plus a source tarball. The
existing Bioconda PR (#65922) covers `0.1.11` metadata, so Bioconda is not
yet a current `0.1.12` install path. Use the published PyPI package or
container until the new candidate is tagged, reviewed, and published.

I would be interested in feedback on the parity checks, HPC packaging, and the
fallback model for unsupported Picard commands.
