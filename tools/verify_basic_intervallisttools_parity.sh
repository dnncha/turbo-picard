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

cat > "$workdir/first.interval_list" <<'EOF'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
chr1	1	10	+	a
chr1	11	20	+	b
chr1	30	40	+	c
EOF

cat > "$workdir/second.interval_list" <<'EOF'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
chr1	5	15	+	d
EOF

cargo run -q -p turbo-picard-cli --bin picard -- \
  IntervalListTools \
  "I=$workdir/first.interval_list" \
  "I=$workdir/second.interval_list" \
  "O=$workdir/turbo.interval_list" \
  ACTION=CONCAT \
  SORT=true \
  UNIQUE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard IntervalListTools \
  "I=$workdir/first.interval_list" \
  "I=$workdir/second.interval_list" \
  "O=$workdir/picard.interval_list" \
  ACTION=CONCAT \
  SORT=true \
  UNIQUE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.interval_list" "$workdir/picard.interval_list" <<'PY'
import sys

def stable_lines(path):
    lines = []
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        if line.startswith("@PG"):
            continue
        lines.append(line)
    return lines

turbo = stable_lines(sys.argv[1])
picard = stable_lines(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"IntervalListTools output differs:\nturbo={turbo}\npicard={picard}")
print("IntervalListTools stable interval_list output matches Picard")
PY

cat > "$workdir/abutting.interval_list" <<'EOF'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
chr1	1	10	+	a
chr1	11	20	+	b
chr1	21	25	+	c
chr1	30	40	+	d
chr1	35	45	+	e
EOF

cargo run -q -p turbo-picard-cli --bin picard -- \
  IntervalListTools \
  "I=$workdir/abutting.interval_list" \
  "O=$workdir/turbo-no-abutting.interval_list" \
  ACTION=CONCAT \
  SORT=true \
  UNIQUE=true \
  DONT_MERGE_ABUTTING=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard IntervalListTools \
  "I=$workdir/abutting.interval_list" \
  "O=$workdir/picard-no-abutting.interval_list" \
  ACTION=CONCAT \
  SORT=true \
  UNIQUE=true \
  DONT_MERGE_ABUTTING=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-no-abutting.interval_list" "$workdir/picard-no-abutting.interval_list" <<'PY'
import sys

def stable_lines(path):
    lines = []
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        if line.startswith("@PG"):
            continue
        lines.append(line)
    return lines

turbo = stable_lines(sys.argv[1])
picard = stable_lines(sys.argv[2])
if turbo != picard:
    raise SystemExit(f"IntervalListTools DONT_MERGE_ABUTTING output differs:\nturbo={turbo}\npicard={picard}")
print("IntervalListTools DONT_MERGE_ABUTTING output matches Picard")
PY
