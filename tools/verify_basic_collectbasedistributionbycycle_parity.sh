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
@SQ	SN:chr1	LN:100
read-a	0	chr1	1	60	4M	*	0	0	ACGT	FFFF
read-b	0	chr1	2	60	4M	*	0	0	AAGT	FFFF
read-c	16	chr1	3	60	4M	*	0	0	NNGT	FFFF
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectBaseDistributionByCycle \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo.txt" \
  "CHART=$workdir/turbo.pdf" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectBaseDistributionByCycle \
  "I=$workdir/input.sam" \
  "O=$workdir/picard.txt" \
  "CHART=$workdir/picard.pdf" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.txt" "$workdir/picard.txt" <<'PY'
import sys

def stable(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line.startswith("READ_END\tCYCLE\t"):
            return [raw for raw in lines[index:] if raw]
    raise SystemExit(f"no base distribution table in {path}")

turbo = stable(sys.argv[1])
picard = stable(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"CollectBaseDistributionByCycle table differs:\nturbo={turbo}\npicard={picard}")
print("CollectBaseDistributionByCycle stable table matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectBaseDistributionByCycle \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-temp-options.txt" \
  "CHART=$workdir/turbo-temp-options.pdf" \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectBaseDistributionByCycle \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-temp-options.txt" \
  "CHART=$workdir/picard-temp-options.pdf" \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-temp-options.txt" "$workdir/picard-temp-options.txt" <<'PY'
import sys

def stable(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line.startswith("READ_END\tCYCLE\t"):
            return [raw for raw in lines[index:] if raw]
    raise SystemExit(f"no base distribution table in {path}")

turbo = stable(sys.argv[1])
picard = stable(sys.argv[2])
if turbo != picard:
    raise SystemExit(
        f"CollectBaseDistributionByCycle temp-option table differs:\n"
        f"turbo={turbo}\npicard={picard}"
    )
print("CollectBaseDistributionByCycle temp-option table matches Picard")
PY
