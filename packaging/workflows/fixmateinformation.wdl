version 1.0

task FixMateInformationTurbo {
  input {
    File input_bam
    String sample_id
  }

  command <<<
    turbo-picard FixMateInformation \
      I=~{input_bam} \
      O=~{sample_id}.fixed.bam \
      CREATE_INDEX=true
  >>>

  output {
    File fixed_bam = "~{sample_id}.fixed.bam"
    File fixed_bai = "~{sample_id}.fixed.bai"
  }
}
