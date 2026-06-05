nextflow.enable.dsl=2

params.use_turbo_picard = true

process PICARD_SAM_TO_FASTQ {
    tag "$meta.id"

    input:
    tuple val(meta), path(bam)

    output:
    tuple val(meta), path("*.fastq"), emit: reads

    script:
    def picard = params.use_turbo_picard ? 'turbo-picard' : 'picard'
    """
    ${picard} SamToFastq \\
        I=${bam} \\
        FASTQ=${meta.id}.fastq
    """
}
