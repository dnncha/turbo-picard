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

The historical 14 August 2026 suite covers 32 small-fixture comparisons
against Picard 3.4.0, with an 84.52x geometric mean and 272.12x maximum
saved ratio. JVM startup contributes to these short-task results; they
do not predict a whole-genome or current-release speedup. Required
outputs and full resource costs need a representative trial.

Useful links:

- Quickstart: https://turbo-picard.readthedocs.io/en/latest/quickstart.html
- Evaluation notes: https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html
- Benchmarks: https://turbo-picard.readthedocs.io/en/latest/benchmarks.html
- Workflow examples: https://github.com/dnncha/turbo-picard/tree/main/packaging/workflows

Install:

```bash
python3 -m pip install turbo-picard==0.1.16
```

PyPI publishes platform wheels and a source distribution. Bioconda accepted
the initial recipes; the 25 October 2026 channel check lists `0.1.15`. The
`0.1.16` main-package and shim updates are open upstream. Use a pinned
PyPI wheel or container for `0.1.16`; check the current channel before posting.

I would be glad to hear from Nextflow maintainers about which Picard command would be
worth testing first in a real pipeline.


Independent evaluation protocol and report checklist:
https://turbo-picard.readthedocs.io/en/latest/independent-evaluation.html
