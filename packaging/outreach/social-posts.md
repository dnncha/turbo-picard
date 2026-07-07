# Social posts

## LinkedIn

I have released `turbo-picard` 0.1.8, a Rust implementation of selected Broad
Picard commands used in genomics workflows.

It is meant for the practical case where a workflow already calls Picard and one
step is costing too much runtime or memory. You can test that command without
redesigning the task:

```bash
turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
```

The repo includes parity checks, benchmark logs, real-data checks, and examples
for WDL, Nextflow, and Snakemake. Current benchmark table: 32 checked commands,
24.94x geometric mean speedup, and 94.36x top speedup versus Picard 3.4.0.

My suggested adoption path is one command at a time: run both tools on the same
representative input, compare the outputs, and only switch the command that
passes review on your data.

GitHub: https://github.com/dnncha/turbo-picard
Docs: https://turbo-picard.readthedocs.io/en/latest/
PyPI: https://pypi.org/project/turbo-picard/

## Mastodon / Bluesky

I released `turbo-picard` 0.1.8: Rust implementations of selected Broad Picard
commands for genomics workflows.

It keeps Picard-style command lines, so you can test one slow step without
rewriting the surrounding workflow.

GitHub: https://github.com/dnncha/turbo-picard
Docs: https://turbo-picard.readthedocs.io/

## Short version

`turbo-picard` 0.1.8 is out: Rust implementations of selected Picard genomics
commands.

Try one slow Picard step, compare outputs on a real input, and switch only what
checks out.

https://github.com/dnncha/turbo-picard
