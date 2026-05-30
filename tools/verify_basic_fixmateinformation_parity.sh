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

cargo run -q -p turbo-picard-cli --bin picard -- \
  FixMateInformation \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-md5.sam" \
  ASSUME_SORTED=true \
  SORT_ORDER=queryname \
  CREATE_MD5_FILE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FixMateInformation \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-md5.sam" \
  ASSUME_SORTED=true \
  SORT_ORDER=queryname \
  CREATE_MD5_FILE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-md5.sam" "$workdir/picard-md5.sam" "$workdir/turbo-md5.sam.md5" "$workdir/picard-md5.sam.md5" <<'PY'
import sys

turbo_path, picard_path, turbo_md5_path, picard_md5_path = sys.argv[1:]

def stable_records(path):
    records = []
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        if not line or line.startswith("@PG"):
            continue
        records.append(line)
    return records

turbo = stable_records(turbo_path)
picard = stable_records(picard_path)
if turbo != picard:
    raise SystemExit(f"FixMateInformation MD5 SAM output differs:\nturbo={turbo}\npicard={picard}")
if open(turbo_md5_path, encoding="utf-8").read().strip() != open(picard_md5_path, encoding="utf-8").read().strip():
    raise SystemExit("FixMateInformation MD5 sidecar differs from Picard")
print("FixMateInformation MD5 sidecar matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  FixMateInformation \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-coordinate.bam" \
  ASSUME_SORTED=true \
  SORT_ORDER=coordinate \
  CREATE_INDEX=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FixMateInformation \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-coordinate.bam" \
  ASSUME_SORTED=true \
  SORT_ORDER=coordinate \
  CREATE_INDEX=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "$workdir/turbo-coordinate.bai"
test -s "$workdir/picard-coordinate.bai"

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam "I=$workdir/turbo-coordinate.bam" "O=$workdir/turbo-coordinate.sam" \
  VALIDATION_STRINGENCY=SILENT QUIET=true
cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam "I=$workdir/picard-coordinate.bam" "O=$workdir/picard-coordinate.sam" \
  VALIDATION_STRINGENCY=SILENT QUIET=true

python3 - "$workdir/turbo-coordinate.sam" "$workdir/picard-coordinate.sam" <<'PY'
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
    raise SystemExit(f"FixMateInformation coordinate BAM output differs:\nturbo={turbo}\npicard={picard}")
print("FixMateInformation coordinate BAM and index output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  FixMateInformation \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-unsorted.sam" \
  ASSUME_SORTED=true \
  SORT_ORDER=unsorted \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FixMateInformation \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-unsorted.sam" \
  ASSUME_SORTED=true \
  SORT_ORDER=unsorted \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-unsorted.sam" "$workdir/picard-unsorted.sam" <<'PY'
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
    raise SystemExit(f"FixMateInformation unsorted SAM output differs:\nturbo={turbo}\npicard={picard}")
print("FixMateInformation unsorted SAM output matches Picard")
PY

cat > "$workdir/supplementary.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
pair1	99	chr1	10	60	4M	*	0	0	ACGT	FFFF
pair1	2147	chr1	100	60	4M	*	0	0	GGGG	FFFF
pair1	147	chr1	30	60	4M	*	0	0	TGCA	FFFF
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  FixMateInformation \
  "I=$workdir/supplementary.sam" \
  "O=$workdir/turbo-supplementary.sam" \
  ASSUME_SORTED=true \
  SORT_ORDER=queryname \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard FixMateInformation \
  "I=$workdir/supplementary.sam" \
  "O=$workdir/picard-supplementary.sam" \
  ASSUME_SORTED=true \
  SORT_ORDER=queryname \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-supplementary.sam" "$workdir/picard-supplementary.sam" <<'PY'
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
    raise SystemExit(f"FixMateInformation supplementary SAM output differs:\nturbo={turbo}\npicard={picard}")
print("FixMateInformation supplementary SAM output matches Picard")
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
