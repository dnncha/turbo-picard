nextflow.enable.dsl=2

params.use_turbo_picard = true

process PICARD_FASTQ_TO_SAM {
    tag "$meta.id"

    input:
    tuple val(meta), path(read1), path(read2)

    output:
    tuple val(meta), path("*.bam"), emit: bam

    script:
    def picard = params.use_turbo_picard ? 'turbo-picard' : 'picard'
    def useSequentialFastqs = meta.use_sequential_fastqs ?: false
    """
    ${picard} FastqToSam \\
        FASTQ=${read1} \\
        FASTQ2=${read2} \\
        OUTPUT=${meta.id}.unmapped.bam \\
        SAMPLE_NAME=${meta.sample_id ?: meta.id} \\
        READ_GROUP_NAME=${meta.read_group ?: meta.id} \\
        USE_SEQUENTIAL_FASTQS=${useSequentialFastqs}
    """
}
