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
read-b	0	chr1	50	60	10M	*	0	0	CCCCCCCCCC	FFFFFFFFFF
read-c	0	chr1	90	60	10M	*	0	0	GGGGGGGGGG	FFFFFFFFFF
read-d	512	chr1	120	60	10M	*	0	0	TTTTTTTTTT	FFFFFFFFFF
read-e	4	*	0	0	*	*	0	0	NNNN	!!!!
SAM

cat > "$workdir/targets.interval_list" <<'INTERVALS'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
chr1	45	60	+	target
INTERVALS

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

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam "I=$workdir/input.bam" \
  HEADER_ONLY=true \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/turbo.header.sam"

"${conda_runner[@]}" run -p "$conda_prefix" picard ViewSam \
  "I=$workdir/input.bam" \
  HEADER_ONLY=true \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/picard.header.sam"

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam "I=$workdir/input.bam" \
  "INTERVAL_LIST=$workdir/targets.interval_list" \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/turbo.interval.sam"

"${conda_runner[@]}" run -p "$conda_prefix" picard ViewSam \
  "I=$workdir/input.bam" \
  "INTERVAL_LIST=$workdir/targets.interval_list" \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/picard.interval.sam"

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam "I=$workdir/input.bam" \
  PF_STATUS=PF \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/turbo-pf.sam"

"${conda_runner[@]}" run -p "$conda_prefix" picard ViewSam \
  "I=$workdir/input.bam" \
  PF_STATUS=PF \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/picard-pf.sam"

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam "I=$workdir/input.bam" \
  PF_STATUS=NonPF \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/turbo-non-pf.sam"

"${conda_runner[@]}" run -p "$conda_prefix" picard ViewSam \
  "I=$workdir/input.bam" \
  PF_STATUS=NonPF \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/picard-non-pf.sam"

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam "I=$workdir/input.bam" \
  ALIGNMENT_STATUS=Aligned \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/turbo-aligned.sam"

"${conda_runner[@]}" run -p "$conda_prefix" picard ViewSam \
  "I=$workdir/input.bam" \
  ALIGNMENT_STATUS=Aligned \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/picard-aligned.sam"

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam "I=$workdir/input.bam" \
  ALIGNMENT_STATUS=Unaligned \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/turbo-unaligned.sam"

"${conda_runner[@]}" run -p "$conda_prefix" picard ViewSam \
  "I=$workdir/input.bam" \
  ALIGNMENT_STATUS=Unaligned \
  VALIDATION_STRINGENCY=SILENT QUIET=true \
  > "$workdir/picard-unaligned.sam"

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

python3 - "$workdir/turbo.header.sam" "$workdir/picard.header.sam" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def header_lines(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line.startswith("@")]

def records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line and not line.startswith("@")]

if records(turbo_path):
    raise SystemExit("ViewSam HEADER_ONLY emitted records")
if header_lines(turbo_path) != header_lines(picard_path):
    raise SystemExit("ViewSam HEADER_ONLY header differs from Picard")
print("ViewSam HEADER_ONLY output matches Picard")
PY

python3 - "$workdir/turbo.interval.sam" "$workdir/picard.interval.sam" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line and not line.startswith("@")]

if records(turbo_path) != records(picard_path):
    raise SystemExit("ViewSam INTERVAL_LIST records differ from Picard")
print("ViewSam INTERVAL_LIST filtering matches Picard")
PY

python3 - "$workdir/turbo-pf.sam" "$workdir/picard-pf.sam" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line and not line.startswith("@")]

if records(turbo_path) != records(picard_path):
    raise SystemExit("ViewSam PF_STATUS=PF records differ from Picard")
print("ViewSam PF_STATUS=PF filtering matches Picard")
PY

python3 - "$workdir/turbo-non-pf.sam" "$workdir/picard-non-pf.sam" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line and not line.startswith("@")]

if records(turbo_path) != records(picard_path):
    raise SystemExit("ViewSam PF_STATUS=NonPF records differ from Picard")
print("ViewSam PF_STATUS=NonPF filtering matches Picard")
PY

python3 - "$workdir/turbo-aligned.sam" "$workdir/picard-aligned.sam" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line and not line.startswith("@")]

if records(turbo_path) != records(picard_path):
    raise SystemExit("ViewSam ALIGNMENT_STATUS=Aligned records differ from Picard")
print("ViewSam ALIGNMENT_STATUS=Aligned filtering matches Picard")
PY

python3 - "$workdir/turbo-unaligned.sam" "$workdir/picard-unaligned.sam" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line and not line.startswith("@")]

if records(turbo_path) != records(picard_path):
    raise SystemExit("ViewSam ALIGNMENT_STATUS=Unaligned records differ from Picard")
print("ViewSam ALIGNMENT_STATUS=Unaligned filtering matches Picard")
PY
