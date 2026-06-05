nextflow.enable.dsl=2

params.use_turbo_picard = true

process PICARD_SORT_SAM {
    tag "$meta.id"

    input:
    tuple val(meta), path(bam)

    output:
    tuple val(meta), path("*.bam"), emit: bam

    script:
    def picard = params.use_turbo_picard ? 'turbo-picard' : 'picard'
    """
    ${picard} SortSam \\
        I=${bam} \\
        O=${meta.id}.sorted.bam \\
        SORT_ORDER=coordinate
    """
}
