process TURBO_PICARD_MARKDUPLICATES {
    tag "$meta.id"
    label 'process_medium'

    input:
    tuple val(meta), path(bam)
    path fasta
    path fai

    output:
    tuple val(meta), path("*.bam"), emit: bam
    tuple val(meta), path("*.bai"), emit: bai
    tuple val(meta), path("*.metrics.txt"), emit: metrics

    when:
    task.ext.when == null || task.ext.when

    script:
    def prefix = task.ext.prefix ?: "\${meta.id}"
    def reference_args = bam.name.endsWith('.cram') ? "REFERENCE_SEQUENCE=\${fasta}" : ""
    """
    turbo-picard MarkDuplicates \
        I=\${bam} \
        O=\${prefix}.marked.bam \
        M=\${prefix}.metrics.txt \
        \${reference_args}
    turbo-picard BuildBamIndex \
        I=\${prefix}.marked.bam \
        O=\${prefix}.marked.bam.bai
    """
}

