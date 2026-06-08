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
      CREATE_INDEX=true \
      VALIDATION_STRINGENCY=SILENT
  >>>

  output {
    File fixed_bam = "~{sample_id}.fixed.bam"
    File fixed_bai = "~{sample_id}.fixed.bai"
  }
}

workflow TurboPicardFixMateTrial {
  input {
    File input_bam
    String sample_id
  }

  call FixMateInformationTurbo {
    input:
      input_bam = input_bam,
      sample_id = sample_id
  }

  output {
    File fixed_bam = FixMateInformationTurbo.fixed_bam
    File fixed_bai = FixMateInformationTurbo.fixed_bai
  }
}
