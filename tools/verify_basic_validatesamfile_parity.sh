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

cat > "$workdir/missing-platform.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:100
@RG	ID:rg1	SM:sample
read1	0	chr1	1	60	4M	*	0	0	ACGT	FFFF	RG:Z:rg1	NM:i:0
SAM

cat > "$workdir/invalid-mapq.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:100
@RG	ID:rg1	SM:sample	PL:ILLUMINA
read1	4	*	0	60	*	*	0	0	ACGT	FFFF	RG:Z:rg1
SAM

cat > "$workdir/paired.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:100
@RG	ID:rg1	SM:sample	PL:ILLUMINA
pair1	99	chr1	1	60	4M	=	11	14	ACGT	FFFF	RG:Z:rg1	NM:i:0
pair1	147	chr1	11	60	4M	=	1	-14	TGCA	FFFF	RG:Z:rg1	NM:i:0
SAM

cat > "$workdir/orphan.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:100
@RG	ID:rg1	SM:sample	PL:ILLUMINA
orphan1	99	chr1	1	60	4M	=	11	14	ACGT	FFFF	RG:Z:rg1	NM:i:0
SAM

cat > "$workdir/ref.fa" <<'FASTA'
>chr1
ACGTACGTACGT
FASTA

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

cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/orphan.sam" \
  "O=$workdir/turbo-orphan-skip-mate.txt" \
  MODE=SUMMARY \
  SKIP_MATE_VALIDATION=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/orphan.sam" \
  "O=$workdir/picard-orphan-skip-mate.txt" \
  MODE=SUMMARY \
  SKIP_MATE_VALIDATION=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/valid.sam" \
  "O=$workdir/turbo-runtime.txt" \
  MODE=SUMMARY \
  "R=$workdir/ref.fa" \
  CREATE_INDEX=true \
  CREATE_MD5_FILE=true \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  COMPRESSION_LEVEL=5 \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/valid.sam" \
  "O=$workdir/picard-runtime.txt" \
  MODE=SUMMARY \
  "R=$workdir/ref.fa" \
  CREATE_INDEX=true \
  CREATE_MD5_FILE=true \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  COMPRESSION_LEVEL=5 \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
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
  "I=$workdir/missing-platform.sam" \
  MODE=SUMMARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/turbo-missing-platform.txt"
turbo_missing_platform_status=$?

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/missing-platform.sam" \
  MODE=SUMMARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/picard-missing-platform.txt"
picard_missing_platform_status=$?

cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/missing-platform.sam" \
  MODE=SUMMARY \
  IGNORE=MISSING_PLATFORM_VALUE \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/turbo-ignore-missing-platform.txt"
turbo_ignore_missing_platform_status=$?

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/missing-platform.sam" \
  MODE=SUMMARY \
  IGNORE=MISSING_PLATFORM_VALUE \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/picard-ignore-missing-platform.txt"
picard_ignore_missing_platform_status=$?

cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/invalid-mapq.sam" \
  MODE=SUMMARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/turbo-invalid-mapq.txt"
turbo_invalid_mapq_status=$?

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/invalid-mapq.sam" \
  MODE=SUMMARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/picard-invalid-mapq.txt"
picard_invalid_mapq_status=$?

cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/invalid-mapq.sam" \
  MODE=SUMMARY \
  IGNORE=INVALID_MAPPING_QUALITY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/turbo-ignore-invalid-mapq.txt"
turbo_ignore_invalid_mapq_status=$?

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/invalid-mapq.sam" \
  MODE=SUMMARY \
  IGNORE=INVALID_MAPPING_QUALITY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/picard-ignore-invalid-mapq.txt"
picard_ignore_invalid_mapq_status=$?

cargo run -q -p turbo-picard-cli --bin picard -- \
  ValidateSamFile \
  "I=$workdir/orphan.sam" \
  MODE=SUMMARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/turbo-orphan.txt"
turbo_orphan_status=$?

"${conda_runner[@]}" run -p "$conda_prefix" picard ValidateSamFile \
  "I=$workdir/orphan.sam" \
  MODE=SUMMARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  > "$workdir/picard-orphan.txt"
picard_orphan_status=$?

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
if [[ "$turbo_missing_platform_status" -eq 0 || "$picard_missing_platform_status" -eq 0 ]]; then
  echo "ValidateSamFile missing-platform fixture should fail for both implementations" >&2
  exit 1
fi
if [[ "$turbo_ignore_missing_platform_status" -ne 0 || "$picard_ignore_missing_platform_status" -ne 0 ]]; then
  echo "ValidateSamFile ignored missing-platform fixture should pass for both implementations" >&2
  exit 1
fi
if [[ "$turbo_invalid_mapq_status" -eq 0 || "$picard_invalid_mapq_status" -eq 0 ]]; then
  echo "ValidateSamFile invalid-MAPQ fixture should fail for both implementations" >&2
  exit 1
fi
if [[ "$turbo_ignore_invalid_mapq_status" -ne 0 || "$picard_ignore_invalid_mapq_status" -ne 0 ]]; then
  echo "ValidateSamFile ignored invalid-MAPQ fixture should pass for both implementations" >&2
  exit 1
fi
if [[ "$turbo_orphan_status" -eq 0 || "$picard_orphan_status" -eq 0 ]]; then
  echo "ValidateSamFile orphan paired-read fixture should fail for both implementations" >&2
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
  "$workdir/turbo-orphan-skip-mate.txt" \
  "$workdir/picard-orphan-skip-mate.txt" \
  "$workdir/turbo-warning.txt" \
  "$workdir/picard-warning.txt" \
  "$workdir/turbo-ignore-missing-nm.txt" \
  "$workdir/picard-ignore-missing-nm.txt" \
  "$workdir/turbo-ignore-warning.txt" \
  "$workdir/picard-ignore-warning.txt" \
  "$workdir/turbo-missing-platform.txt" \
  "$workdir/picard-missing-platform.txt" \
  "$workdir/turbo-ignore-missing-platform.txt" \
  "$workdir/picard-ignore-missing-platform.txt" \
  "$workdir/turbo-invalid-mapq.txt" \
  "$workdir/picard-invalid-mapq.txt" \
  "$workdir/turbo-ignore-invalid-mapq.txt" \
  "$workdir/picard-ignore-invalid-mapq.txt" \
  "$workdir/turbo-orphan.txt" \
  "$workdir/picard-orphan.txt" \
  "$workdir/turbo-verbose-warning.txt" \
  "$workdir/picard-verbose-warning.txt" \
  "$workdir/turbo-verbose-max-output.txt" \
  "$workdir/picard-verbose-max-output.txt" \
  "$workdir/turbo-runtime.txt" \
  "$workdir/picard-runtime.txt" \
  "$workdir" <<'PY'
import sys
from pathlib import Path

(
    turbo_valid,
    picard_valid,
    turbo_paired,
    picard_paired,
    turbo_orphan_skip_mate,
    picard_orphan_skip_mate,
    turbo_warning,
    picard_warning,
    turbo_ignore_missing_nm,
    picard_ignore_missing_nm,
    turbo_ignore_warning,
    picard_ignore_warning,
    turbo_missing_platform,
    picard_missing_platform,
    turbo_ignore_missing_platform,
    picard_ignore_missing_platform,
    turbo_invalid_mapq,
    picard_invalid_mapq,
    turbo_ignore_invalid_mapq,
    picard_ignore_invalid_mapq,
    turbo_orphan,
    picard_orphan,
    turbo_verbose_warning,
    picard_verbose_warning,
    turbo_verbose_max_output,
    picard_verbose_max_output,
    turbo_runtime,
    picard_runtime,
    workdir,
) = sys.argv[1:]
workdir = Path(workdir)

def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()

if read(turbo_valid) != read(picard_valid):
    raise SystemExit("ValidateSamFile valid summary differs from Picard")
if read(turbo_paired) != read(picard_paired):
    raise SystemExit("ValidateSamFile paired-record summary differs from Picard")
if read(turbo_orphan_skip_mate) != read(picard_orphan_skip_mate):
    raise SystemExit("ValidateSamFile SKIP_MATE_VALIDATION summary differs from Picard")
if read(turbo_warning) != read(picard_warning):
    raise SystemExit("ValidateSamFile warning summary differs from Picard")
if read(turbo_ignore_missing_nm) != read(picard_ignore_missing_nm):
    raise SystemExit("ValidateSamFile ignored missing-NM summary differs from Picard")
if read(turbo_ignore_warning) != read(picard_ignore_warning):
    raise SystemExit("ValidateSamFile ignored warning summary differs from Picard")
if read(turbo_missing_platform) != read(picard_missing_platform):
    raise SystemExit("ValidateSamFile missing-platform summary differs from Picard")
if read(turbo_ignore_missing_platform) != read(picard_ignore_missing_platform):
    raise SystemExit("ValidateSamFile ignored missing-platform summary differs from Picard")
if read(turbo_invalid_mapq) != read(picard_invalid_mapq):
    raise SystemExit("ValidateSamFile invalid-MAPQ summary differs from Picard")
if read(turbo_ignore_invalid_mapq) != read(picard_ignore_invalid_mapq):
    raise SystemExit("ValidateSamFile ignored invalid-MAPQ summary differs from Picard")
if read(turbo_orphan) != read(picard_orphan):
    raise SystemExit("ValidateSamFile orphan mate summary differs from Picard")
if read(turbo_verbose_warning) != read(picard_verbose_warning):
    raise SystemExit("ValidateSamFile verbose warning output differs from Picard")
if read(turbo_verbose_max_output) != read(picard_verbose_max_output):
    raise SystemExit("ValidateSamFile verbose MAX_OUTPUT differs from Picard")
if read(turbo_runtime) != read(picard_runtime):
    raise SystemExit("ValidateSamFile runtime summary differs from Picard")
unexpected = [
    "turbo-runtime.txt.md5",
    "picard-runtime.txt.md5",
    "turbo-runtime.txt.idx",
    "picard-runtime.txt.idx",
]
present = [name for name in unexpected if (workdir / name).exists()]
if present:
    raise SystemExit(f"unexpected ValidateSamFile runtime sidecars: {present}")
print("ValidateSamFile basic SUMMARY, VERBOSE, MAX_OUTPUT, mate validation, missing-platform, invalid-MAPQ, and runtime output matches Picard")
PY
