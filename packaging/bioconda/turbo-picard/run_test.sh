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
