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

cat > "$workdir/valid.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:100
@RG	ID:rg1	SM:sample	PL:ILLUMINA
read1	0	chr1	1	60	4M	*	0	0	ACGT	FFFF	RG:Z:rg1	NM:i:0
SAM

cat > "$workdir/warning.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:100
read1	0	chr1	1	60	4M	*	0	0	ACGT	FFFF
SAM

cat > "$workdir/missing-nm.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:100
@RG	ID:rg1	SM:sample	PL:ILLUMINA
read1	0	chr1	1	60	4M	*	0	0	ACGT	FFFF	RG:Z:rg1
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/valid.sam" \
  "O=$workdir/turbo-valid.txt" \
  MODE=SUMMARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/valid.sam" \
  "O=$workdir/picard-valid.txt" \
  MODE=SUMMARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

set +e
cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/warning.sam" \
  MODE=SUMMARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/turbo-warning.txt"
turbo_status=$?

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/warning.sam" \
  MODE=SUMMARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/picard-warning.txt"
picard_status=$?

cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/missing-nm.sam" \
  MODE=SUMMARY \
  IGNORE=MISSING_TAG_NM \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/turbo-ignore-missing-nm.txt"
turbo_ignore_status=$?

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/missing-nm.sam" \
  MODE=SUMMARY \
  IGNORE=MISSING_TAG_NM \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/picard-ignore-missing-nm.txt"
picard_ignore_status=$?

cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/warning.sam" \
  MODE=SUMMARY \
  IGNORE=MISSING_TAG_NM \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/turbo-ignore-warning.txt"
turbo_ignore_warning_status=$?

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/warning.sam" \
  MODE=SUMMARY \
  IGNORE=MISSING_TAG_NM \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/picard-ignore-warning.txt"
picard_ignore_warning_status=$?
set -e

if [[ "$turbo_status" -eq 0 || "$picard_status" -eq 0 ]]; then
  echo "ValidateSamFile warning fixture should fail for both implementations" >&2
  exit 1
fi
if [[ "$turbo_ignore_status" -ne 0 || "$picard_ignore_status" -ne 0 ]]; then
  echo "ValidateSamFile ignored missing-NM fixture should pass for both implementations" >&2
  exit 1
fi
if [[ "$turbo_ignore_warning_status" -eq 0 || "$picard_ignore_warning_status" -eq 0 ]]; then
  echo "ValidateSamFile warning fixture with ignored NM should still fail for both implementations" >&2
  exit 1
fi

python3 - \
  "$workdir/turbo-valid.txt" \
  "$workdir/picard-valid.txt" \
  "$workdir/turbo-warning.txt" \
  "$workdir/picard-warning.txt" \
  "$workdir/turbo-ignore-missing-nm.txt" \
  "$workdir/picard-ignore-missing-nm.txt" \
  "$workdir/turbo-ignore-warning.txt" \
  "$workdir/picard-ignore-warning.txt" <<'PY'
import sys

(
    turbo_valid,
    picard_valid,
    turbo_warning,
    picard_warning,
    turbo_ignore_missing_nm,
    picard_ignore_missing_nm,
    turbo_ignore_warning,
    picard_ignore_warning,
) = sys.argv[1:]

def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()

if read(turbo_valid) != read(picard_valid):
    raise SystemExit("ValidateSamFile valid summary differs from Picard")
if read(turbo_warning) != read(picard_warning):
    raise SystemExit("ValidateSamFile warning summary differs from Picard")
if read(turbo_ignore_missing_nm) != read(picard_ignore_missing_nm):
    raise SystemExit("ValidateSamFile ignored missing-NM summary differs from Picard")
if read(turbo_ignore_warning) != read(picard_ignore_warning):
    raise SystemExit("ValidateSamFile ignored warning summary differs from Picard")
print("ValidateSamFile basic SUMMARY output matches Picard")
PY
