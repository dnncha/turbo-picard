# Direct maintainer note

Subject:

Possible `turbo-picard` test for one Picard workflow step

Body:

Hi,

I maintain `turbo-picard`, a Rust implementation of selected Picard commands:

https://github.com/dnncha/turbo-picard

I am looking for good real-world test cases. I am not asking you to replace
Picard across a workflow. The more useful test would be one slow command on one
representative input, with the outputs compared against upstream Picard before
anything changes.

Example:

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

The repo has parity and benchmark checks against Picard 3.4.0. Current numbers:
32 checked commands, 87.47x geometric mean speedup, and 261.75x top speedup. The
parity docs are here:

https://turbo-picard.readthedocs.io/en/latest/parity.html

If one Picard step in your workflow is already a runtime or memory problem, I
would be interested in using that as a focused comparison.

Thanks,
Donncha
