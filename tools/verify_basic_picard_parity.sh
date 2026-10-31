#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conda_prefix="${TURBO_PICARD_CONDA_PREFIX:-$repo_root/.conda-turbo-picard}"
workdir="$(mktemp -d)"
cleanup() {
  local status=$?
  if ((status == 0)); then
    rm -rf "$workdir"
  else
    echo "Failed MarkDuplicates parity evidence retained in $workdir" >&2
  fi
}
trap cleanup EXIT
export TURBO_PICARD_REQUIRE_NATIVE=1

if command -v mamba >/dev/null 2>&1; then
  conda_runner=(mamba)
elif command -v micromamba >/dev/null 2>&1; then
  conda_runner=(micromamba)
else
  echo "mamba or micromamba is required for Picard parity verification" >&2
  exit 127
fi

"${conda_runner[@]}" run -p "$conda_prefix" picard MarkDuplicates --version \
  > "$workdir/picard-version.txt" 2>&1 || true
if [[ -n "${TURBO_PICARD_PICARD_VERSION:-}" ]]; then
  grep -Fx "Version:${TURBO_PICARD_PICARD_VERSION}" "$workdir/picard-version.txt"
fi

for fixture in basic paired scoring softclip secondary pair-score-tie pair-score-tie-reverse-r1 clear-dt tagging-all duplicate-set-members barcode-tag read-barcode-tags optical multi-input multi-library multi-input-libraries remove-sequencing-duplicates; do
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
  if [[ "$fixture" == "pair-score-tie-reverse-r1" ]]; then
    read_name_regex=""
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

  common=()
  if [[ "$fixture" == "multi-input" || "$fixture" == "multi-input-libraries" ]]; then
    common+=("I=$fixture_dir/input1.bam" "I=$fixture_dir/input2.bam")
  else
    common+=("I=$fixture_dir/input.bam")
  fi
  common+=(\
    ASSUME_SORTED=true \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true \
    CLEAR_DT=true \
    "REMOVE_SEQUENCING_DUPLICATES=$remove_sequencing_duplicates" \
    "TAGGING_POLICY=$tagging_policy" \
    "TAG_DUPLICATE_SET_MEMBERS=$tag_duplicate_set_members")
  if [[ -n "$read_name_regex" ]]; then
    common+=("READ_NAME_REGEX=$read_name_regex")
  fi
  if [[ -n "$optical_duplicate_pixel_distance" ]]; then
    common+=("OPTICAL_DUPLICATE_PIXEL_DISTANCE=$optical_duplicate_pixel_distance")
  fi
  if [[ -n "$barcode_tag" ]]; then
    common+=("BARCODE_TAG=$barcode_tag")
  fi
  if [[ -n "$read_one_barcode_tag" ]]; then
    common+=("READ_ONE_BARCODE_TAG=$read_one_barcode_tag")
  fi
  if [[ -n "$read_two_barcode_tag" ]]; then
    common+=("READ_TWO_BARCODE_TAG=$read_two_barcode_tag")
  fi
  native=(cargo run -q -p turbo-picard-cli --bin picard -- MarkDuplicates
    "${common[@]}" "O=$fixture_workdir/turbo-picard.bam" "M=$fixture_workdir/turbo-picard.metrics.txt")
  upstream=("${conda_runner[@]}" run -p "$conda_prefix" picard MarkDuplicates
    "${common[@]}" "O=$fixture_workdir/picard.bam" "M=$fixture_workdir/picard.metrics.txt")
  python3 - "$fixture_workdir/commands.json" "${#native[@]}" "${native[@]}" "${upstream[@]}" <<'PY'
import json
import sys
from pathlib import Path
count = int(sys.argv[2])
Path(sys.argv[1]).write_text(json.dumps({"native": sys.argv[3:3+count], "picard": sys.argv[3+count:]}, indent=2) + "\n")
PY
  "${native[@]}" > "$fixture_workdir/native.stdout" 2> "$fixture_workdir/native.stderr"
  "${upstream[@]}" > "$fixture_workdir/picard.stdout" 2> "$fixture_workdir/picard.stderr"

  for producer in turbo-picard picard; do
    "${conda_runner[@]}" run -p "$conda_prefix" samtools view -h \
      -o "$fixture_workdir/$producer.sam" "$fixture_workdir/$producer.bam"
  done

  echo "Checking MarkDuplicates fixture: $fixture"
  python3 "$repo_root/tools/compare_markduplicates.py" \
    --picard-bam "$fixture_workdir/picard.sam" \
    --turbo-picard-bam "$fixture_workdir/turbo-picard.sam" \
    --picard-metrics "$fixture_workdir/picard.metrics.txt" \
    --turbo-picard-metrics "$fixture_workdir/turbo-picard.metrics.txt"
done
