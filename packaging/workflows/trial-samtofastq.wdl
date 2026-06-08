version 1.0

task SamToFastqTurbo {
  input {
    File input_bam
    String sample_id
    Boolean output_per_rg = false
    String rg_tag = "PU"
  }

  command <<<
    turbo-picard SamToFastq \
      I=~{input_bam} \
      ~{if output_per_rg then "OUTPUT_PER_RG=true" else "FASTQ=" + sample_id + ".fastq"} \
      ~{if output_per_rg then "RG_TAG=" + rg_tag else ""} \
      ~{if output_per_rg then "OUTPUT_DIR=" + sample_id + ".rg-fastq" else ""} \
      VALIDATION_STRINGENCY=SILENT
  >>>

  output {
    File? fastq = "~{sample_id}.fastq"
    Array[File] per_rg_fastqs = glob(sample_id + ".rg-fastq/*")
  }
}

workflow TurboPicardSamToFastqTrial {
  input {
    File input_bam
    String sample_id
    Boolean output_per_rg = false
    String rg_tag = "PU"
  }

  call SamToFastqTurbo {
    input:
      input_bam = input_bam,
      sample_id = sample_id,
      output_per_rg = output_per_rg,
      rg_tag = rg_tag
  }

  output {
    File? fastq = SamToFastqTurbo.fastq
    Array[File] per_rg_fastqs = SamToFastqTurbo.per_rg_fastqs
  }
}
