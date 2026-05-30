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

cat > "$workdir/multi-warning.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:100
read1	0	chr1	1	60	4M	*	0	0	ACGT	FFFF
read2	0	chr1	2	60	4M	*	0	0	TGCA	FFFF
SAM

cat > "$workdir/missing-nm.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:100
@RG	ID:rg1	SM:sample	PL:ILLUMINA
read1	0	chr1	1	60	4M	*	0	0	ACGT	FFFF	RG:Z:rg1
SAM

cat > "$workdir/paired.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:100
@RG	ID:rg1	SM:sample	PL:ILLUMINA
pair1	99	chr1	1	60	4M	=	11	14	ACGT	FFFF	RG:Z:rg1	NM:i:0
pair1	147	chr1	11	60	4M	=	1	-14	TGCA	FFFF	RG:Z:rg1	NM:i:0
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

cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/paired.sam" \
  "O=$workdir/turbo-paired.txt" \
  MODE=SUMMARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/paired.sam" \
  "O=$workdir/picard-paired.txt" \
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

cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/warning.sam" \
  MODE=VERBOSE \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/turbo-verbose-warning.txt"
turbo_verbose_status=$?

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/warning.sam" \
  MODE=VERBOSE \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/picard-verbose-warning.txt"
picard_verbose_status=$?

cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/multi-warning.sam" \
  MODE=VERBOSE \
  MAX_OUTPUT=2 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/turbo-verbose-max-output.txt"
turbo_verbose_max_output_status=$?

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/multi-warning.sam" \
  MODE=VERBOSE \
  MAX_OUTPUT=2 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/picard-verbose-max-output.txt"
picard_verbose_max_output_status=$?
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
if [[ "$turbo_verbose_status" -eq 0 || "$picard_verbose_status" -eq 0 ]]; then
  echo "ValidateSamFile verbose warning fixture should fail for both implementations" >&2
  exit 1
fi
if [[ "$turbo_verbose_max_output_status" -eq 0 || "$picard_verbose_max_output_status" -eq 0 ]]; then
  echo "ValidateSamFile verbose MAX_OUTPUT fixture should fail for both implementations" >&2
  exit 1
fi

python3 - \
  "$workdir/turbo-valid.txt" \
  "$workdir/picard-valid.txt" \
  "$workdir/turbo-paired.txt" \
  "$workdir/picard-paired.txt" \
  "$workdir/turbo-warning.txt" \
  "$workdir/picard-warning.txt" \
  "$workdir/turbo-ignore-missing-nm.txt" \
  "$workdir/picard-ignore-missing-nm.txt" \
  "$workdir/turbo-ignore-warning.txt" \
  "$workdir/picard-ignore-warning.txt" \
  "$workdir/turbo-verbose-warning.txt" \
  "$workdir/picard-verbose-warning.txt" \
  "$workdir/turbo-verbose-max-output.txt" \
  "$workdir/picard-verbose-max-output.txt" <<'PY'
import sys

(
    turbo_valid,
    picard_valid,
    turbo_paired,
    picard_paired,
    turbo_warning,
    picard_warning,
    turbo_ignore_missing_nm,
    picard_ignore_missing_nm,
    turbo_ignore_warning,
    picard_ignore_warning,
    turbo_verbose_warning,
    picard_verbose_warning,
    turbo_verbose_max_output,
    picard_verbose_max_output,
) = sys.argv[1:]

def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()

if read(turbo_valid) != read(picard_valid):
    raise SystemExit("ValidateSamFile valid summary differs from Picard")
if read(turbo_paired) != read(picard_paired):
    raise SystemExit("ValidateSamFile paired-record summary differs from Picard")
if read(turbo_warning) != read(picard_warning):
    raise SystemExit("ValidateSamFile warning summary differs from Picard")
if read(turbo_ignore_missing_nm) != read(picard_ignore_missing_nm):
    raise SystemExit("ValidateSamFile ignored missing-NM summary differs from Picard")
if read(turbo_ignore_warning) != read(picard_ignore_warning):
    raise SystemExit("ValidateSamFile ignored warning summary differs from Picard")
if read(turbo_verbose_warning) != read(picard_verbose_warning):
    raise SystemExit("ValidateSamFile verbose warning output differs from Picard")
if read(turbo_verbose_max_output) != read(picard_verbose_max_output):
    raise SystemExit("ValidateSamFile verbose MAX_OUTPUT differs from Picard")
print("ValidateSamFile basic SUMMARY, VERBOSE, and MAX_OUTPUT output matches Picard")
PY
