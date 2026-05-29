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

cat > "$workdir/ref.dict" <<'DICT'
@HD	VN:1.6
@SQ	SN:chr1	LN:1000	M5:00000000000000000000000000000000	UR:file://ref.fa
@SQ	SN:chr2	LN:1000	M5:11111111111111111111111111111111	UR:file://ref.fa
DICT

cat > "$workdir/input.bed" <<'BED'
chr2	10	20	name-b	0	-
chr1	0	4	name-a	0	+
chr1	0	4	name-a	0	+
BED

cargo run -q -p turbo-picard-cli --bin picard -- \
  BedToIntervalList \
  "I=$workdir/input.bed" \
  "O=$workdir/turbo.interval_list" \
  "SD=$workdir/ref.dict" \
  SORT=true \
  UNIQUE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard BedToIntervalList \
  "I=$workdir/input.bed" \
  "O=$workdir/picard.interval_list" \
  "SD=$workdir/ref.dict" \
  SORT=true \
  UNIQUE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard.interval_list" "$workdir/turbo.interval_list"
echo "BedToIntervalList output matches Picard"
