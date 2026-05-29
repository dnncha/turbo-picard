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

cat > "$workdir/r1.fastq" <<'FQ'
@read1
ACGT
+
FFFF
@read2
TGCA
+
EEEE
FQ

cat > "$workdir/r2.fastq" <<'FQ'
@read1
TTTT
+
IIII
@read2
CCCC
+
HHHH
FQ

cargo run -q -p turbo-picard-cli --bin picard -- \
  FastqToSam \
  "F1=$workdir/r1.fastq" \
  "F2=$workdir/r2.fastq" \
  "O=$workdir/turbo.sam" \
  SM=sample \
  RG=rg1 \
  LB=lib \
  PL=ILLUMINA \
  PU=unit \
  QUALITY_FORMAT=Standard \
  COMMENT=first-comment \
  COMMENT=second-comment \
  CREATE_MD5_FILE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FastqToSam \
  "F1=$workdir/r1.fastq" \
  "F2=$workdir/r2.fastq" \
  "O=$workdir/picard.sam" \
  SM=sample \
  RG=rg1 \
  LB=lib \
  PL=ILLUMINA \
  PU=unit \
  QUALITY_FORMAT=Standard \
  COMMENT=first-comment \
  COMMENT=second-comment \
  CREATE_MD5_FILE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.sam" "$workdir/picard.sam" <<'PY'
import sys

def stable(path):
    rows = []
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        if line.startswith("@HD") or line.startswith("@RG") or line.startswith("@CO") or not line.startswith("@"):
            rows.append(line)
    return rows

turbo = stable(sys.argv[1])
picard = stable(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"FastqToSam stable SAM differs:\nturbo={turbo}\npicard={picard}")
print("FastqToSam stable SAM matches Picard")
PY

cmp "$workdir/turbo.sam.md5" "$workdir/picard.sam.md5"
echo "FastqToSam MD5 sidecar matches Picard"

cargo run -q -p turbo-picard-cli --bin picard -- \
  FastqToSam \
  "F1=$workdir/r1.fastq" \
  "O=$workdir/turbo-unsorted.sam" \
  SM=sample \
  RG=rg1 \
  SORT_ORDER=unsorted \
  QUALITY_FORMAT=Standard \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FastqToSam \
  "F1=$workdir/r1.fastq" \
  "O=$workdir/picard-unsorted.sam" \
  SM=sample \
  RG=rg1 \
  SORT_ORDER=unsorted \
  QUALITY_FORMAT=Standard \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-unsorted.sam" "$workdir/picard-unsorted.sam" <<'PY'
import sys

def stable(path):
    rows = []
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        if line.startswith("@HD") or line.startswith("@RG") or not line.startswith("@"):
            rows.append(line)
    return rows

turbo = stable(sys.argv[1])
picard = stable(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"FastqToSam unsorted stable SAM differs:\nturbo={turbo}\npicard={picard}")
print("FastqToSam SORT_ORDER=unsorted stable SAM matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  FastqToSam \
  "F1=$workdir/r1.fastq" \
  "O=$workdir/turbo-coordinate.sam" \
  SM=sample \
  RG=rg1 \
  SORT_ORDER=coordinate \
  QUALITY_FORMAT=Standard \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FastqToSam \
  "F1=$workdir/r1.fastq" \
  "O=$workdir/picard-coordinate.sam" \
  SM=sample \
  RG=rg1 \
  SORT_ORDER=coordinate \
  QUALITY_FORMAT=Standard \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-coordinate.sam" "$workdir/picard-coordinate.sam" <<'PY'
import sys

def stable(path):
    rows = []
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        if line.startswith("@HD") or line.startswith("@RG") or not line.startswith("@"):
            rows.append(line)
    return rows

turbo = stable(sys.argv[1])
picard = stable(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"FastqToSam coordinate stable SAM differs:\nturbo={turbo}\npicard={picard}")
print("FastqToSam SORT_ORDER=coordinate stable SAM matches Picard")
PY
