#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conda_prefix="${JEANLUC_CONDA_PREFIX:-$repo_root/.conda-jeanluc}"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

for fixture in basic paired scoring softclip secondary pair-score-tie; do
  fixture_dir="$repo_root/fixtures/markduplicates/$fixture"
  fixture_workdir="$workdir/$fixture"
  mkdir -p "$fixture_workdir"

  cargo run -q -p jeanluc-cli -- \
    MarkDuplicates \
    "I=$fixture_dir/input.bam" \
    "O=$fixture_workdir/jeanluc.bam" \
    "M=$fixture_workdir/jeanluc.metrics.txt" \
    ASSUME_SORTED=true \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true

  mamba run -p "$conda_prefix" samtools view -h "$fixture_workdir/jeanluc.bam" > "$fixture_workdir/jeanluc.sam"
  mamba run -p "$conda_prefix" samtools view -h "$fixture_dir/picard.bam" > "$fixture_workdir/picard.sam"

  python3 "$repo_root/tools/compare_markduplicates.py" \
    --picard-bam "$fixture_workdir/picard.sam" \
    --jeanluc-bam "$fixture_workdir/jeanluc.sam" \
    --picard-metrics "$fixture_dir/picard.metrics.txt" \
    --jeanluc-metrics "$fixture_workdir/jeanluc.metrics.txt"
done
