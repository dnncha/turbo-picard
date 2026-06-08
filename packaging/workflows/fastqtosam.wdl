version 1.0

task FastqToSamTurbo {
  input {
    File fastq_r1
    File fastq_r2
    String sample_id
    String read_group_name
    Boolean use_sequential_fastqs = false
  }

  command <<<
    turbo-picard FastqToSam \
      FASTQ=~{fastq_r1} \
      FASTQ2=~{fastq_r2} \
      OUTPUT=~{sample_id}.unmapped.bam \
      SAMPLE_NAME=~{sample_id} \
      READ_GROUP_NAME=~{read_group_name} \
      USE_SEQUENTIAL_FASTQS=~{if use_sequential_fastqs then "true" else "false"}
  >>>

  output {
    File unmapped_bam = "~{sample_id}.unmapped.bam"
  }
}
