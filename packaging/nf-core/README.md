# nf-core integration notes

This directory holds copy-paste examples for wiring `turbo-picard` into nf-core
modules without changing the Picard argument style pipelines already use.

## Recommended rollout

1. Run `tools/audit_real_data.py` on a representative BAM from the workflow.
2. Keep upstream Picard available through `TURBO_PICARD_FALLBACK_COMMAND` while
   mixed command coverage remains.
3. Enable `params.use_turbo_picard = true` only after the audit bundle passes for
   the commands that module invokes.

## Example process block

```nextflow
process PICARD_MARK_DUPLICATES {
    tag "$meta.id"
    label 'process_medium'

    input:
    tuple val(meta), path(bam)

    output:
    tuple val(meta), path("*.bam"), emit: bam
    path "*.metrics.txt", emit: metrics

    script:
    def picard = params.use_turbo_picard ? 'turbo-picard' : 'picard'
    """
    ${picard} MarkDuplicates \\
        I=${bam} \\
        O=${meta.id}.marked.bam \\
        M=${meta.id}.metrics.txt \\
        ASSUME_SORTED=true \\
        VALIDATION_STRINGENCY=SILENT \\
        CREATE_INDEX=true
    """
}
```

## CRAM inputs

Pass the workflow reference FASTA with Picard-compatible `REFERENCE_SEQUENCE`:

```nextflow
    """
    export TURBO_PICARD_REFERENCE=${params.fasta}
    ${picard} SortSam I=${cram} O=${meta.id}.sorted.cram SORT_ORDER=coordinate R=${params.fasta}
    """
```

## Container image

Build the reference image from the repository root:

```bash
docker build -t turbo-picard:local .
```

Use that image in a profile while evaluating speed and parity before opening an
nf-core module PR.

## Opt-in process candidate

The repository also contains `turbo_picard_markduplicates.nf`, an opt-in process
candidate with explicit BAM/CRAM reference inputs and marked BAM, BAI, metrics
and version outputs. The test profile under `tests/` exercises the local BAM,
reference-backed CRAM, output channels and stub behavior using redistributable
repository fixtures. The candidate is still an adoption asset, not an nf-core
release: a pinned public container or Conda artifact, module lint, and external
nf-core review remain outstanding.

The process deliberately does not turn the test profile into a Picard
replacement claim. The tests establish wrapper wiring and output contracts;
production parity and performance require the separate evidence gates described
in `benchmarks/markduplicates-competitors/README.md`.
