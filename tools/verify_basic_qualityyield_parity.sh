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
read-a	0	chr1	10	60	4M	*	0	0	ACGT	FFFF
read-b	512	chr1	20	60	4M	*	0	0	NNNN	!!!!
read-c	256	chr1	30	60	4M	*	0	0	ACGT	FFFF
read-d	2048	chr1	40	60	4M	*	0	0	TGCA	EEEE
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectQualityYieldMetrics "I=$workdir/input.sam" "O=$workdir/turbo.txt" \
  VALIDATION_STRINGENCY=SILENT QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectQualityYieldMetrics \
  "I=$workdir/input.sam" "O=$workdir/picard.txt" \
  VALIDATION_STRINGENCY=SILENT QUIET=true

python3 - "$workdir/turbo.txt" "$workdir/picard.txt" <<'PY'
import sys

def stable_row(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line.startswith("TOTAL_READS\t"):
            return lines[index], lines[index + 1]
    raise SystemExit(f"no metrics table in {path}")

turbo = stable_row(sys.argv[1])
picard = stable_row(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"QualityYield stable metrics differ:\nturbo={turbo}\npicard={picard}")
print("CollectQualityYieldMetrics stable metrics match Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectQualityYieldMetrics "I=$workdir/input.sam" "O=$workdir/turbo-stop-after.txt" \
  STOP_AFTER=1 VALIDATION_STRINGENCY=SILENT QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectQualityYieldMetrics \
  "I=$workdir/input.sam" "O=$workdir/picard-stop-after.txt" \
  STOP_AFTER=1 VALIDATION_STRINGENCY=SILENT QUIET=true

python3 - "$workdir/turbo-stop-after.txt" "$workdir/picard-stop-after.txt" <<'PY'
import sys

def stable_row(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line.startswith("TOTAL_READS\t"):
            return lines[index], lines[index + 1]
    raise SystemExit(f"no metrics table in {path}")

turbo = stable_row(sys.argv[1])
picard = stable_row(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"QualityYield STOP_AFTER metrics differ:\nturbo={turbo}\npicard={picard}")
print("CollectQualityYieldMetrics STOP_AFTER metrics match Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectQualityYieldMetrics "I=$workdir/input.sam" "O=$workdir/turbo-include-non-primary.txt" \
  INCLUDE_SECONDARY_ALIGNMENTS=true INCLUDE_SUPPLEMENTAL_ALIGNMENTS=true \
  VALIDATION_STRINGENCY=SILENT QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectQualityYieldMetrics \
  "I=$workdir/input.sam" "O=$workdir/picard-include-non-primary.txt" \
  INCLUDE_SECONDARY_ALIGNMENTS=true INCLUDE_SUPPLEMENTAL_ALIGNMENTS=true \
  VALIDATION_STRINGENCY=SILENT QUIET=true

python3 - "$workdir/turbo-include-non-primary.txt" "$workdir/picard-include-non-primary.txt" <<'PY'
import sys

def stable_row(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line.startswith("TOTAL_READS\t"):
            return lines[index], lines[index + 1]
    raise SystemExit(f"no metrics table in {path}")

turbo = stable_row(sys.argv[1])
picard = stable_row(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"QualityYield non-primary inclusion metrics differ:\nturbo={turbo}\npicard={picard}")
print("CollectQualityYieldMetrics non-primary inclusion metrics match Picard")
PY
