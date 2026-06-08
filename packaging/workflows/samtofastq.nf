nextflow.enable.dsl=2

params.use_turbo_picard = true

process PICARD_SAM_TO_FASTQ {
    tag "$meta.id"

    input:
    tuple val(meta), path(bam)

    output:
    tuple val(meta), path("${meta.id}.fastq"), optional: true, emit: reads
    tuple val(meta), path("${meta.id}.rg-fastq"), optional: true, emit: per_rg_reads

    script:
    def picard = params.use_turbo_picard ? 'turbo-picard' : 'picard'
    def outputPerRg = meta.output_per_rg ?: false
    def rgTag = meta.rg_tag ?: 'PU'
    def outputArgs = outputPerRg
        ? "OUTPUT_PER_RG=true \\\n        RG_TAG=${rgTag} \\\n        OUTPUT_DIR=${meta.id}.rg-fastq"
        : "FASTQ=${meta.id}.fastq"
    """
    ${picard} SamToFastq \\
        I=${bam} \\
        ${outputArgs}
    """
}
