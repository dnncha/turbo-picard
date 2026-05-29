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
        if line.strip() and not line.startswith("@PG")
    ]

turbo = stable_lines(sys.argv[1])
picard = stable_lines(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"RevertSam stable SAM output differs:\nturbo={turbo}\npicard={picard}")
print("RevertSam stable SAM output matches Picard")
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
