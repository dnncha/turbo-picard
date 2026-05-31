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

cat > "$workdir/ref.fa" <<'FASTA'
>chr1
ACGTACGTACGT
FASTA

cat > "$workdir/input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:12
read1	0	chr1	1	60	4M	*	0	0	ACGA	FFFF
read2	0	chr1	5	60	2M1I2M	*	0	0	ACGTA	FFFFF
read3	0	chr1	8	60	2M1D2M	*	0	0	TACG	FFFF
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  SetNmMdAndUqTags \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo.sam" \
  "R=$workdir/ref.fa" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard SetNmMdAndUqTags \
  "I=$workdir/input.sam" \
  "O=$workdir/picard.sam" \
  "R=$workdir/ref.fa" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

cat > "$workdir/input-existing-tags.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:12
read1	0	chr1	1	60	4M	*	0	0	ACGA	FFFF	MD:Z:keep	NM:i:99
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  SetNmMdAndUqTags \
  "I=$workdir/input-existing-tags.sam" \
  "O=$workdir/turbo-set-only-uq.sam" \
  "R=$workdir/ref.fa" \
  SET_ONLY_UQ=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard SetNmMdAndUqTags \
  "I=$workdir/input-existing-tags.sam" \
  "O=$workdir/picard-set-only-uq.sam" \
  "R=$workdir/ref.fa" \
  SET_ONLY_UQ=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

cargo run -q -p turbo-picard-cli --bin picard -- \
  SetNmMdAndUqTags \
  "I=$workdir/input-existing-tags.sam" \
  "O=$workdir/turbo-runtime.sam" \
  "R=$workdir/ref.fa" \
  CREATE_INDEX=true \
  CREATE_MD5_FILE=true \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard SetNmMdAndUqTags \
  "I=$workdir/input-existing-tags.sam" \
  "O=$workdir/picard-runtime.sam" \
  "R=$workdir/ref.fa" \
  CREATE_INDEX=true \
  CREATE_MD5_FILE=true \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
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
    raise SystemExit(f"SetNmMdAndUqTags SAM output differs:\nturbo={turbo}\npicard={picard}")
print("SetNmMdAndUqTags stable SAM output matches Picard")
PY

python3 - "$workdir/turbo-set-only-uq.sam" "$workdir/picard-set-only-uq.sam" <<'PY'
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
    raise SystemExit(f"SetNmMdAndUqTags SET_ONLY_UQ output differs:\nturbo={turbo}\npicard={picard}")
print("SetNmMdAndUqTags SET_ONLY_UQ output matches Picard")
PY

python3 - "$workdir" <<'PY'
import pathlib
import sys

workdir = pathlib.Path(sys.argv[1])
expected = [
    "turbo-runtime.sam.md5",
    "picard-runtime.sam.md5",
]
missing = [name for name in expected if not (workdir / name).exists()]
if missing:
    raise SystemExit(f"missing SetNmMdAndUqTags md5 sidecars: {missing}")
unexpected = [
    "turbo-runtime.sam.bai",
    "picard-runtime.sam.bai",
    "turbo-runtime.sam.idx",
    "picard-runtime.sam.idx",
]
present = [name for name in unexpected if (workdir / name).exists()]
if present:
    raise SystemExit(f"unexpected SetNmMdAndUqTags index sidecars: {present}")
print("SetNmMdAndUqTags runtime sidecar behavior matches Picard")
PY
