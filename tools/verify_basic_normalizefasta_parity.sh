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

cat > "$workdir/input.fa" <<'FASTA'
>chr1 first chromosome
ACGTACGTAC
GTAC
>chr2
NNNNNN
FASTA

cargo run -q -p turbo-picard-cli --bin picard -- \
  NormalizeFasta \
  "I=$workdir/input.fa" \
  "O=$workdir/turbo.fa" \
  LINE_LENGTH=5 \
  TRUNCATE_SEQUENCE_NAMES_AT_WHITESPACE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard NormalizeFasta \
  "I=$workdir/input.fa" \
  "O=$workdir/picard.fa" \
  LINE_LENGTH=5 \
  TRUNCATE_SEQUENCE_NAMES_AT_WHITESPACE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard.fa" "$workdir/turbo.fa"
echo "NormalizeFasta output matches Picard"

cargo run -q -p turbo-picard-cli --bin picard -- \
  NormalizeFasta \
  "I=$workdir/input.fa" \
  "O=$workdir/turbo-common.fa" \
  LINE_LENGTH=5 \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard NormalizeFasta \
  "I=$workdir/input.fa" \
  "O=$workdir/picard-common.fa" \
  LINE_LENGTH=5 \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-common.fa" "$workdir/turbo-common.fa"
test ! -e "$workdir/turbo-common.fa.md5"
test ! -e "$workdir/picard-common.fa.md5"
test ! -e "$workdir/turbo-common.fa.fai"
test ! -e "$workdir/picard-common.fa.fai"
echo "NormalizeFasta common no-op sidecar options match Picard"
