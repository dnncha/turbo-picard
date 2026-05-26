#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conda_prefix="${JEANLUC_CONDA_PREFIX:-$repo_root/.conda-jeanluc}"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

for fixture in basic paired scoring softclip secondary pair-score-tie clear-dt tagging-all duplicate-set-members; do
  fixture_dir="$repo_root/fixtures/markduplicates/$fixture"
  fixture_workdir="$workdir/$fixture"
  tagging_policy="DontTag"
  tag_duplicate_set_members="false"
  if [[ "$fixture" == "tagging-all" ]]; then
    tagging_policy="All"
  fi
  if [[ "$fixture" == "duplicate-set-members" ]]; then
    tag_duplicate_set_members="true"
  fi
  mkdir -p "$fixture_workdir"

  cargo run -q -p jeanluc-cli --bin picard -- \
    MarkDuplicates \
    "I=$fixture_dir/input.bam" \
    "O=$fixture_workdir/jeanluc.bam" \
    "M=$fixture_workdir/jeanluc.metrics.txt" \
    ASSUME_SORTED=true \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true \
    CLEAR_DT=true \
    READ_NAME_REGEX=null \
    "TAGGING_POLICY=$tagging_policy" \
    "TAG_DUPLICATE_SET_MEMBERS=$tag_duplicate_set_members"

  mamba run -p "$conda_prefix" samtools view -h "$fixture_workdir/jeanluc.bam" > "$fixture_workdir/jeanluc.sam"
  mamba run -p "$conda_prefix" samtools view -h "$fixture_dir/picard.bam" > "$fixture_workdir/picard.sam"

  python3 "$repo_root/tools/compare_markduplicates.py" \
    --picard-bam "$fixture_workdir/picard.sam" \
    --jeanluc-bam "$fixture_workdir/jeanluc.sam" \
    --picard-metrics "$fixture_dir/picard.metrics.txt" \
    --jeanluc-metrics "$fixture_workdir/jeanluc.metrics.txt"
done
