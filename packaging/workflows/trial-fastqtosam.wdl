version 1.0

task FastqToSamTurbo {
  input {
    File input_fastq
    File input_fastq2
    String sample_id
    String read_group
    Boolean use_sequential_fastqs = false
  }

  command <<<
    turbo-picard FastqToSam \
      FASTQ=~{input_fastq} \
      FASTQ2=~{input_fastq2} \
      OUTPUT=~{sample_id}.unmapped.bam \
      SAMPLE_NAME=~{sample_id} \
      READ_GROUP_NAME=~{read_group} \
      USE_SEQUENTIAL_FASTQS=~{if use_sequential_fastqs then "true" else "false"} \
      VALIDATION_STRINGENCY=SILENT
  >>>

  output {
    File unmapped_bam = "~{sample_id}.unmapped.bam"
  }
}

workflow TurboPicardFastqToSamTrial {
  input {
    File input_fastq
    File input_fastq2
    String sample_id
    String read_group
    Boolean use_sequential_fastqs = false
  }

  call FastqToSamTurbo {
    input:
      input_fastq = input_fastq,
      input_fastq2 = input_fastq2,
      sample_id = sample_id,
      read_group = read_group,
      use_sequential_fastqs = use_sequential_fastqs
  }

  output {
    File unmapped_bam = FastqToSamTurbo.unmapped_bam
  }
}
