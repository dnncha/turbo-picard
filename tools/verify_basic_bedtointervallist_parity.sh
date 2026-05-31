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

cargo run -q -p turbo-picard-cli --bin picard -- \
  BedToIntervalList \
  "I=$workdir/input.bed" \
  "O=$workdir/turbo-unsorted.interval_list" \
  "SD=$workdir/ref.dict" \
  SORT=false \
  UNIQUE=false \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard BedToIntervalList \
  "I=$workdir/input.bed" \
  "O=$workdir/picard-unsorted.interval_list" \
  "SD=$workdir/ref.dict" \
  SORT=false \
  UNIQUE=false \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-unsorted.interval_list" "$workdir/turbo-unsorted.interval_list"
echo "BedToIntervalList SORT=false output matches Picard"

cat > "$workdir/missing-contig.bed" <<'BED'
chr_missing	0	3	missing	0	+
chr1	4	8	kept	0	+
BED

cargo run -q -p turbo-picard-cli --bin picard -- \
  BedToIntervalList \
  "I=$workdir/missing-contig.bed" \
  "O=$workdir/turbo-drop-missing.interval_list" \
  "SD=$workdir/ref.dict" \
  DROP_MISSING_CONTIGS=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard BedToIntervalList \
  "I=$workdir/missing-contig.bed" \
  "O=$workdir/picard-drop-missing.interval_list" \
  "SD=$workdir/ref.dict" \
  DROP_MISSING_CONTIGS=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-drop-missing.interval_list" "$workdir/turbo-drop-missing.interval_list"
echo "BedToIntervalList DROP_MISSING_CONTIGS output matches Picard"

cat > "$workdir/zero-length.bed" <<'BED'
chr1	5	5	zero	0	+
chr1	5	8	kept	0	+
BED

cargo run -q -p turbo-picard-cli --bin picard -- \
  BedToIntervalList \
  "I=$workdir/zero-length.bed" \
  "O=$workdir/turbo-zero-skipped.interval_list" \
  "SD=$workdir/ref.dict" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard BedToIntervalList \
  "I=$workdir/zero-length.bed" \
  "O=$workdir/picard-zero-skipped.interval_list" \
  "SD=$workdir/ref.dict" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-zero-skipped.interval_list" "$workdir/turbo-zero-skipped.interval_list"
echo "BedToIntervalList zero-length default skip output matches Picard"

cargo run -q -p turbo-picard-cli --bin picard -- \
  BedToIntervalList \
  "I=$workdir/zero-length.bed" \
  "O=$workdir/turbo-zero-kept.interval_list" \
  "SD=$workdir/ref.dict" \
  KEEP_LENGTH_ZERO_INTERVALS=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard BedToIntervalList \
  "I=$workdir/zero-length.bed" \
  "O=$workdir/picard-zero-kept.interval_list" \
  "SD=$workdir/ref.dict" \
  KEEP_LENGTH_ZERO_INTERVALS=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-zero-kept.interval_list" "$workdir/turbo-zero-kept.interval_list"
echo "BedToIntervalList KEEP_LENGTH_ZERO_INTERVALS output matches Picard"

cargo run -q -p turbo-picard-cli --bin picard -- \
  BedToIntervalList \
  "I=$workdir/zero-length.bed" \
  "O=$workdir/turbo-runtime.interval_list" \
  "SD=$workdir/ref.dict" \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  COMPRESSION_LEVEL=1 \
  MAX_RECORDS_IN_RAM=1000 \
  TMP_DIR="$workdir" \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard BedToIntervalList \
  "I=$workdir/zero-length.bed" \
  "O=$workdir/picard-runtime.interval_list" \
  "SD=$workdir/ref.dict" \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  COMPRESSION_LEVEL=1 \
  MAX_RECORDS_IN_RAM=1000 \
  TMP_DIR="$workdir" \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-runtime.interval_list" "$workdir/turbo-runtime.interval_list"
test ! -e "$workdir/turbo-runtime.interval_list.md5"
test ! -e "$workdir/picard-runtime.interval_list.md5"
test ! -e "$workdir/turbo-runtime.interval_list.bai"
test ! -e "$workdir/picard-runtime.interval_list.bai"
echo "BedToIntervalList common no-op sidecar options match Picard"
