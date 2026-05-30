#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conda_prefix="${TURBO_PICARD_CONDA_PREFIX:-$repo_root/.conda-turbo-picard}"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

if command -v mamba >/dev/null 2>&1; then
  conda_runner=(mamba)
elif command -v micromamba >/dev/null 2>&1; then
  conda_runner=(micromamba)
else
  echo "mamba or micromamba is required for Picard parity verification" >&2
  exit 127
fi

cat > "$workdir/input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
@RG	ID:rg1	SM:sample	LB:lib	PL:ILLUMINA
pair1	1123	chr1	10	60	4M	=	30	24	ACGT	!!!!	RG:Z:rg1	OQ:Z:FFFF	NM:i:0	MD:Z:4	PG:Z:align	MC:Z:4M	MQ:i:60
pair1	1171	chr1	30	60	4M	=	10	-24	TGCA	!!!!	RG:Z:rg1	OQ:Z:EEEE	NM:i:0	MD:Z:4	PG:Z:align	MC:Z:4M	MQ:i:60
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  RevertSam \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard RevertSam \
  "I=$workdir/input.sam" \
  "O=$workdir/picard.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.sam" "$workdir/picard.sam" <<'PY'
import sys

def stable_lines(path):
    return [
        line.rstrip("\n")
        for line in open(path, encoding="utf-8")
        if line.strip()
    ]

turbo = stable_lines(sys.argv[1])
picard = stable_lines(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"RevertSam stable SAM output differs:\nturbo={turbo}\npicard={picard}")
print("RevertSam stable SAM output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  RevertSam \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-md5.sam" \
  CREATE_MD5_FILE=true \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard RevertSam \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-md5.sam" \
  CREATE_MD5_FILE=true \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-md5.sam" "$workdir/picard-md5.sam" "$workdir/turbo-md5.sam.md5" "$workdir/picard-md5.sam.md5" <<'PY'
import sys

turbo_path, picard_path, turbo_md5_path, picard_md5_path = sys.argv[1:]

def stable_lines(path):
    return [
        line.rstrip("\n")
        for line in open(path, encoding="utf-8")
        if line.strip()
    ]

turbo = stable_lines(turbo_path)
picard = stable_lines(picard_path)
if turbo != picard:
    raise SystemExit(f"RevertSam MD5 SAM output differs:\nturbo={turbo}\npicard={picard}")
if open(turbo_md5_path, encoding="utf-8").read().strip() != open(picard_md5_path, encoding="utf-8").read().strip():
    raise SystemExit("RevertSam MD5 sidecar differs from Picard")
print("RevertSam MD5 sidecar and temp-option output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  RevertSam \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-create-index.bam" \
  CREATE_INDEX=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard RevertSam \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-create-index.bam" \
  CREATE_INDEX=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam \
  "I=$workdir/turbo-create-index.bam" \
  "O=$workdir/turbo-create-index.sam"

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam \
  "I=$workdir/picard-create-index.bam" \
  "O=$workdir/picard-create-index.sam"

python3 - "$workdir/turbo-create-index.sam" "$workdir/picard-create-index.sam" "$workdir/turbo-create-index.bai" "$workdir/picard-create-index.bai" <<'PY'
import os
import sys

turbo_path, picard_path, turbo_bai_path, picard_bai_path = sys.argv[1:]

def stable_lines(path):
    return [
        line.rstrip("\n")
        for line in open(path, encoding="utf-8")
        if line.strip() and not line.startswith("@PG")
    ]

turbo = stable_lines(turbo_path)
picard = stable_lines(picard_path)
if turbo != picard:
    raise SystemExit(f"RevertSam CREATE_INDEX output differs:\nturbo={turbo}\npicard={picard}")
if os.path.exists(turbo_bai_path) or os.path.exists(picard_bai_path):
    raise SystemExit("RevertSam CREATE_INDEX unexpectedly wrote a BAI sidecar")
print("RevertSam CREATE_INDEX no-index output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  RevertSam \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-coordinate-index.bam" \
  SORT_ORDER=coordinate \
  CREATE_INDEX=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard RevertSam \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-coordinate-index.bam" \
  SORT_ORDER=coordinate \
  CREATE_INDEX=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam \
  "I=$workdir/turbo-coordinate-index.bam" \
  "O=$workdir/turbo-coordinate-index.sam"

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam \
  "I=$workdir/picard-coordinate-index.bam" \
  "O=$workdir/picard-coordinate-index.sam"

python3 - "$workdir/turbo-coordinate-index.sam" "$workdir/picard-coordinate-index.sam" "$workdir/turbo-coordinate-index.bai" "$workdir/picard-coordinate-index.bai" <<'PY'
import os
import sys

turbo_path, picard_path, turbo_bai_path, picard_bai_path = sys.argv[1:]

def stable_lines(path):
    return [
        line.rstrip("\n")
        for line in open(path, encoding="utf-8")
        if line.strip() and not line.startswith("@PG")
    ]

turbo = stable_lines(turbo_path)
picard = stable_lines(picard_path)
if turbo != picard:
    raise SystemExit(f"RevertSam coordinate CREATE_INDEX output differs:\nturbo={turbo}\npicard={picard}")
if not os.path.exists(turbo_bai_path) or not os.path.exists(picard_bai_path):
    raise SystemExit("RevertSam coordinate CREATE_INDEX did not write expected BAI sidecars")
print("RevertSam coordinate CREATE_INDEX output and BAI sidecar matches Picard")
PY

cat > "$workdir/sort-order-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-b	0	chr1	50	60	4M	*	0	0	CCCC	FFFF	OQ:Z:HHHH
read-b	1024	chr1	70	60	4M	*	0	0	GGGG	FFFF	OQ:Z:IIII
read-a	0	chr1	10	60	4M	*	0	0	AAAA	FFFF	OQ:Z:JJJJ
SAM

for sort_order in unsorted coordinate; do
  cargo run -q -p turbo-picard-cli --bin picard -- \
    RevertSam \
    "I=$workdir/sort-order-input.sam" \
    "O=$workdir/turbo-$sort_order.sam" \
    "SORT_ORDER=$sort_order" \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true

  "${conda_runner[@]}" run -p "$conda_prefix" picard RevertSam \
    "I=$workdir/sort-order-input.sam" \
    "O=$workdir/picard-$sort_order.sam" \
    "SORT_ORDER=$sort_order" \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true

  python3 - "$workdir/turbo-$sort_order.sam" "$workdir/picard-$sort_order.sam" "$sort_order" <<'PY'
import sys

turbo_path, picard_path, sort_order = sys.argv[1:]

def stable_lines(path):
    return [
        line.rstrip("\n")
        for line in open(path, encoding="utf-8")
        if line.strip() and not line.startswith("@PG")
    ]

turbo = stable_lines(turbo_path)
picard = stable_lines(picard_path)
if turbo != picard:
    raise SystemExit(f"RevertSam SORT_ORDER={sort_order} output differs:\nturbo={turbo}\npicard={picard}")
print(f"RevertSam SORT_ORDER={sort_order} output matches Picard")
PY
done

cat > "$workdir/keep-alignment-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
@PG	ID:aligner	PN:aligner
read1	1024	chr1	10	60	4M	*	0	0	ACGT	!!!!	OQ:Z:FFFF	NM:i:0	MD:Z:4	MC:Z:4M	MQ:i:60	XT:Z:clearme
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  RevertSam \
  "I=$workdir/keep-alignment-input.sam" \
  "O=$workdir/turbo-keep-alignment.sam" \
  REMOVE_ALIGNMENT_INFORMATION=false \
  RESTORE_HARDCLIPS=false \
  ATTRIBUTE_TO_CLEAR=XT \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard RevertSam \
  "I=$workdir/keep-alignment-input.sam" \
  "O=$workdir/picard-keep-alignment.sam" \
  REMOVE_ALIGNMENT_INFORMATION=false \
  RESTORE_HARDCLIPS=false \
  ATTRIBUTE_TO_CLEAR=XT \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-keep-alignment.sam" "$workdir/picard-keep-alignment.sam" <<'PY'
import sys

def stable_lines(path):
    return [
        line.rstrip("\n")
        for line in open(path, encoding="utf-8")
        if line.strip()
    ]

turbo = stable_lines(sys.argv[1])
picard = stable_lines(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"RevertSam keep-alignment output differs:\nturbo={turbo}\npicard={picard}")
print("RevertSam keep-alignment output matches Picard")
PY

cat > "$workdir/secondary-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-primary	0	chr1	10	60	4M	*	0	0	AAAA	!!!!	OQ:Z:FFFF	NM:i:0	MD:Z:4
read-secondary	256	chr1	20	60	4M	*	0	0	CCCC	!!!!	OQ:Z:GGGG	NM:i:0	MD:Z:4
read-supplementary	2048	chr1	30	60	4M	*	0	0	GGGG	!!!!	OQ:Z:HHHH	NM:i:0	MD:Z:4
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  RevertSam \
  "I=$workdir/secondary-input.sam" \
  "O=$workdir/turbo-secondary.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard RevertSam \
  "I=$workdir/secondary-input.sam" \
  "O=$workdir/picard-secondary.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-secondary.sam" "$workdir/picard-secondary.sam" <<'PY'
import sys

def stable_lines(path):
    return [
        line.rstrip("\n")
        for line in open(path, encoding="utf-8")
        if line.strip()
    ]

turbo = stable_lines(sys.argv[1])
picard = stable_lines(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"RevertSam secondary/supplementary filtering differs:\nturbo={turbo}\npicard={picard}")
print("RevertSam secondary/supplementary filtering matches Picard")
PY

cat > "$workdir/reverse-attrs-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read1	16	chr1	10	60	4M	*	0	0	ACGA	!!!!	OQ:Z:abcd	XR:Z:wxyz	XC:Z:ACGA	NM:i:0	MD:Z:4
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  RevertSam \
  "I=$workdir/reverse-attrs-input.sam" \
  "O=$workdir/turbo-reverse-attrs.sam" \
  ATTRIBUTE_TO_REVERSE=XR \
  ATTRIBUTE_TO_REVERSE_COMPLEMENT=XC \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard RevertSam \
  "I=$workdir/reverse-attrs-input.sam" \
  "O=$workdir/picard-reverse-attrs.sam" \
  ATTRIBUTE_TO_REVERSE=XR \
  ATTRIBUTE_TO_REVERSE_COMPLEMENT=XC \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-reverse-attrs.sam" "$workdir/picard-reverse-attrs.sam" <<'PY'
import sys

def stable_lines(path):
    return [
        line.rstrip("\n")
        for line in open(path, encoding="utf-8")
        if line.strip()
    ]

turbo = stable_lines(sys.argv[1])
picard = stable_lines(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"RevertSam reverse-attribute output differs:\nturbo={turbo}\npicard={picard}")
print("RevertSam reverse-attribute output matches Picard")
PY

cat > "$workdir/hardclips-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read1	16	chr1	10	60	2H4M3H	*	0	0	ACGA	!!!!	OQ:Z:abcd	XB:Z:TTCCC	XQ:Z:vwxyz	NM:i:0	MD:Z:4
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  RevertSam \
  "I=$workdir/hardclips-input.sam" \
  "O=$workdir/turbo-hardclips.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard RevertSam \
  "I=$workdir/hardclips-input.sam" \
  "O=$workdir/picard-hardclips.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-hardclips.sam" "$workdir/picard-hardclips.sam" <<'PY'
import sys

def stable_lines(path):
    return [
        line.rstrip("\n")
        for line in open(path, encoding="utf-8")
        if line.strip()
    ]

turbo = stable_lines(sys.argv[1])
picard = stable_lines(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"RevertSam hard-clip restoration output differs:\nturbo={turbo}\npicard={picard}")
print("RevertSam hard-clip restoration output matches Picard")
PY

cat > "$workdir/custom-tags-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
@RG	ID:rg1	SM:sample	LB:lib	PL:ILLUMINA
read1	1024	chr1	10	60	4M	*	0	0	ACGT	!!!!	RG:Z:rg1	OQ:Z:FFFF	NM:i:0	MD:Z:4	XT:Z:clearme	XA:i:7
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  RevertSam \
  "I=$workdir/custom-tags-input.sam" \
  "O=$workdir/turbo-custom.sam" \
  ATTRIBUTE_TO_CLEAR=XT \
  ATTRIBUTE_TO_CLEAR=XA \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard RevertSam \
  "I=$workdir/custom-tags-input.sam" \
  "O=$workdir/picard-custom.sam" \
  ATTRIBUTE_TO_CLEAR=XT \
  ATTRIBUTE_TO_CLEAR=XA \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-custom.sam" "$workdir/picard-custom.sam" <<'PY'
import sys

def stable_lines(path):
    return [
        line.rstrip("\n")
        for line in open(path, encoding="utf-8")
        if line.strip() and not line.startswith("@PG")
    ]

turbo = stable_lines(sys.argv[1])
picard = stable_lines(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"RevertSam custom ATTRIBUTE_TO_CLEAR output differs:\nturbo={turbo}\npicard={picard}")
print("RevertSam custom ATTRIBUTE_TO_CLEAR output matches Picard")
PY
