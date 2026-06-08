nextflow.enable.dsl=2

params.input_bam = null
params.sample_id = "trial"
params.output_per_rg = false
params.rg_tag = "PU"
params.use_turbo_picard = true

workflow {
    Channel
        .of([
            [
                id: params.sample_id,
                output_per_rg: params.output_per_rg,
                rg_tag: params.rg_tag,
            ],
            file(params.input_bam),
        ])
        .set { bam_ch }

    PICARD_SAM_TO_FASTQ(bam_ch)
}

process PICARD_SAM_TO_FASTQ {
    tag "$meta.id"

    input:
    tuple val(meta), path(bam)

    output:
    tuple val(meta), path("${meta.id}.fastq"), optional: true, emit: reads
    tuple val(meta), path("${meta.id}.rg-fastq"), optional: true, emit: per_rg_reads

    script:
    def picard = params.use_turbo_picard ? 'turbo-picard' : 'picard'
    def outputArgs = meta.output_per_rg
        ? "OUTPUT_PER_RG=true \\\n        RG_TAG=${meta.rg_tag} \\\n        OUTPUT_DIR=${meta.id}.rg-fastq"
        : "FASTQ=${meta.id}.fastq"
    """
    ${picard} SamToFastq \\
        I=${bam} \\
        ${outputArgs} \\
        VALIDATION_STRINGENCY=SILENT
    """
}
