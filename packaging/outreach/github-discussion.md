# GitHub discussion or issue comment

If you are looking at a pipeline step that already uses Picard, `turbo-picard`
may be worth a narrow trial on that one command before discussing anything
broader.

Why it is easy to test:

- it keeps the familiar Picard-style command shape
- the repo includes starter files for `WDL`, `Nextflow`, and `Snakemake`
- there is a `choose-your-first-command.md` guide plus a small one-command
  trial bundle

The sensible workflow is:

1. choose one expensive Picard step
2. run upstream Picard and `turbo-picard` on the same representative shard
3. compare the exact downstream-consumed outputs
4. decide on that one command before talking about a wider switch
