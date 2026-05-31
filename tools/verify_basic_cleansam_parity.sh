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
@SQ	SN:chr1	LN:10
mapped	0	chr1	2	60	4M	*	0	0	ACGT	FFFF
overhang	0	chr1	8	60	5M	*	0	0	ACGTA	FFFFF
terminal-del	0	chr1	8	60	3M3D2M	*	0	0	ACGTA	FFFFF
unmapped	4	*	0	60	*	*	0	0	NNNN	!!!!
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  CleanSam \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CleanSam \
  "I=$workdir/input.sam" \
  "O=$workdir/picard.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.sam" "$workdir/picard.sam" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def records(path):
    data = {}
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("@"):
                continue
            fields = line.rstrip("\n").split("\t")
            data[fields[0]] = (fields[4], fields[5])
    return data

if records(turbo_path) != records(picard_path):
    raise SystemExit(f"CleanSam MAPQ/CIGAR differs:\nturbo={records(turbo_path)}\npicard={records(picard_path)}")
print("CleanSam basic MAPQ/CIGAR output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CleanSam \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-sidecars.bam" \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  COMPRESSION_LEVEL=5 \
  MAX_RECORDS_IN_RAM=500 \
  "TMP_DIR=$workdir" \
  VERBOSITY=WARNING \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CleanSam \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-sidecars.bam" \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  COMPRESSION_LEVEL=5 \
  MAX_RECORDS_IN_RAM=500 \
  "TMP_DIR=$workdir" \
  VERBOSITY=WARNING \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam \
  "I=$workdir/turbo-sidecars.bam" \
  "O=$workdir/turbo-sidecars.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam \
  "I=$workdir/picard-sidecars.bam" \
  "O=$workdir/picard-sidecars.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir" <<'PY'
import pathlib
import re
import sys

workdir = pathlib.Path(sys.argv[1])

def records(path):
    data = {}
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("@"):
                continue
            fields = line.rstrip("\n").split("\t")
            data[fields[0]] = (fields[4], fields[5])
    return data

turbo_records = records(workdir / "turbo-sidecars.sam")
picard_records = records(workdir / "picard-sidecars.sam")
if turbo_records != picard_records:
    raise SystemExit(
        f"CleanSam BAM MAPQ/CIGAR differs:\nturbo={turbo_records}\npicard={picard_records}"
    )

for name in [
    "turbo-sidecars.bam.md5",
    "picard-sidecars.bam.md5",
    "turbo-sidecars.bai",
    "picard-sidecars.bai",
]:
    path = workdir / name
    if not path.exists():
        raise SystemExit(f"missing CleanSam sidecar: {name}")

for name in ["turbo-sidecars.bam.md5", "picard-sidecars.bam.md5"]:
    value = (workdir / name).read_text(encoding="utf-8").strip()
    if not re.fullmatch(r"[0-9a-f]{32}", value):
        raise SystemExit(f"invalid CleanSam md5 sidecar {name}: {value!r}")

print("CleanSam runtime sidecars and BAM MAPQ/CIGAR output match Picard")
PY
