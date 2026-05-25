#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conda_prefix="${JEANLUC_CONDA_PREFIX:-$repo_root/.conda-jeanluc}"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

cargo run -q -p jeanluc-cli -- \
  MarkDuplicates \
  "I=$repo_root/fixtures/markduplicates/basic/input.bam" \
  "O=$workdir/jeanluc.bam" \
  "M=$workdir/jeanluc.metrics.txt"

mamba run -p "$conda_prefix" samtools view -h "$workdir/jeanluc.bam" > "$workdir/jeanluc.sam"
mamba run -p "$conda_prefix" samtools view -h "$repo_root/fixtures/markduplicates/basic/picard.bam" > "$workdir/picard.sam"

python3 "$repo_root/tools/compare_markduplicates.py" \
  --picard-bam "$workdir/picard.sam" \
  --jeanluc-bam "$workdir/jeanluc.sam" \
  --picard-metrics "$repo_root/fixtures/markduplicates/basic/picard.metrics.txt" \
  --jeanluc-metrics "$workdir/jeanluc.metrics.txt"
