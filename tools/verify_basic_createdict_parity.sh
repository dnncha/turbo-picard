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

cat > "$workdir/ref.fa" <<'FASTA'
>chr1 first chromosome
ACGTACGT
>chr2
NNNN
FASTA

cargo run -q -p turbo-picard-cli --bin picard -- \
  CreateSequenceDictionary \
  "R=$workdir/ref.fa" \
  "O=$workdir/turbo.dict" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CreateSequenceDictionary \
  "R=$workdir/ref.fa" \
  "O=$workdir/picard.dict" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard.dict" "$workdir/turbo.dict"
echo "CreateSequenceDictionary output matches Picard"

cat > "$workdir/alt.tsv" <<'ALT'
chr1	1
chr1	CM000663.2
chr2	2
ALT

cargo run -q -p turbo-picard-cli --bin picard -- \
  CreateSequenceDictionary \
  "R=$workdir/ref.fa" \
  "O=$workdir/turbo-alt.dict" \
  "AN=$workdir/alt.tsv" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CreateSequenceDictionary \
  "R=$workdir/ref.fa" \
  "O=$workdir/picard-alt.dict" \
  "AN=$workdir/alt.tsv" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-alt.dict" "$workdir/turbo-alt.dict"
echo "CreateSequenceDictionary ALT_NAMES output matches Picard"

cargo run -q -p turbo-picard-cli --bin picard -- \
  CreateSequenceDictionary \
  "R=$workdir/ref.fa" \
  "O=$workdir/turbo-md5.dict" \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  MAX_RECORDS_IN_RAM=1000 \
  TMP_DIR="$workdir" \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  COMPRESSION_LEVEL=1 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CreateSequenceDictionary \
  "R=$workdir/ref.fa" \
  "O=$workdir/picard-md5.dict" \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  MAX_RECORDS_IN_RAM=1000 \
  TMP_DIR="$workdir" \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  COMPRESSION_LEVEL=1 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-md5.dict" "$workdir/turbo-md5.dict"
test -s "$workdir/turbo-md5.dict.md5"
test -s "$workdir/picard-md5.dict.md5"
grep -Eq '^[0-9a-f]{32}$' "$workdir/turbo-md5.dict.md5"
grep -Eq '^[0-9a-f]{32}$' "$workdir/picard-md5.dict.md5"
test ! -e "$workdir/turbo-md5.dict.bai"
test ! -e "$workdir/picard-md5.dict.bai"
echo "CreateSequenceDictionary md5/runtime options match Picard"
