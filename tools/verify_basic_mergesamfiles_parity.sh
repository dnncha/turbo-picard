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

cat > "$workdir/input-a.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-c	0	chr1	90	60	10M	*	0	0	CCCCCCCCCC	FFFFFFFFFF
SAM

cat > "$workdir/input-b.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-a	0	chr1	10	60	10M	*	0	0	AAAAAAAAAA	FFFFFFFFFF
read-b	0	chr1	50	60	10M	*	0	0	BBBBBBBBBB	FFFFFFFFFF
SAM

for sort_order in coordinate queryname unsorted; do
  assume_sorted_arg=""
  if [[ "$sort_order" == "coordinate" ]]; then
    assume_sorted_arg="AS=true"
  fi

  cargo run -q -p turbo-picard-cli --bin picard -- \
    MergeSamFiles \
    "I=$workdir/input-a.sam" \
    "I=$workdir/input-b.sam" \
    "O=$workdir/turbo-$sort_order.sam" \
    "SORT_ORDER=$sort_order" \
    ${assume_sorted_arg:+"$assume_sorted_arg"} \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true

  "${conda_runner[@]}" run -p "$conda_prefix" picard MergeSamFiles \
    "I=$workdir/input-a.sam" \
    "I=$workdir/input-b.sam" \
    "O=$workdir/picard-$sort_order.sam" \
    "SORT_ORDER=$sort_order" \
    ${assume_sorted_arg:+"$assume_sorted_arg"} \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true

  python3 - "$sort_order" "$workdir/turbo-$sort_order.sam" "$workdir/picard-$sort_order.sam" <<'PY'
import sys
sort_order, turbo_path, picard_path = sys.argv[1:]

def header_sort_order(path):
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("@HD\t"):
                for field in line.rstrip("\n").split("\t")[1:]:
                    if field.startswith("SO:"):
                        return field[3:]
    return None

def record_names(path):
    names = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if not line.startswith("@"):
                names.append(line.split("\t", 1)[0])
    return names

if header_sort_order(turbo_path) != sort_order:
    raise SystemExit(f"turbo-picard MergeSamFiles did not set SO:{sort_order}")
if record_names(turbo_path) != record_names(picard_path):
    raise SystemExit("MergeSamFiles record order differs from Picard")
print(f"MergeSamFiles {sort_order} output order matches Picard")
PY
done

cargo run -q -p turbo-picard-cli --bin picard -- \
  MergeSamFiles \
  "I=$workdir/input-a.sam" \
  "I=$workdir/input-b.sam" \
  "O=$workdir/turbo-sidecars.bam" \
  SORT_ORDER=coordinate \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  COMPRESSION_LEVEL=5 \
  MAX_RECORDS_IN_RAM=500 \
  "TMP_DIR=$workdir" \
  VERBOSITY=WARNING \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard MergeSamFiles \
  "I=$workdir/input-a.sam" \
  "I=$workdir/input-b.sam" \
  "O=$workdir/picard-sidecars.bam" \
  SORT_ORDER=coordinate \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  COMPRESSION_LEVEL=5 \
  MAX_RECORDS_IN_RAM=500 \
  "TMP_DIR=$workdir" \
  VERBOSITY=WARNING \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam \
  "I=$workdir/turbo-sidecars.bam" \
  "O=$workdir/turbo-sidecars.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

cargo run -q -p turbo-picard-cli --bin picard -- \
  ViewSam \
  "I=$workdir/picard-sidecars.bam" \
  "O=$workdir/picard-sidecars.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-sidecars.sam" "$workdir/picard-sidecars.sam" "$workdir" <<'PY'
import re
import sys
from pathlib import Path

turbo_path, picard_path, workdir = sys.argv[1:]
workdir = Path(workdir)

def record_names(path):
    names = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if not line.startswith("@"):
                names.append(line.split("\t", 1)[0])
    return names

if record_names(turbo_path) != record_names(picard_path):
    raise SystemExit("MergeSamFiles sidecar BAM record order differs from Picard")

expected = [
    "turbo-sidecars.bam.md5",
    "picard-sidecars.bam.md5",
    "turbo-sidecars.bai",
    "picard-sidecars.bai",
]
missing = [name for name in expected if not (workdir / name).exists()]
if missing:
    raise SystemExit(f"MergeSamFiles sidecar outputs missing: {missing}")
for name in ["turbo-sidecars.bam.md5", "picard-sidecars.bam.md5"]:
    text = (workdir / name).read_text(encoding="utf-8").strip()
    if not re.fullmatch(r"[0-9a-f]{32}", text):
        raise SystemExit(f"MergeSamFiles invalid md5 sidecar content in {name}: {text!r}")
print("MergeSamFiles runtime sidecars and BAM record order match Picard")
PY
