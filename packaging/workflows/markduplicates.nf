nextflow.enable.dsl=2

params.use_turbo_picard = true

process PICARD_MARK_DUPLICATES {
    tag "$meta.id"

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
        VALIDATION_STRINGENCY=SILENT
    """
}
