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
compare="$repo_root/tools/parity_compare.py"

if [[ ! -f "$reference" ]]; then
  echo "missing mitochondrial reference: $reference" >&2
  exit 1
fi

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
  parity_view_to_sam "" "$1" "$2" "$repo_root"
}

# Typical GATK-style preprocessing chain on a coordinate-sorted shard:
# MarkDuplicates -> SortSam -> SetNmMdAndUqTags
picard MarkDuplicates \
  "I=$input_bam" "O=$workdir/picard-markdup.bam" "M=$workdir/picard-markdup.metrics.txt" \
  ASSUME_SORTED=true
turbo MarkDuplicates \
  "I=$input_bam" "O=$workdir/turbo-markdup.bam" "M=$workdir/turbo-markdup.metrics.txt" \
  ASSUME_SORTED=true
view_to_sam "$workdir/picard-markdup.bam" "$workdir/picard-markdup.sam"
view_to_sam "$workdir/turbo-markdup.bam" "$workdir/turbo-markdup.sam"
python3 "$compare" markduplicates --label "GATK combo MarkDuplicates" --repo-root "$repo_root" \
  --picard-alignment "$workdir/picard-markdup.sam" \
  --turbo-alignment "$workdir/turbo-markdup.sam" \
  --picard-metrics "$workdir/picard-markdup.metrics.txt" \
  --turbo-metrics "$workdir/turbo-markdup.metrics.txt"

picard SortSam "I=$input_bam" "O=$workdir/picard-sorted.bam" SORT_ORDER=coordinate
turbo SortSam "I=$input_bam" "O=$workdir/turbo-sorted.bam" SORT_ORDER=coordinate
view_to_sam "$workdir/picard-sorted.bam" "$workdir/picard-sorted.sam"
view_to_sam "$workdir/turbo-sorted.bam" "$workdir/turbo-sorted.sam"
python3 "$compare" merge-multiset --label "GATK combo SortSam raw shard" \
  --picard "$workdir/picard-sorted.sam" --turbo "$workdir/turbo-sorted.sam"

picard SortSam \
  "I=$workdir/picard-markdup.bam" "O=$workdir/picard-markdup-sorted.bam" \
  SORT_ORDER=coordinate
turbo SortSam \
  "I=$workdir/turbo-markdup.bam" "O=$workdir/turbo-markdup-sorted.bam" \
  SORT_ORDER=coordinate
view_to_sam "$workdir/picard-markdup-sorted.bam" "$workdir/picard-markdup-sorted.sam"
view_to_sam "$workdir/turbo-markdup-sorted.bam" "$workdir/turbo-markdup-sorted.sam"
python3 "$compare" merge-multiset --label "GATK combo SortSam after MarkDuplicates" \
  --picard "$workdir/picard-markdup-sorted.sam" --turbo "$workdir/turbo-markdup-sorted.sam"

picard SortSam \
  "I=$workdir/picard-markdup-sorted.bam" "O=$workdir/picard-markdup-qn.bam" \
  SORT_ORDER=queryname
turbo SortSam \
  "I=$workdir/turbo-markdup-sorted.bam" "O=$workdir/turbo-markdup-qn.bam" \
  SORT_ORDER=queryname
picard FixMateInformation \
  "I=$workdir/picard-markdup-qn.bam" "O=$workdir/picard-fixmate.bam" \
  SORT_ORDER=coordinate
turbo FixMateInformation \
  "I=$workdir/turbo-markdup-qn.bam" "O=$workdir/turbo-fixmate.bam" \
  SORT_ORDER=coordinate
view_to_sam "$workdir/picard-fixmate.bam" "$workdir/picard-fixmate.sam"
view_to_sam "$workdir/turbo-fixmate.bam" "$workdir/turbo-fixmate.sam"
python3 "$compare" stable-sam --label "GATK combo FixMateInformation" \
  --picard "$workdir/picard-fixmate.sam" --turbo "$workdir/turbo-fixmate.sam"

picard SetNmMdAndUqTags \
  "I=$workdir/picard-markdup-sorted.bam" "O=$workdir/picard-setnmmd.bam" \
  "R=$reference"
turbo SetNmMdAndUqTags \
  "I=$workdir/turbo-markdup-sorted.bam" "O=$workdir/turbo-setnmmd.bam" \
  "R=$reference"
view_to_sam "$workdir/picard-setnmmd.bam" "$workdir/picard-setnmmd.sam"
view_to_sam "$workdir/turbo-setnmmd.bam" "$workdir/turbo-setnmmd.sam"
python3 "$compare" merge-multiset --label "GATK combo SetNmMdAndUqTags" \
  --picard "$workdir/picard-setnmmd.sam" --turbo "$workdir/turbo-setnmmd.sam"

echo "GATK preprocessing combo parity passed: MarkDuplicates, raw-shard SortSam, post-markdup SortSam, FixMateInformation, SetNmMdAndUqTags"
