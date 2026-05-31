#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conda_prefix="${TURBO_PICARD_CONDA_PREFIX:-$repo_root/.conda-turbo-picard}"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

cat > "$workdir/Rscript" <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod +x "$workdir/Rscript"

if command -v mamba >/dev/null 2>&1; then
  conda_runner=(mamba)
elif command -v micromamba >/dev/null 2>&1; then
  conda_runner=(micromamba)
else
  echo "mamba or micromamba is required for Picard parity verification" >&2
  exit 127
fi

run_picard() {
  "${conda_runner[@]}" run -p "$conda_prefix" env "PATH=$workdir:$conda_prefix/bin:$PATH" picard "$@"
}

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

run_picard CollectInsertSizeMetrics \
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

run_picard CollectInsertSizeMetrics \
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

run_picard CollectInsertSizeMetrics \
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

run_picard CollectInsertSizeMetrics \
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

run_picard CollectInsertSizeMetrics \
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

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectInsertSizeMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-stop-after.txt" \
  "H=$workdir/turbo-stop-after.pdf" \
  STOP_AFTER=2 \
  AS=true \
  DEVIATIONS=5 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

run_picard CollectInsertSizeMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-stop-after.txt" \
  "H=$workdir/picard-stop-after.pdf" \
  STOP_AFTER=2 \
  AS=true \
  DEVIATIONS=5 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-stop-after.txt" "$workdir/picard-stop-after.txt" <<'PY'
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
    raise SystemExit(
        f"CollectInsertSizeMetrics STOP_AFTER/AS/DEVIATIONS output differs:\n"
        f"turbo={turbo}\npicard={picard}"
    )
print("CollectInsertSizeMetrics STOP_AFTER, AS alias, and DEVIATIONS output matches Picard")
PY

cat > "$workdir/sample-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
@RG	ID:rg1	SM:sampleA	LB:lib1	PL:ILLUMINA	PU:unit1
pair1	99	chr1	10	60	4M	=	30	24	ACGT	FFFF	RG:Z:rg1
pair1	147	chr1	30	60	4M	=	10	-24	TGCA	FFFF	RG:Z:rg1
pair2	99	chr1	100	60	4M	=	130	34	AAAA	FFFF	RG:Z:rg1
pair2	147	chr1	130	60	4M	=	100	-34	TTTT	FFFF	RG:Z:rg1
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectInsertSizeMetrics \
  "I=$workdir/sample-input.sam" \
  "O=$workdir/turbo-sample-level.txt" \
  "H=$workdir/turbo-sample-level.pdf" \
  METRIC_ACCUMULATION_LEVEL=SAMPLE \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

run_picard CollectInsertSizeMetrics \
  "I=$workdir/sample-input.sam" \
  "O=$workdir/picard-sample-level.txt" \
  "H=$workdir/picard-sample-level.pdf" \
  METRIC_ACCUMULATION_LEVEL=SAMPLE \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-sample-level.txt" "$workdir/picard-sample-level.txt" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]

def stable_sections(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metric_rows = []
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("MEDIAN_INSERT_SIZE\t"):
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                metric_rows.append(lines[cursor])
                cursor += 1
        if line.startswith("insert_size\t"):
            histogram.append(line)
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                histogram.append(lines[cursor])
                cursor += 1
    if not metric_rows:
        raise SystemExit(f"no insert-size metrics table in {path}")
    return metric_rows, histogram

turbo = stable_sections(turbo_path)
picard = stable_sections(picard_path)
if turbo != picard:
    raise SystemExit(f"CollectInsertSizeMetrics SAMPLE accumulation output differs:\nturbo={turbo}\npicard={picard}")
print("CollectInsertSizeMetrics SAMPLE accumulation output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectInsertSizeMetrics \
  "I=$workdir/sample-input.sam" \
  "O=$workdir/turbo-library-level.txt" \
  "H=$workdir/turbo-library-level.pdf" \
  METRIC_ACCUMULATION_LEVEL=LIBRARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

run_picard CollectInsertSizeMetrics \
  "I=$workdir/sample-input.sam" \
  "O=$workdir/picard-library-level.txt" \
  "H=$workdir/picard-library-level.pdf" \
  METRIC_ACCUMULATION_LEVEL=LIBRARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-library-level.txt" "$workdir/picard-library-level.txt" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]

def stable_sections(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metric_rows = []
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("MEDIAN_INSERT_SIZE\t"):
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                metric_rows.append(lines[cursor])
                cursor += 1
        if line.startswith("insert_size\t"):
            histogram.append(line)
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                histogram.append(lines[cursor])
                cursor += 1
    if not metric_rows:
        raise SystemExit(f"no insert-size metrics table in {path}")
    return metric_rows, histogram

turbo = stable_sections(turbo_path)
picard = stable_sections(picard_path)
if turbo != picard:
    raise SystemExit(f"CollectInsertSizeMetrics LIBRARY accumulation output differs:\nturbo={turbo}\npicard={picard}")
print("CollectInsertSizeMetrics LIBRARY accumulation output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectInsertSizeMetrics \
  "I=$workdir/sample-input.sam" \
  "O=$workdir/turbo-read-group-level.txt" \
  "H=$workdir/turbo-read-group-level.pdf" \
  METRIC_ACCUMULATION_LEVEL=READ_GROUP \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

run_picard CollectInsertSizeMetrics \
  "I=$workdir/sample-input.sam" \
  "O=$workdir/picard-read-group-level.txt" \
  "H=$workdir/picard-read-group-level.pdf" \
  METRIC_ACCUMULATION_LEVEL=READ_GROUP \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-read-group-level.txt" "$workdir/picard-read-group-level.txt" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]

def stable_sections(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metric_rows = []
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("MEDIAN_INSERT_SIZE\t"):
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                metric_rows.append(lines[cursor])
                cursor += 1
        if line.startswith("insert_size\t"):
            histogram.append(line)
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                histogram.append(lines[cursor])
                cursor += 1
    if not metric_rows:
        raise SystemExit(f"no insert-size metrics table in {path}")
    return metric_rows, histogram

turbo = stable_sections(turbo_path)
picard = stable_sections(picard_path)
if turbo != picard:
    raise SystemExit(f"CollectInsertSizeMetrics READ_GROUP accumulation output differs:\nturbo={turbo}\npicard={picard}")
print("CollectInsertSizeMetrics READ_GROUP accumulation output matches Picard")
PY
