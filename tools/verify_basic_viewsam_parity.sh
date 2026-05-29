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
read-b	0	chr1	50	60	10M	*	0	0	BBBBBBBBBB	FFFFFFFFFF
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  SortSam "I=$workdir/input.sam" "O=$workdir/input.bam" SORT_ORDER=coordinate

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam "I=$workdir/input.bam" \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/turbo.sam"

"${conda_runner[@]}" run -p "$conda_prefix" picard ViewSam \
  "I=$workdir/input.bam" \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/picard.sam"

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam "I=$workdir/input.bam" \
  RECORDS_ONLY=true \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/turbo.records.sam"

"${conda_runner[@]}" run -p "$conda_prefix" picard ViewSam \
  "I=$workdir/input.bam" \
  RECORDS_ONLY=true \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/picard.records.sam"

python3 - "$workdir/turbo.sam" "$workdir/picard.sam" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def record_names(path):
    with open(path, encoding="utf-8") as handle:
        return [line.split("\t", 1)[0] for line in handle if not line.startswith("@")]

def sq_lines(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line.startswith("@SQ\t")]

if sq_lines(turbo_path) != sq_lines(picard_path):
    raise SystemExit("ViewSam sequence dictionary differs from Picard")
if record_names(turbo_path) != record_names(picard_path):
    raise SystemExit("ViewSam record order differs from Picard")
print("ViewSam basic SAM output matches Picard")
PY

python3 - "$workdir/turbo.records.sam" "$workdir/picard.records.sam" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line and not line.startswith("@")]

def header_lines(path):
    with open(path, encoding="utf-8") as handle:
        return [line for line in handle if line.startswith("@")]

if header_lines(turbo_path):
    raise SystemExit("ViewSam RECORDS_ONLY emitted header lines")
if records(turbo_path) != records(picard_path):
    raise SystemExit("ViewSam RECORDS_ONLY records differ from Picard")
print("ViewSam RECORDS_ONLY output matches Picard")
PY
