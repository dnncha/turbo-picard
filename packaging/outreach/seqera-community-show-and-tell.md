# Seqera Community - Show & Tell

Category: Show & Tell

Title:

`turbo-picard`: Rust versions of selected Picard commands

Post:

Hi all,

I have been working on `turbo-picard`, a Rust implementation of selected Picard
commands:

https://github.com/dnncha/turbo-picard

The idea is to make a Picard-heavy workflow faster without changing the shape of
the task. Existing calls like this:

```bash
picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

can be tested as:

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

I would not recommend swapping a whole workflow at once. The better test is one
slow command on one representative input, then compare the BAM/SAM/metrics files
against upstream Picard before changing anything larger.

The repo currently has benchmark and parity checks for 32 commands against
Picard 3.4.0. In that suite the geometric mean speedup is 87.47x, with a 261.75x
top speedup. The `MarkDuplicates` run in the repo also shows lower memory use
than Picard on the same input.

Useful links:

- Quickstart: https://turbo-picard.readthedocs.io/en/latest/quickstart.html
- Evaluation notes: https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html
- Benchmarks: https://turbo-picard.readthedocs.io/en/latest/benchmarks.html
- Workflow examples: https://github.com/dnncha/turbo-picard/tree/main/packaging/workflows

Install:

```bash
python3 -m pip install turbo-picard
```

PyPI has Linux x86_64 and macOS Apple Silicon wheels plus a source tarball. The
existing Bioconda PR (#65922) covers older `0.1.10` metadata, so Bioconda is not
yet a current `0.1.12` install path. Use the published PyPI package or
container until the new candidate is tagged, reviewed, and published.

I would be glad to hear from Nextflow maintainers about which Picard command would be
worth testing first in a real pipeline.
