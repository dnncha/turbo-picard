# turbo-picard one-page brief

`turbo-picard` is a faster Rust implementation of selected Picard commands.
It is meant for teams that already use Picard in real pipelines and want a
lower-friction way to speed up the commands that hurt most.

## What stays familiar

- Picard-style command names
- Picard-style `KEY=VALUE` arguments
- existing workflow boundaries in `WDL`, `Nextflow`, `Snakemake`, or shell

## What the project is actually claiming

The intended adoption path is one command at a time, not a blanket replacement
story. The useful question is whether a specific Picard step can be tested on
representative data, compared directly against upstream Picard, and then
swapped in a narrow workflow boundary if the result is boring on real data.

## Good first commands

- `MarkDuplicates`
- `SortSam`
- `SamToFastq`
- `BuildBamIndex`

## What to do first

1. choose one expensive Picard command
2. run upstream Picard and `turbo-picard` on the same representative shard
3. compare the exact downstream-consumed outputs
4. decide on that one command before widening the claim

## What not to claim

- that all of Picard is already interchangeable
- that one benchmark number proves the tool is right for every workflow
- that exact Picard-rendered chart PDFs are reproduced everywhere

## Useful links

- docs quickstart: `https://turbo-picard.readthedocs.io/en/latest/quickstart.html`
- first command guide: `https://turbo-picard.readthedocs.io/en/latest/first-command.html`
- evaluation playbook: `https://turbo-picard.readthedocs.io/en/latest/evaluation-playbook.html`
- FAQ: `https://turbo-picard.readthedocs.io/en/latest/faq.html`
