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
  "O=$workdir/turbo-runtime.sam" \
  SM=sample \
  RG=rg1 \
  QUALITY_FORMAT=Standard \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  MAX_RECORDS_IN_RAM=1000 \
  "TMP_DIR=$workdir" \
  "R=$workdir/ref.fa" \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FastqToSam \
  "F1=$workdir/r1.fastq" \
  "O=$workdir/picard-runtime.sam" \
  SM=sample \
  RG=rg1 \
  QUALITY_FORMAT=Standard \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  MAX_RECORDS_IN_RAM=1000 \
  "TMP_DIR=$workdir" \
  "R=$workdir/ref.fa" \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-runtime.sam" "$workdir/picard-runtime.sam" <<'PY'
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
    raise SystemExit(f"FastqToSam runtime stable SAM differs:\nturbo={turbo}\npicard={picard}")
print("FastqToSam runtime option stable SAM matches Picard")
PY

test -f "$workdir/turbo-runtime.sam.md5"
test -f "$workdir/picard-runtime.sam.md5"
test ! -e "$workdir/turbo-runtime.sam.bai"
test ! -e "$workdir/turbo-runtime.bai"
test ! -e "$workdir/picard-runtime.sam.bai"
test ! -e "$workdir/picard-runtime.bai"
echo "FastqToSam runtime sidecar compatibility matches Picard"

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

cat > "$workdir/empty-lines.fastq" <<'FQ'

@read1
ACGT
+
FFFF
FQ

cargo run -q -p turbo-picard-cli --bin picard -- \
  FastqToSam \
  "F1=$workdir/empty-lines.fastq" \
  "O=$workdir/turbo-empty-lines.sam" \
  SM=sample \
  RG=rg1 \
  ALLOW_AND_IGNORE_EMPTY_LINES=true \
  QUALITY_FORMAT=Standard \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FastqToSam \
  "F1=$workdir/empty-lines.fastq" \
  "O=$workdir/picard-empty-lines.sam" \
  SM=sample \
  RG=rg1 \
  ALLOW_AND_IGNORE_EMPTY_LINES=true \
  QUALITY_FORMAT=Standard \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-empty-lines.sam" "$workdir/picard-empty-lines.sam" <<'PY'
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
    raise SystemExit(f"FastqToSam empty-line stable SAM differs:\nturbo={turbo}\npicard={picard}")
print("FastqToSam ALLOW_AND_IGNORE_EMPTY_LINES output matches Picard")
PY

: > "$workdir/empty.fastq"
cargo run -q -p turbo-picard-cli --bin picard -- \
  FastqToSam \
  "F1=$workdir/empty.fastq" \
  "O=$workdir/turbo-empty.sam" \
  SM=sample \
  RG=rg1 \
  ALLOW_EMPTY_FASTQ=true \
  QUALITY_FORMAT=Standard \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FastqToSam \
  "F1=$workdir/empty.fastq" \
  "O=$workdir/picard-empty.sam" \
  SM=sample \
  RG=rg1 \
  ALLOW_EMPTY_FASTQ=true \
  QUALITY_FORMAT=Standard \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-empty.sam" "$workdir/picard-empty.sam" <<'PY'
import sys

def header(path):
    return [line.rstrip("\n") for line in open(path, encoding="utf-8") if line.startswith("@HD") or line.startswith("@RG")]

if header(sys.argv[1]) != header(sys.argv[2]):
    raise SystemExit("FastqToSam empty FASTQ header differs from Picard")
print("FastqToSam ALLOW_EMPTY_FASTQ header matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  FastqToSam \
  "F1=$workdir/r1.fastq" \
  "O=$workdir/turbo-minmax.sam" \
  SM=sample \
  RG=rg1 \
  MIN_Q=0 \
  MAX_Q=40 \
  QUALITY_FORMAT=Standard \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FastqToSam \
  "F1=$workdir/r1.fastq" \
  "O=$workdir/picard-minmax.sam" \
  SM=sample \
  RG=rg1 \
  MIN_Q=0 \
  MAX_Q=40 \
  QUALITY_FORMAT=Standard \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-minmax.sam" "$workdir/picard-minmax.sam" <<'PY'
import sys

def stable(path):
    rows = []
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        if line.startswith("@HD") or line.startswith("@RG") or not line.startswith("@"):
            rows.append(line)
    return rows

if stable(sys.argv[1]) != stable(sys.argv[2]):
    raise SystemExit("FastqToSam MIN_Q/MAX_Q output differs from Picard")
print("FastqToSam MIN_Q/MAX_Q output matches Picard")
PY

cat > "$workdir/illumina.fastq" <<'FQ'
@read1
ACGT
+
bbbb
FQ

for name in r1 illumina; do
  cargo run -q -p turbo-picard-cli --bin picard -- \
    FastqToSam \
    "F1=$workdir/$name.fastq" \
    "O=$workdir/turbo-auto-$name.sam" \
    SM=sample \
    RG=rg1 \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true

  "${conda_runner[@]}" run -p "$conda_prefix" picard FastqToSam \
    "F1=$workdir/$name.fastq" \
    "O=$workdir/picard-auto-$name.sam" \
    SM=sample \
    RG=rg1 \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true

  python3 - "$workdir/turbo-auto-$name.sam" "$workdir/picard-auto-$name.sam" "$name" <<'PY'
import sys

turbo_path, picard_path, name = sys.argv[1:]

def stable(path):
    rows = []
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        if line.startswith("@HD") or line.startswith("@RG") or not line.startswith("@"):
            rows.append(line)
    return rows

if stable(turbo_path) != stable(picard_path):
    raise SystemExit(f"FastqToSam auto quality output differs from Picard for {name}")
print(f"FastqToSam auto quality output matches Picard for {name}")
PY
done
