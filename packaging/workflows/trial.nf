nextflow.enable.dsl=2

params.input_bam = null
params.sample_id = "trial"
params.use_turbo_picard = true

workflow {
    Channel
        .of([ [id: params.sample_id], file(params.input_bam) ])
        .set { bam_ch }

    PICARD_MARK_DUPLICATES(bam_ch)
}

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
