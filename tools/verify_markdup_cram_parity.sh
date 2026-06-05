#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=tools/cram_compat.sh
source "$repo_root/tools/cram_compat.sh"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT
parity_conda_setup "$repo_root" "$workdir"

reference="$repo_root/fixtures/reference/chr1.fa"
compare="$repo_root/tools/parity_compare.py"

picard() {
  parity_picard "$@"
}

turbo() {
  cargo run -q -p turbo-picard-cli --bin picard -- "$@" \
    VALIDATION_STRINGENCY=SILENT QUIET=true
}

bam_to_cram() {
  cram_encode_bam "$reference" "$2" "$1"
}

view_to_sam() {
  parity_view_to_sam "$reference" "$1" "$2" "$repo_root"
}

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

  if [[ "$fixture" == "multi-input" || "$fixture" == "multi-input-libraries" ]]; then
    bam_to_cram "$fixture_dir/input1.bam" "$fixture_workdir/input1.cram"
    bam_to_cram "$fixture_dir/input2.bam" "$fixture_workdir/input2.cram"
    inputs=("I=$fixture_workdir/input1.cram" "I=$fixture_workdir/input2.cram")
  else
    bam_to_cram "$fixture_dir/input.bam" "$fixture_workdir/input.cram"
    inputs=("I=$fixture_workdir/input.cram")
  fi

  extra=(
    ASSUME_SORTED=true CLEAR_DT=true
    "REMOVE_SEQUENCING_DUPLICATES=$remove_sequencing_duplicates"
    "TAGGING_POLICY=$tagging_policy"
    "TAG_DUPLICATE_SET_MEMBERS=$tag_duplicate_set_members"
    "R=$reference"
  )
  if [[ -n "$read_name_regex" ]]; then
    extra+=("READ_NAME_REGEX=$read_name_regex")
  fi
  if [[ -n "$optical_duplicate_pixel_distance" ]]; then
    extra+=("OPTICAL_DUPLICATE_PIXEL_DISTANCE=$optical_duplicate_pixel_distance")
  fi
  if [[ -n "$barcode_tag" ]]; then
    extra+=("BARCODE_TAG=$barcode_tag")
  fi
  if [[ -n "$read_one_barcode_tag" ]]; then
    extra+=("READ_ONE_BARCODE_TAG=$read_one_barcode_tag")
  fi
  if [[ -n "$read_two_barcode_tag" ]]; then
    extra+=("READ_TWO_BARCODE_TAG=$read_two_barcode_tag")
  fi

  picard MarkDuplicates \
    "${inputs[@]}" \
    "O=$fixture_workdir/picard.cram" \
    "M=$fixture_workdir/picard.metrics.txt" \
    "${extra[@]}"
  turbo MarkDuplicates \
    "${inputs[@]}" \
    "O=$fixture_workdir/turbo.cram" \
    "M=$fixture_workdir/turbo.metrics.txt" \
    "${extra[@]}"

  view_to_sam "$fixture_workdir/picard.cram" "$fixture_workdir/picard.sam"
  view_to_sam "$fixture_workdir/turbo.cram" "$fixture_workdir/turbo.sam"
  python3 "$compare" markduplicates --label "MarkDuplicates CRAM/$fixture" --repo-root "$repo_root" \
    --picard-alignment "$fixture_workdir/picard.sam" \
    --turbo-alignment "$fixture_workdir/turbo.sam" \
    --picard-metrics "$fixture_workdir/picard.metrics.txt" \
    --turbo-metrics "$fixture_workdir/turbo.metrics.txt"
  echo "MarkDuplicates CRAM parity passed for fixture: $fixture"
done

echo "All MarkDuplicates CRAM fixture parity checks passed"