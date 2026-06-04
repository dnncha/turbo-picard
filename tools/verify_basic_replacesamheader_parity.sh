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
read-a	0	chr1	10	60	10M	*	0	0	AAAAAAAAAA	FFFFFFFFFF
SAM

cat > "$workdir/header.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:2000
@CO	replacement header
SAM

cat > "$workdir/ref.fa" <<'FA'
>chr1
ACGTACGTACGT
FA

cargo run -q -p turbo-picard-cli --bin picard -- \
  ReplaceSamHeader "I=$workdir/input.sam" "O=$workdir/turbo.sam" "HEADER=$workdir/header.sam" \
  CREATE_MD5_FILE=true VALIDATION_STRINGENCY=SILENT QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard ReplaceSamHeader \
  "I=$workdir/input.sam" "O=$workdir/picard.sam" "HEADER=$workdir/header.sam" \
  CREATE_MD5_FILE=true VALIDATION_STRINGENCY=SILENT QUIET=true

python3 - "$workdir/turbo.sam" "$workdir/picard.sam" "$workdir/turbo.sam.md5" "$workdir/picard.sam.md5" <<'PY'
import sys
turbo_path, picard_path, turbo_md5_path, picard_md5_path = sys.argv[1:]

def header_lines(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line.startswith("@")]

def record_names(path):
    with open(path, encoding="utf-8") as handle:
        return [line.split("\t", 1)[0] for line in handle if not line.startswith("@")]

if header_lines(turbo_path) != header_lines(picard_path):
    raise SystemExit("ReplaceSamHeader header differs from Picard")
if record_names(turbo_path) != record_names(picard_path):
    raise SystemExit("ReplaceSamHeader records differ from Picard")
if open(turbo_md5_path, encoding="utf-8").read().strip() != open(picard_md5_path, encoding="utf-8").read().strip():
    raise SystemExit("ReplaceSamHeader MD5 sidecar differs from Picard")
print("ReplaceSamHeader basic output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  ReplaceSamHeader \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-runtime.sam" \
  "HEADER=$workdir/header.sam" \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  MAX_RECORDS_IN_RAM=1000 \
  "TMP_DIR=$workdir" \
  "REFERENCE_SEQUENCE=$workdir/ref.fa" \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard ReplaceSamHeader \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-runtime.sam" \
  "HEADER=$workdir/header.sam" \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  MAX_RECORDS_IN_RAM=1000 \
  "TMP_DIR=$workdir" \
  "REFERENCE_SEQUENCE=$workdir/ref.fa" \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-runtime.sam" "$workdir/picard-runtime.sam" "$workdir/turbo-runtime.sam.md5" "$workdir/picard-runtime.sam.md5" <<'PY'
import sys
turbo_path, picard_path, turbo_md5_path, picard_md5_path = sys.argv[1:]

def header_lines(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line.startswith("@")]

def record_names(path):
    with open(path, encoding="utf-8") as handle:
        return [line.split("\t", 1)[0] for line in handle if not line.startswith("@")]

if header_lines(turbo_path) != header_lines(picard_path):
    raise SystemExit("ReplaceSamHeader runtime header differs from Picard")
if record_names(turbo_path) != record_names(picard_path):
    raise SystemExit("ReplaceSamHeader runtime records differ from Picard")
if open(turbo_md5_path, encoding="utf-8").read().strip() != open(picard_md5_path, encoding="utf-8").read().strip():
    raise SystemExit("ReplaceSamHeader runtime MD5 sidecar differs from Picard")
print("ReplaceSamHeader runtime option output matches Picard")
PY

test ! -e "$workdir/turbo-runtime.sam.bai"
test ! -e "$workdir/turbo-runtime.bai"
test ! -e "$workdir/picard-runtime.sam.bai"
test ! -e "$workdir/picard-runtime.bai"
echo "ReplaceSamHeader runtime sidecar compatibility matches Picard"
