#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=tools/cram_compat.sh
source "$repo_root/tools/cram_compat.sh"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT
parity_conda_setup "$repo_root" "$workdir"

input_bam="$repo_root/benchmarks/real-data/gatk-na12878-mito/input.bam"
reference="$repo_root/fixtures/reference/chrM.fa"
checked_in_cram="$repo_root/benchmarks/real-data/gatk-na12878-mito-cram/input.cram"
input_cram="${TURBO_PICARD_INPUT_CRAM:-$checked_in_cram}"
compare="$repo_root/tools/parity_compare.py"

if [[ ! -f "$input_bam" ]]; then
  echo "missing GATK mitochondrial fixture: $input_bam" >&2
  exit 1
fi

picard() {
  parity_picard "$@"
}

turbo() {
  cargo run -q -p turbo-picard-cli --bin picard -- "$@" \
    VALIDATION_STRINGENCY=SILENT QUIET=true
}

view_to_sam() {
  parity_view_to_sam "$reference" "$1" "$2" "$repo_root"
}

if [[ ! -f "$input_cram" ]]; then
  input_cram="$workdir/input.cram"
  cram_encode_bam "$reference" "$input_cram" "$input_bam"
fi

picard MarkDuplicates \
  "I=$input_cram" "O=$workdir/picard-markdup.cram" "M=$workdir/picard-markdup.metrics.txt" \
  "R=$reference" ASSUME_SORTED=true
turbo MarkDuplicates \
  "I=$input_cram" "O=$workdir/turbo-markdup.cram" "M=$workdir/turbo-markdup.metrics.txt" \
  "R=$reference" ASSUME_SORTED=true
view_to_sam "$workdir/picard-markdup.cram" "$workdir/picard-markdup.sam"
view_to_sam "$workdir/turbo-markdup.cram" "$workdir/turbo-markdup.sam"
python3 "$compare" markduplicates --label "GATK combo CRAM MarkDuplicates" --repo-root "$repo_root" \
  --picard-alignment "$workdir/picard-markdup.sam" \
  --turbo-alignment "$workdir/turbo-markdup.sam" \
  --picard-metrics "$workdir/picard-markdup.metrics.txt" \
  --turbo-metrics "$workdir/turbo-markdup.metrics.txt"

picard SortSam \
  "I=$input_cram" "O=$workdir/picard-sorted.cram" \
  "R=$reference" SORT_ORDER=coordinate
turbo SortSam \
  "I=$input_cram" "O=$workdir/turbo-sorted.cram" \
  "R=$reference" SORT_ORDER=coordinate
view_to_sam "$workdir/picard-sorted.cram" "$workdir/picard-sorted.sam"
view_to_sam "$workdir/turbo-sorted.cram" "$workdir/turbo-sorted.sam"
python3 "$compare" merge-multiset --label "GATK combo CRAM SortSam raw shard" \
  --picard "$workdir/picard-sorted.sam" --turbo "$workdir/turbo-sorted.sam"

picard SortSam \
  "I=$workdir/picard-markdup.cram" "O=$workdir/picard-markdup-sorted.cram" \
  "R=$reference" SORT_ORDER=coordinate
turbo SortSam \
  "I=$workdir/turbo-markdup.cram" "O=$workdir/turbo-markdup-sorted.cram" \
  "R=$reference" SORT_ORDER=coordinate
view_to_sam "$workdir/picard-markdup-sorted.cram" "$workdir/picard-markdup-sorted.sam"
view_to_sam "$workdir/turbo-markdup-sorted.cram" "$workdir/turbo-markdup-sorted.sam"
python3 "$compare" merge-multiset --label "GATK combo CRAM SortSam after MarkDuplicates" \
  --picard "$workdir/picard-markdup-sorted.sam" --turbo "$workdir/turbo-markdup-sorted.sam"

picard SortSam \
  "I=$workdir/picard-markdup-sorted.cram" "O=$workdir/picard-markdup-qn.cram" \
  "R=$reference" SORT_ORDER=queryname
turbo SortSam \
  "I=$workdir/turbo-markdup-sorted.cram" "O=$workdir/turbo-markdup-qn.cram" \
  "R=$reference" SORT_ORDER=queryname
picard FixMateInformation \
  "I=$workdir/picard-markdup-qn.cram" "O=$workdir/picard-fixmate.cram" \
  "R=$reference" SORT_ORDER=coordinate
turbo FixMateInformation \
  "I=$workdir/turbo-markdup-qn.cram" "O=$workdir/turbo-fixmate.cram" \
  "R=$reference" SORT_ORDER=coordinate
view_to_sam "$workdir/picard-fixmate.cram" "$workdir/picard-fixmate.sam"
view_to_sam "$workdir/turbo-fixmate.cram" "$workdir/turbo-fixmate.sam"
python3 "$compare" stable-sam --label "GATK combo CRAM FixMateInformation" \
  --picard "$workdir/picard-fixmate.sam" --turbo "$workdir/turbo-fixmate.sam"

picard SetNmMdAndUqTags \
  "I=$workdir/picard-markdup-sorted.cram" "O=$workdir/picard-setnmmd.cram" \
  "R=$reference"
turbo SetNmMdAndUqTags \
  "I=$workdir/turbo-markdup-sorted.cram" "O=$workdir/turbo-setnmmd.cram" \
  "R=$reference"
view_to_sam "$workdir/picard-setnmmd.cram" "$workdir/picard-setnmmd.sam"
view_to_sam "$workdir/turbo-setnmmd.cram" "$workdir/turbo-setnmmd.sam"
python3 "$compare" stable-sam --label "GATK combo CRAM SetNmMdAndUqTags" \
  --picard "$workdir/picard-setnmmd.sam" --turbo "$workdir/turbo-setnmmd.sam"

echo "GATK preprocessing CRAM combo parity passed: MarkDuplicates, raw-shard SortSam, post-markdup SortSam, FixMateInformation, SetNmMdAndUqTags"