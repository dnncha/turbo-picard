process TURBO_PICARD_MARKDUPLICATES {
    tag "${meta.id}"
    label 'process_medium'

    input:
    tuple val(meta), path(reads)
    tuple val(meta2), path(fasta), path(fai)

    output:
    tuple val(meta), path("*.bam"), emit: bam
    tuple val(meta), path("*.bai"), emit: bai
    tuple val(meta), path("*.metrics.txt"), emit: metrics
    tuple val("${task.process}"), val('turbo-picard'), eval("turbo-picard --version 2>&1 | sed -n 's/.* //p'"), topic: versions, emit: versions_turbo_picard

    when:
    task.ext.when == null || task.ext.when

    script:
    def args = task.ext.args ?: ''
    def prefix = task.ext.prefix ?: "${meta.id}.marked"
    def output_bam = "${prefix}.bam"
    def reference_args = reads.name.toLowerCase().endsWith('.cram') ? "REFERENCE_SEQUENCE=${fasta}" : ""
    if (reads.name.toLowerCase().endsWith('.cram') && !fasta) {
        error('A FASTA and FAI reference tuple is required for CRAM input')
    }
    if ("${reads}" == output_bam) {
        error('Input and output names are the same, use task.ext.prefix to disambiguate')
    }
    """
    turbo-picard MarkDuplicates \
        ${args} \
        I=${reads} \
        O=${output_bam} \
        M=${prefix}.metrics.txt \
        ${reference_args}
    turbo-picard BuildBamIndex \
        I=${output_bam} \
        O=${output_bam}.bai
    """

    stub:
    def prefix = task.ext.prefix ?: "${meta.id}.marked"
    """
    touch ${prefix}.bam
    touch ${prefix}.bam.bai
    touch ${prefix}.metrics.txt
    """
}
