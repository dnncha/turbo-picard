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
pair1	99	chr1	10	60	4M	=	30	24	ACGT	FFFF
pair1	147	chr1	30	60	4M	=	10	-24	TGCA	FFFF
pair2	99	chr1	100	60	4M	=	130	34	AAAA	FFFF
pair2	147	chr1	130	60	4M	=	100	-34	TTTT	FFFF
dup1	1123	chr1	200	60	4M	=	240	44	CCCC	FFFF
dup1	1171	chr1	240	60	4M	=	200	-44	GGGG	FFFF
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectInsertSizeMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo.txt" \
  "H=$workdir/turbo.pdf" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectInsertSizeMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard.txt" \
  "H=$workdir/picard.pdf" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.txt" "$workdir/picard.txt" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]

def stable_sections(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("MEDIAN_INSERT_SIZE\t"):
            metrics = (line, lines[index + 1])
        if line == "insert_size\tAll_Reads.fr_count":
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                histogram.append(lines[cursor])
                cursor += 1
    if metrics is None:
        raise SystemExit(f"no insert-size metrics table in {path}")
    return metrics, histogram

turbo = stable_sections(turbo_path)
picard = stable_sections(picard_path)
if turbo != picard:
    raise SystemExit(f"CollectInsertSizeMetrics stable output differs:\nturbo={turbo}\npicard={picard}")
print("CollectInsertSizeMetrics stable metrics and histogram match Picard")
PY

test -s "$workdir/turbo.pdf"

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectInsertSizeMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-include-duplicates.txt" \
  "H=$workdir/turbo-include-duplicates.pdf" \
  INCLUDE_DUPLICATES=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectInsertSizeMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-include-duplicates.txt" \
  "H=$workdir/picard-include-duplicates.pdf" \
  INCLUDE_DUPLICATES=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-include-duplicates.txt" "$workdir/picard-include-duplicates.txt" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]

def stable_sections(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("MEDIAN_INSERT_SIZE\t"):
            metrics = (line, lines[index + 1])
        if line == "insert_size\tAll_Reads.fr_count":
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                histogram.append(lines[cursor])
                cursor += 1
    if metrics is None:
        raise SystemExit(f"no insert-size metrics table in {path}")
    return metrics, histogram

turbo = stable_sections(turbo_path)
picard = stable_sections(picard_path)
if turbo != picard:
    raise SystemExit(f"CollectInsertSizeMetrics INCLUDE_DUPLICATES output differs:\nturbo={turbo}\npicard={picard}")
print("CollectInsertSizeMetrics INCLUDE_DUPLICATES output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectInsertSizeMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-minimum-pct-alias.txt" \
  "H=$workdir/turbo-minimum-pct-alias.pdf" \
  M=0.5 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectInsertSizeMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-minimum-pct-alias.txt" \
  "H=$workdir/picard-minimum-pct-alias.pdf" \
  M=0.5 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-minimum-pct-alias.txt" "$workdir/picard-minimum-pct-alias.txt" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]

def stable_sections(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("MEDIAN_INSERT_SIZE\t"):
            metrics = (line, lines[index + 1])
        if line == "insert_size\tAll_Reads.fr_count":
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                histogram.append(lines[cursor])
                cursor += 1
    if metrics is None:
        raise SystemExit(f"no insert-size metrics table in {path}")
    return metrics, histogram

turbo = stable_sections(turbo_path)
picard = stable_sections(picard_path)
if turbo != picard:
    raise SystemExit(f"CollectInsertSizeMetrics M alias output differs:\nturbo={turbo}\npicard={picard}")
print("CollectInsertSizeMetrics M alias output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectInsertSizeMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-level.txt" \
  "H=$workdir/turbo-level.pdf" \
  LEVEL=ALL_READS \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectInsertSizeMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-level.txt" \
  "H=$workdir/picard-level.pdf" \
  LEVEL=ALL_READS \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-level.txt" "$workdir/picard-level.txt" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]

def stable_sections(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("MEDIAN_INSERT_SIZE\t"):
            metrics = (line, lines[index + 1])
        if line == "insert_size\tAll_Reads.fr_count":
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                histogram.append(lines[cursor])
                cursor += 1
    if metrics is None:
        raise SystemExit(f"no insert-size metrics table in {path}")
    return metrics, histogram

turbo = stable_sections(turbo_path)
picard = stable_sections(picard_path)
if turbo != picard:
    raise SystemExit(f"CollectInsertSizeMetrics LEVEL output differs:\nturbo={turbo}\npicard={picard}")
print("CollectInsertSizeMetrics LEVEL output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectInsertSizeMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-temp-options.txt" \
  "H=$workdir/turbo-temp-options.pdf" \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectInsertSizeMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-temp-options.txt" \
  "H=$workdir/picard-temp-options.pdf" \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-temp-options.txt" "$workdir/picard-temp-options.txt" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]

def stable_sections(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("MEDIAN_INSERT_SIZE\t"):
            metrics = (line, lines[index + 1])
        if line == "insert_size\tAll_Reads.fr_count":
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                histogram.append(lines[cursor])
                cursor += 1
    if metrics is None:
        raise SystemExit(f"no insert-size metrics table in {path}")
    return metrics, histogram

turbo = stable_sections(turbo_path)
picard = stable_sections(picard_path)
if turbo != picard:
    raise SystemExit(f"CollectInsertSizeMetrics temp-option output differs:\nturbo={turbo}\npicard={picard}")
print("CollectInsertSizeMetrics temp-option output matches Picard")
PY
