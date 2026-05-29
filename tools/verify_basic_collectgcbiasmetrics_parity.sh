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

cat > "$workdir/ref.fa" <<'FA'
>low
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
>high
CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC
FA

cat > "$workdir/input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:low	LN:40
@SQ	SN:high	LN:40
low1	0	low	1	60	20M	*	0	0	AAAAAAAAAAAAAAAAAAAA	FFFFFFFFFFFFFFFFFFFF
high1	0	high	1	60	20M	*	0	0	CCCCCCCCCCCCCCCCCCCC	FFFFFFFFFFFFFFFFFFFF
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectGcBiasMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo.detail.txt" \
  "S=$workdir/turbo.summary.txt" \
  "CHART=$workdir/turbo.pdf" \
  "R=$workdir/ref.fa" \
  SCAN_WINDOW_SIZE=20 \
  MINIMUM_GENOME_FRACTION=0 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectGcBiasMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard.detail.txt" \
  "S=$workdir/picard.summary.txt" \
  "CHART=$workdir/picard.pdf" \
  "R=$workdir/ref.fa" \
  SCAN_WINDOW_SIZE=20 \
  MINIMUM_GENOME_FRACTION=0 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.detail.txt" "$workdir/picard.detail.txt" "$workdir/turbo.summary.txt" "$workdir/picard.summary.txt" <<'PY'
import sys

def table(path, header):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line == header:
            rows = []
            cursor = index
            while cursor < len(lines) and lines[cursor]:
                rows.append(lines[cursor])
                cursor += 1
            return rows
    raise SystemExit(f"missing table {header!r} in {path}")

detail_header = "ACCUMULATION_LEVEL\tREADS_USED\tGC\tWINDOWS\tREAD_STARTS\tMEAN_BASE_QUALITY\tNORMALIZED_COVERAGE\tERROR_BAR_WIDTH\tSAMPLE\tLIBRARY\tREAD_GROUP"
summary_header = "ACCUMULATION_LEVEL\tREADS_USED\tWINDOW_SIZE\tTOTAL_CLUSTERS\tALIGNED_READS\tAT_DROPOUT\tGC_DROPOUT\tGC_NC_0_19\tGC_NC_20_39\tGC_NC_40_59\tGC_NC_60_79\tGC_NC_80_100\tSAMPLE\tLIBRARY\tREAD_GROUP"

checks = [
    (sys.argv[1], sys.argv[2], detail_header, "CollectGcBiasMetrics detail"),
    (sys.argv[3], sys.argv[4], summary_header, "CollectGcBiasMetrics summary"),
]

for turbo_path, picard_path, header, label in checks:
    turbo = table(turbo_path, header)
    picard = table(picard_path, header)
    if turbo != picard:
        raise SystemExit(f"{label} stable table differs:\nturbo={turbo}\npicard={picard}")

print("CollectGcBiasMetrics stable metric tables match Picard")
PY

test -s "$workdir/turbo.pdf"

cat > "$workdir/duplicate-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:low	LN:40
@SQ	SN:high	LN:40
low1	0	low	1	60	20M	*	0	0	AAAAAAAAAAAAAAAAAAAA	FFFFFFFFFFFFFFFFFFFF
lowdup	1024	low	1	60	20M	*	0	0	AAAAAAAAAAAAAAAAAAAA	!!!!!!!!!!!!!!!!!!!!
high1	0	high	1	60	20M	*	0	0	CCCCCCCCCCCCCCCCCCCC	FFFFFFFFFFFFFFFFFFFF
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectGcBiasMetrics \
  "I=$workdir/duplicate-input.sam" \
  "O=$workdir/turbo-duplicates.detail.txt" \
  "S=$workdir/turbo-duplicates.summary.txt" \
  "CHART=$workdir/turbo-duplicates.pdf" \
  "R=$workdir/ref.fa" \
  SCAN_WINDOW_SIZE=20 \
  MINIMUM_GENOME_FRACTION=0 \
  ALSO_IGNORE_DUPLICATES=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectGcBiasMetrics \
  "I=$workdir/duplicate-input.sam" \
  "O=$workdir/picard-duplicates.detail.txt" \
  "S=$workdir/picard-duplicates.summary.txt" \
  "CHART=$workdir/picard-duplicates.pdf" \
  "R=$workdir/ref.fa" \
  SCAN_WINDOW_SIZE=20 \
  MINIMUM_GENOME_FRACTION=0 \
  ALSO_IGNORE_DUPLICATES=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-duplicates.detail.txt" "$workdir/picard-duplicates.detail.txt" "$workdir/turbo-duplicates.summary.txt" "$workdir/picard-duplicates.summary.txt" <<'PY'
import sys

def table(path, header):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line == header:
            rows = []
            cursor = index
            while cursor < len(lines) and lines[cursor]:
                rows.append(lines[cursor])
                cursor += 1
            return rows
    raise SystemExit(f"missing table {header!r} in {path}")

detail_header = "ACCUMULATION_LEVEL\tREADS_USED\tGC\tWINDOWS\tREAD_STARTS\tMEAN_BASE_QUALITY\tNORMALIZED_COVERAGE\tERROR_BAR_WIDTH\tSAMPLE\tLIBRARY\tREAD_GROUP"
summary_header = "ACCUMULATION_LEVEL\tREADS_USED\tWINDOW_SIZE\tTOTAL_CLUSTERS\tALIGNED_READS\tAT_DROPOUT\tGC_DROPOUT\tGC_NC_0_19\tGC_NC_20_39\tGC_NC_40_59\tGC_NC_60_79\tGC_NC_80_100\tSAMPLE\tLIBRARY\tREAD_GROUP"

checks = [
    (sys.argv[1], sys.argv[2], detail_header, "CollectGcBiasMetrics duplicate detail"),
    (sys.argv[3], sys.argv[4], summary_header, "CollectGcBiasMetrics duplicate summary"),
]

for turbo_path, picard_path, header, label in checks:
    turbo = table(turbo_path, header)
    picard = table(picard_path, header)
    if turbo != picard:
        raise SystemExit(f"{label} stable table differs:\nturbo={turbo}\npicard={picard}")

print("CollectGcBiasMetrics duplicate-filtered metric tables match Picard")
PY

test -s "$workdir/turbo-duplicates.pdf"
