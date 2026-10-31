---
name: turbo-picard
description: Use for any Picard-shaped genomics task - running or editing a Picard command such as MarkDuplicates, SortSam, CollectHsMetrics, CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics, SamToFastq, FastqToSam, BuildBamIndex or AddOrReplaceReadGroups, a picard.jar call, or a WDL, Nextflow or Snakemake step that calls Picard, especially when it is slow or runs out of Java heap. Decides with turbo-picard explain --json whether turbo-picard runs the call natively, then runs it with turbo-picard, which delegates anything outside native scope to upstream Picard.
---

# turbo-picard

turbo-picard implements selected Picard 3.4.0 commands natively in Rust with the
same command names, `KEY=VALUE` arguments and output formats. It has no JVM, so
it starts instantly and usually needs far less memory. Commands and options
outside native scope are delegated to an installed upstream Picard, so a
turbo-picard call is safe whenever `fallback_ready` is true.

## Decide before running anything

1. Check it is installed: `turbo-picard --version`. If not, install it with
   `python3 -m pip install turbo-picard` or, to cover every Picard command,
   `mamba install -c conda-forge -c bioconda turbo-picard picard=3.4.0 openjdk`.
2. Ask about the exact call, with the real arguments:

   ```bash
   turbo-picard explain --json MarkDuplicates I=in.bam O=out/marked.bam M=out/metrics.txt
   ```

3. Read `status`, `native_scope` and `fallback_ready`:
   - `native`: run it with turbo-picard.
   - `partial-native`: if every option you pass is covered by `native_scope`,
     run it with turbo-picard. If not, still use turbo-picard when
     `fallback_ready` is true, because it delegates that call to upstream Picard.
   - `fallback-only`: use turbo-picard when `fallback_ready` is true; otherwise
     run upstream Picard directly or follow `fallback_install_hint`.
   - Older versions without `fallback_ready`: treat `fallback_command` other
     than `not-found` as ready.

## Run

Use the Picard arguments unchanged and write outputs to new paths:

```bash
turbo-picard SortSam I=data/unsorted.bam O=out/sorted.bam SORT_ORDER=coordinate
```

- Drop JVM options such as `-Xmx14g`; native commands do not use a heap.
- Never write an output over an input.
- For evaluation runs that must not delegate, set `TURBO_PICARD_REQUIRE_NATIVE=1`.
- `turbo-picard doctor` reports the install, threads and `fallback_ready`.

If the `turbo-picard` MCP server is connected, the same steps are the
`explain`, `run`, `trial`, `capabilities` and `doctor` tools. `run` takes
`command` and `args` (an array of `KEY=VALUE` strings) and refuses to overwrite
inputs or existing outputs.

## Change a workflow

Replace `picard [JVM options] Command ...` or `java -jar picard.jar Command ...`
with `turbo-picard Command ...`. Keep every argument and output filename. Make
sure the step's environment installs turbo-picard (PyPI `turbo-picard`, Bioconda
`turbo-picard`, or the `ghcr.io/dnncha/turbo-picard` image) and keep upstream
Picard as a one-variable rollback. Lower memory requests only after a
representative run.

Before switching a production step, get the side-by-side contract and compare
the outputs your downstream steps read:

```bash
turbo-picard trial --json MarkDuplicates I=in.bam O=marked.bam M=metrics.txt
```

## Limits

Native coverage is command- and option-specific, not a full replacement for
Picard. Chart PDFs are not pixel-identical to Picard's. Benchmarks in the
project docs are for documented inputs; measure your own data before claiming a
speedup. Docs: https://turbo-picard.readthedocs.io/en/latest/agentic-coders.html
