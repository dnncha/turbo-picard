nextflow.enable.dsl=2

params.input_fastq = null
params.input_fastq2 = null
params.sample_id = "trial"
params.read_group = "trial-rg"
params.use_turbo_picard = true

workflow {
    Channel
        .of([
            [id: params.sample_id, sample_id: params.sample_id, read_group: params.read_group],
            file(params.input_fastq),
            file(params.input_fastq2),
        ])
        .set { fastq_ch }

    PICARD_FASTQ_TO_SAM(fastq_ch)
}

process PICARD_FASTQ_TO_SAM {
    tag "$meta.id"

    input:
    tuple val(meta), path(read1), path(read2)

    output:
    tuple val(meta), path("*.bam"), emit: bam

    script:
    def picard = params.use_turbo_picard ? 'turbo-picard' : 'picard'
    """
    ${picard} FastqToSam \\
        FASTQ=${read1} \\
        FASTQ2=${read2} \\
        OUTPUT=${meta.id}.unmapped.bam \\
        SAMPLE_NAME=${meta.sample_id} \\
        READ_GROUP_NAME=${meta.read_group} \\
        VALIDATION_STRINGENCY=SILENT
    """
}
