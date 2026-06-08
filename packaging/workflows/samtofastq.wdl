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
