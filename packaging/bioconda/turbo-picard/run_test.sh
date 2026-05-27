#!/usr/bin/env bash
set -euo pipefail

cat > input.sam <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-a	0	chr1	10	60	50M	*	0	0	AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA	IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII
read-b	0	chr1	10	60	50M	*	0	0	AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA	IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII
SAM

turbo-picard MarkDuplicates \
  I=input.sam \
  O=marked.sam \
  M=metrics.txt \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s marked.sam
test -s metrics.txt
grep -q 'UNPAIRED_READ_DUPLICATES' metrics.txt
grep -q $'Unknown Library\t2\t0\t0\t0\t1\t0\t0\t0.5' metrics.txt

picard MarkDuplicates \
  I=input.sam \
  O=picard-shim.sam \
  M=picard-shim.metrics.txt \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s picard-shim.sam
test -s picard-shim.metrics.txt

cat > unsorted.sam <<'SAM'
@HD	VN:1.6	SO:unsorted
@SQ	SN:chr1	LN:1000
read-c	0	chr1	90	60	10M	*	0	0	CCCCCCCCCC	FFFFFFFFFF
read-a	0	chr1	10	60	10M	*	0	0	AAAAAAAAAA	FFFFFFFFFF
read-b	0	chr1	50	60	10M	*	0	0	BBBBBBBBBB	FFFFFFFFFF
SAM

turbo-picard SortSam \
  I=unsorted.sam \
  O=coordinate.sam \
  SORT_ORDER=coordinate \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s coordinate.sam
grep -q $'@HD\tVN:1.6\tSO:coordinate' coordinate.sam
awk '!/^@/ { print $1 }' coordinate.sam | tr '\n' ' ' | grep -q '^read-a read-b read-c $'

turbo-picard SamToFastq \
  I=input.sam \
  FASTQ=reads.fastq \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s reads.fastq
grep -q '^@read-a$' reads.fastq

turbo-picard AddOrReplaceReadGroups \
  I=input.sam \
  O=readgroups.sam \
  RGID=new \
  RGLB=library-a \
  RGPL=ILLUMINA \
  RGPU=unit-a \
  RGSM=sample-a \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s readgroups.sam
grep -q $'@RG\tID:new\tLB:library-a\tPL:ILLUMINA\tSM:sample-a\tPU:unit-a' readgroups.sam
grep -q $'RG:Z:new' readgroups.sam

turbo-picard CollectAlignmentSummaryMetrics \
  I=input.sam \
  O=alignment_metrics.txt \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s alignment_metrics.txt
grep -q 'picard.analysis.AlignmentSummaryMetrics' alignment_metrics.txt
grep -q '^UNPAIRED' alignment_metrics.txt
