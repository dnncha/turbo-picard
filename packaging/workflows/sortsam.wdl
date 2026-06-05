version 1.0

task SortSamTurbo {
  input {
    File input_bam
    String sample_id
  }

  command <<<
    turbo-picard SortSam \
      I=~{input_bam} \
      O=~{sample_id}.sorted.bam \
      SORT_ORDER=coordinate
  >>>

  output {
    File sorted_bam = "~{sample_id}.sorted.bam"
  }
}
