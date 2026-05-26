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

for fixture in basic paired scoring softclip secondary pair-score-tie clear-dt tagging-all duplicate-set-members barcode-tag read-barcode-tags optical multi-input multi-library multi-input-libraries remove-sequencing-duplicates; do
  fixture_dir="$repo_root/fixtures/markduplicates/$fixture"
  fixture_workdir="$workdir/$fixture"
  tagging_policy="DontTag"
  tag_duplicate_set_members="false"
  barcode_tag=""
  read_one_barcode_tag=""
  read_two_barcode_tag=""
  read_name_regex="null"
  optical_duplicate_pixel_distance=""
  remove_sequencing_duplicates="false"
  if [[ "$fixture" == "tagging-all" ]]; then
    tagging_policy="All"
  fi
  if [[ "$fixture" == "duplicate-set-members" ]]; then
    tag_duplicate_set_members="true"
  fi
  if [[ "$fixture" == "barcode-tag" ]]; then
    barcode_tag="RX"
  fi
  if [[ "$fixture" == "read-barcode-tags" ]]; then
    read_one_barcode_tag="BX"
    read_two_barcode_tag="BY"
  fi
  if [[ "$fixture" == "optical" ]]; then
    tagging_policy="All"
    read_name_regex=""
    optical_duplicate_pixel_distance="100"
  fi
  if [[ "$fixture" == "remove-sequencing-duplicates" ]]; then
    tagging_policy="All"
    read_name_regex=""
    optical_duplicate_pixel_distance="100"
    remove_sequencing_duplicates="true"
  fi
  mkdir -p "$fixture_workdir"

  command=(cargo run -q -p turbo-picard-cli --bin picard -- \
    MarkDuplicates)
  if [[ "$fixture" == "multi-input" || "$fixture" == "multi-input-libraries" ]]; then
    command+=("I=$fixture_dir/input1.bam" "I=$fixture_dir/input2.bam")
  else
    command+=("I=$fixture_dir/input.bam")
  fi
  command+=(\
    "O=$fixture_workdir/turbo-picard.bam" \
    "M=$fixture_workdir/turbo-picard.metrics.txt" \
    ASSUME_SORTED=true \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true \
    CLEAR_DT=true \
    "REMOVE_SEQUENCING_DUPLICATES=$remove_sequencing_duplicates" \
    "TAGGING_POLICY=$tagging_policy" \
    "TAG_DUPLICATE_SET_MEMBERS=$tag_duplicate_set_members")
  if [[ -n "$read_name_regex" ]]; then
    command+=("READ_NAME_REGEX=$read_name_regex")
  fi
  if [[ -n "$optical_duplicate_pixel_distance" ]]; then
    command+=("OPTICAL_DUPLICATE_PIXEL_DISTANCE=$optical_duplicate_pixel_distance")
  fi
  if [[ -n "$barcode_tag" ]]; then
    command+=("BARCODE_TAG=$barcode_tag")
  fi
  if [[ -n "$read_one_barcode_tag" ]]; then
    command+=("READ_ONE_BARCODE_TAG=$read_one_barcode_tag")
  fi
  if [[ -n "$read_two_barcode_tag" ]]; then
    command+=("READ_TWO_BARCODE_TAG=$read_two_barcode_tag")
  fi
  "${command[@]}"

  "${conda_runner[@]}" run -p "$conda_prefix" samtools view -h "$fixture_workdir/turbo-picard.bam" > "$fixture_workdir/turbo-picard.sam"
  "${conda_runner[@]}" run -p "$conda_prefix" samtools view -h "$fixture_dir/picard.bam" > "$fixture_workdir/picard.sam"

  python3 "$repo_root/tools/compare_markduplicates.py" \
    --picard-bam "$fixture_workdir/picard.sam" \
    --turbo-picard-bam "$fixture_workdir/turbo-picard.sam" \
    --picard-metrics "$fixture_dir/picard.metrics.txt" \
    --turbo-picard-metrics "$fixture_workdir/turbo-picard.metrics.txt"
done
