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
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
pair1	99	chr1	10	60	4M	*	0	0	ACGT	FFFF
pair1	147	chr1	30	60	4M	*	0	0	TGCA	FFFF
single	0	chr1	50	60	4M	*	0	0	AAAA	FFFF
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  FixMateInformation \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo.sam" \
  ASSUME_SORTED=true \
  SORT_ORDER=queryname \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FixMateInformation \
  "I=$workdir/input.sam" \
  "O=$workdir/picard.sam" \
  ASSUME_SORTED=true \
  SORT_ORDER=queryname \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.sam" "$workdir/picard.sam" <<'PY'
import sys

def stable_records(path):
    records = []
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        if not line or line.startswith("@PG"):
            continue
        records.append(line)
    return records

turbo = stable_records(sys.argv[1])
picard = stable_records(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"FixMateInformation SAM output differs:\nturbo={turbo}\npicard={picard}")
print("FixMateInformation stable SAM output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  FixMateInformation \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-temp-options.sam" \
  ASSUME_SORTED=true \
  SORT_ORDER=queryname \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FixMateInformation \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-temp-options.sam" \
  ASSUME_SORTED=true \
  SORT_ORDER=queryname \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-temp-options.sam" "$workdir/picard-temp-options.sam" <<'PY'
import sys

def stable_records(path):
    records = []
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        if not line or line.startswith("@PG"):
            continue
        records.append(line)
    return records

turbo = stable_records(sys.argv[1])
picard = stable_records(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"FixMateInformation temp-option SAM output differs:\nturbo={turbo}\npicard={picard}")
print("FixMateInformation temp-option SAM output matches Picard")
PY

cat > "$workdir/missing-mate.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
single	99	chr1	10	60	4M	=	30	24	ACGT	FFFF
SAM

if cargo run -q -p turbo-picard-cli --bin picard -- \
  FixMateInformation \
  "I=$workdir/missing-mate.sam" \
  "O=$workdir/turbo-missing.sam" \
  ASSUME_SORTED=true \
  SORT_ORDER=queryname \
  IGNORE_MISSING_MATES=false \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true 2>"$workdir/turbo-missing.err"; then
  echo "FixMateInformation unexpectedly accepted missing mate" >&2
  exit 1
fi

if "${conda_runner[@]}" run -p "$conda_prefix" picard FixMateInformation \
  "I=$workdir/missing-mate.sam" \
  "O=$workdir/picard-missing.sam" \
  ASSUME_SORTED=true \
  SORT_ORDER=queryname \
  IGNORE_MISSING_MATES=false \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true 2>"$workdir/picard-missing.err"; then
  echo "Picard unexpectedly accepted missing mate" >&2
  exit 1
fi

grep -q "Missing second read of pair: single" "$workdir/turbo-missing.err"
grep -q "Missing second read of pair: single" "$workdir/picard-missing.err"
echo "FixMateInformation missing mate failure matches Picard"
