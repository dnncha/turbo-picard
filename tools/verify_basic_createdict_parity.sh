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
