nextflow.enable.dsl=2

params.input_bam = null
params.sample_id = "trial"
params.use_turbo_picard = true

workflow {
    Channel
        .of([ [id: params.sample_id], file(params.input_bam) ])
        .set { bam_ch }

    PICARD_FIX_MATE_INFORMATION(bam_ch)
}

process PICARD_FIX_MATE_INFORMATION {
    tag "$meta.id"

    input:
    tuple val(meta), path(bam)

    output:
    tuple val(meta), path("*.bam"), emit: bam
    tuple val(meta), path("*.bai"), emit: bai

    script:
    def picard = params.use_turbo_picard ? 'turbo-picard' : 'picard'
    """
    ${picard} FixMateInformation \\
        I=${bam} \\
        O=${meta.id}.fixed.bam \\
        CREATE_INDEX=true \\
        VALIDATION_STRINGENCY=SILENT
    """
}
