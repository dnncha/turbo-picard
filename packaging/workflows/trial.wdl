version 1.0

task MarkDuplicatesTurbo {
  input {
    File input_bam
    String sample_id
  }

  command <<<
    turbo-picard MarkDuplicates \
      I=~{input_bam} \
      O=~{sample_id}.marked.bam \
      M=~{sample_id}.metrics.txt \
      ASSUME_SORTED=true \
      VALIDATION_STRINGENCY=SILENT
  >>>

  output {
    File marked_bam = "~{sample_id}.marked.bam"
    File metrics = "~{sample_id}.metrics.txt"
  }
}

workflow TurboPicardTrial {
  input {
    File input_bam
    String sample_id
  }

  call MarkDuplicatesTurbo {
    input:
      input_bam = input_bam,
      sample_id = sample_id
  }

  output {
    File marked_bam = MarkDuplicatesTurbo.marked_bam
    File metrics = MarkDuplicatesTurbo.metrics
  }
}
