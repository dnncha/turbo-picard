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

cat > "$workdir/Rscript" <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod +x "$workdir/Rscript"
export PATH="$workdir:$PATH"

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

picard ViewSam "I=$input_bam" > "$workdir/picard-viewsam.sam"
turbo ViewSam "I=$input_bam" > "$workdir/turbo-viewsam.sam"
python3 "$compare" stable-sam --label "GATK mito BAM ViewSam" \
  --picard "$workdir/picard-viewsam.sam" --turbo "$workdir/turbo-viewsam.sam"

picard CleanSam "I=$input_bam" "O=$workdir/picard-cleansam.bam" CREATE_INDEX=true
turbo CleanSam "I=$input_bam" "O=$workdir/turbo-cleansam.bam" CREATE_INDEX=true
view_to_sam "$workdir/picard-cleansam.bam" "$workdir/picard-cleansam.sam"
view_to_sam "$workdir/turbo-cleansam.bam" "$workdir/turbo-cleansam.sam"
python3 "$compare" cleansam --label "GATK mito BAM CleanSam" \
  --picard "$workdir/picard-cleansam.sam" --turbo "$workdir/turbo-cleansam.sam"

picard AddOrReplaceReadGroups \
  "I=$input_bam" "O=$workdir/picard-addrg.bam" CREATE_INDEX=true \
  RGID=turbo RGLB=library RGPL=ILLUMINA RGPU=unit RGSM=sample
turbo AddOrReplaceReadGroups \
  "I=$input_bam" "O=$workdir/turbo-addrg.bam" CREATE_INDEX=true \
  RGID=turbo RGLB=library RGPL=ILLUMINA RGPU=unit RGSM=sample
view_to_sam "$workdir/picard-addrg.bam" "$workdir/picard-addrg.sam"
view_to_sam "$workdir/turbo-addrg.bam" "$workdir/turbo-addrg.sam"
PYTHONPATH="$repo_root/tools" python3 - "$workdir/picard-addrg.sam" "$workdir/turbo-addrg.sam" <<'PY'
import sys
from pathlib import Path

import compare_real_data

picard = Path(sys.argv[1])
turbo = Path(sys.argv[2])
if compare_real_data.digest_sam_records_and_read_groups(picard) != (
    compare_real_data.digest_sam_records_and_read_groups(turbo)
):
    raise SystemExit("GATK mito BAM AddOrReplaceReadGroups differs from Picard")
print("GATK mito BAM AddOrReplaceReadGroups parity check passed")
PY

picard MarkDuplicates \
  "I=$input_bam" "O=$workdir/picard-markdup.bam" "M=$workdir/picard-markdup.metrics.txt" \
  ASSUME_SORTED=true
turbo MarkDuplicates \
  "I=$input_bam" "O=$workdir/turbo-markdup.bam" "M=$workdir/turbo-markdup.metrics.txt" \
  ASSUME_SORTED=true
view_to_sam "$workdir/picard-markdup.bam" "$workdir/picard-markdup.sam"
view_to_sam "$workdir/turbo-markdup.bam" "$workdir/turbo-markdup.sam"
python3 "$compare" markduplicates --label "GATK mito BAM MarkDuplicates" --repo-root "$repo_root" \
  --picard-alignment "$workdir/picard-markdup.sam" \
  --turbo-alignment "$workdir/turbo-markdup.sam" \
  --picard-metrics "$workdir/picard-markdup.metrics.txt" \
  --turbo-metrics "$workdir/turbo-markdup.metrics.txt"

picard SortSam "I=$input_bam" "O=$workdir/picard-sorted.bam" SORT_ORDER=coordinate
turbo SortSam "I=$input_bam" "O=$workdir/turbo-sorted.bam" SORT_ORDER=coordinate
view_to_sam "$workdir/picard-sorted.bam" "$workdir/picard-sorted.sam"
view_to_sam "$workdir/turbo-sorted.bam" "$workdir/turbo-sorted.sam"
python3 "$compare" merge-multiset --label "GATK mito BAM SortSam" \
  --picard "$workdir/picard-sorted.sam" --turbo "$workdir/turbo-sorted.sam"

picard_validate_exit=0
picard ValidateSamFile \
  "I=$input_bam" "O=$workdir/picard-validate.txt" MODE=SUMMARY \
  || picard_validate_exit=$?
turbo_validate_exit=0
turbo ValidateSamFile \
  "I=$input_bam" "O=$workdir/turbo-validate.txt" MODE=SUMMARY \
  || turbo_validate_exit=$?
if [[ "$picard_validate_exit" != "$turbo_validate_exit" ]]; then
  echo "GATK mito BAM ValidateSamFile exit differs from Picard: Picard=$picard_validate_exit turbo=$turbo_validate_exit" >&2
  exit 1
fi
python3 "$compare" validate-summary --label "GATK mito BAM ValidateSamFile" \
  --picard "$workdir/picard-validate.txt" --turbo "$workdir/turbo-validate.txt"

picard CollectQualityYieldMetrics \
  "I=$input_bam" "O=$workdir/picard-quality-yield.txt"
turbo CollectQualityYieldMetrics \
  "I=$input_bam" "O=$workdir/turbo-quality-yield.txt"
python3 "$compare" metrics --label "GATK mito BAM CollectQualityYieldMetrics" \
  --picard "$workdir/picard-quality-yield.txt" --turbo "$workdir/turbo-quality-yield.txt"

picard CollectAlignmentSummaryMetrics \
  "I=$input_bam" "O=$workdir/picard-alignment-summary.txt"
turbo CollectAlignmentSummaryMetrics \
  "I=$input_bam" "O=$workdir/turbo-alignment-summary.txt"
python3 "$compare" metrics --label "GATK mito BAM CollectAlignmentSummaryMetrics" \
  --picard "$workdir/picard-alignment-summary.txt" --turbo "$workdir/turbo-alignment-summary.txt"

picard CollectInsertSizeMetrics \
  "I=$input_bam" "O=$workdir/picard-insert-size.txt" "H=$workdir/picard-insert-size.pdf"
turbo CollectInsertSizeMetrics \
  "I=$input_bam" "O=$workdir/turbo-insert-size.txt" "H=$workdir/turbo-insert-size.pdf"
python3 "$compare" metrics --label "GATK mito BAM CollectInsertSizeMetrics" \
  --picard "$workdir/picard-insert-size.txt" --turbo "$workdir/turbo-insert-size.txt"

picard MeanQualityByCycle \
  "I=$input_bam" "O=$workdir/picard-mean-quality.txt" "CHART=$workdir/picard-mean-quality.pdf"
turbo MeanQualityByCycle \
  "I=$input_bam" "O=$workdir/turbo-mean-quality.txt" "CHART=$workdir/turbo-mean-quality.pdf"
python3 "$compare" metrics --label "GATK mito BAM MeanQualityByCycle" \
  --picard "$workdir/picard-mean-quality.txt" --turbo "$workdir/turbo-mean-quality.txt"

picard QualityScoreDistribution \
  "I=$input_bam" "O=$workdir/picard-quality-score.txt" "CHART=$workdir/picard-quality-score.pdf"
turbo QualityScoreDistribution \
  "I=$input_bam" "O=$workdir/turbo-quality-score.txt" "CHART=$workdir/turbo-quality-score.pdf"
python3 "$compare" metrics --label "GATK mito BAM QualityScoreDistribution" \
  --picard "$workdir/picard-quality-score.txt" --turbo "$workdir/turbo-quality-score.txt"

picard CollectBaseDistributionByCycle \
  "I=$input_bam" "O=$workdir/picard-base-cycle.txt" "CHART=$workdir/picard-base-cycle.pdf"
turbo CollectBaseDistributionByCycle \
  "I=$input_bam" "O=$workdir/turbo-base-cycle.txt" "CHART=$workdir/turbo-base-cycle.pdf"
python3 "$compare" metrics --label "GATK mito BAM CollectBaseDistributionByCycle" \
  --picard "$workdir/picard-base-cycle.txt" --turbo "$workdir/turbo-base-cycle.txt"

picard CollectGcBiasMetrics \
  "I=$input_bam" "O=$workdir/picard-gc-detail.txt" \
  "S=$workdir/picard-gc-summary.txt" "CHART=$workdir/picard-gc.pdf" \
  "R=$reference"
turbo CollectGcBiasMetrics \
  "I=$input_bam" "O=$workdir/turbo-gc-detail.txt" \
  "S=$workdir/turbo-gc-summary.txt" "CHART=$workdir/turbo-gc.pdf" \
  "R=$reference"
python3 "$compare" metrics --label "GATK mito BAM CollectGcBiasMetrics detail" \
  --picard "$workdir/picard-gc-detail.txt" --turbo "$workdir/turbo-gc-detail.txt"
python3 "$compare" metrics --label "GATK mito BAM CollectGcBiasMetrics summary" \
  --picard "$workdir/picard-gc-summary.txt" --turbo "$workdir/turbo-gc-summary.txt"

picard BuildBamIndex "I=$input_bam" "O=$workdir/picard.bai"
turbo BuildBamIndex "I=$input_bam" "O=$workdir/turbo.bai"
python3 "$compare" binary --label "GATK mito BAM BuildBamIndex" \
  --picard "$workdir/picard.bai" --turbo "$workdir/turbo.bai"

picard RevertSam "I=$input_bam" "O=$workdir/picard-revert.sam"
turbo RevertSam "I=$input_bam" "O=$workdir/turbo-revert.sam"
python3 "$compare" stable-sam --label "GATK mito BAM RevertSam" \
  --picard "$workdir/picard-revert.sam" --turbo "$workdir/turbo-revert.sam"

picard SamToFastq \
  "I=$input_bam" "FASTQ=$workdir/picard-r1.fastq" \
  "SECOND_END_FASTQ=$workdir/picard-r2.fastq" "UNPAIRED_FASTQ=$workdir/picard-unpaired.fastq"
turbo SamToFastq \
  "I=$input_bam" "FASTQ=$workdir/turbo-r1.fastq" \
  "SECOND_END_FASTQ=$workdir/turbo-r2.fastq" "UNPAIRED_FASTQ=$workdir/turbo-unpaired.fastq"
python3 "$compare" fastq-trio --label "GATK mito BAM SamToFastq" \
  --picard-r1 "$workdir/picard-r1.fastq" --picard-r2 "$workdir/picard-r2.fastq" \
  --picard-unpaired "$workdir/picard-unpaired.fastq" \
  --turbo-r1 "$workdir/turbo-r1.fastq" --turbo-r2 "$workdir/turbo-r2.fastq" \
  --turbo-unpaired "$workdir/turbo-unpaired.fastq"

picard CollectMultipleMetrics \
  "I=$input_bam" "O=$workdir/picard-multi" \
  PROGRAM=null PROGRAM=CollectQualityYieldMetrics PROGRAM=CollectInsertSizeMetrics \
  "TMP_DIR=$workdir"
turbo CollectMultipleMetrics \
  "I=$input_bam" "O=$workdir/turbo-multi" \
  PROGRAM=null PROGRAM=CollectQualityYieldMetrics PROGRAM=CollectInsertSizeMetrics \
  "TMP_DIR=$workdir"
python3 "$compare" metrics --label "GATK mito BAM CollectMultipleMetrics quality yield" \
  --picard "$workdir/picard-multi.quality_yield_metrics" \
  --turbo "$workdir/turbo-multi.quality_yield_metrics"
python3 "$compare" metrics --label "GATK mito BAM CollectMultipleMetrics insert size" \
  --picard "$workdir/picard-multi.insert_size_metrics" \
  --turbo "$workdir/turbo-multi.insert_size_metrics"

picard CollectWgsMetrics \
  "I=$input_bam" "O=$workdir/picard-wgs.txt" "R=$reference"
turbo CollectWgsMetrics \
  "I=$input_bam" "O=$workdir/turbo-wgs.txt" "R=$reference"
python3 "$compare" metrics --label "GATK mito BAM CollectWgsMetrics" \
  --picard "$workdir/picard-wgs.txt" --turbo "$workdir/turbo-wgs.txt"

header_sam="$workdir/replacement-header.sam"
printf '@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chrM\tLN:16569\n@RG\tID:rg1\tSM:sample\tLB:lib\tPL:ILLUMINA\n' > "$header_sam"
picard ReplaceSamHeader \
  "I=$input_bam" "O=$workdir/picard-reheader.bam" "HEADER=$header_sam"
turbo ReplaceSamHeader \
  "I=$input_bam" "O=$workdir/turbo-reheader.bam" "HEADER=$header_sam"
view_to_sam "$workdir/picard-reheader.bam" "$workdir/picard-reheader.sam"
view_to_sam "$workdir/turbo-reheader.bam" "$workdir/turbo-reheader.sam"
python3 "$compare" stable-sam --label "GATK mito BAM ReplaceSamHeader" \
  --picard "$workdir/picard-reheader.sam" --turbo "$workdir/turbo-reheader.sam"

picard MergeSamFiles \
  "I=$input_bam" "I=$input_bam" "O=$workdir/picard-merged.bam" ASSUME_SORTED=true
turbo MergeSamFiles \
  "I=$input_bam" "I=$input_bam" "O=$workdir/turbo-merged.bam" ASSUME_SORTED=true
view_to_sam "$workdir/picard-merged.bam" "$workdir/picard-merged.sam"
view_to_sam "$workdir/turbo-merged.bam" "$workdir/turbo-merged.sam"
python3 "$compare" merge-multiset --label "GATK mito BAM MergeSamFiles" \
  --picard "$workdir/picard-merged.sam" --turbo "$workdir/turbo-merged.sam"

picard SortSam "I=$input_bam" "O=$workdir/picard-qn.bam" SORT_ORDER=queryname
turbo SortSam "I=$input_bam" "O=$workdir/turbo-qn.bam" SORT_ORDER=queryname
picard FixMateInformation \
  "I=$workdir/picard-qn.bam" "O=$workdir/picard-fixmate.bam" SORT_ORDER=coordinate
turbo FixMateInformation \
  "I=$workdir/turbo-qn.bam" "O=$workdir/turbo-fixmate.bam" SORT_ORDER=coordinate
view_to_sam "$workdir/picard-fixmate.bam" "$workdir/picard-fixmate.sam"
view_to_sam "$workdir/turbo-fixmate.bam" "$workdir/turbo-fixmate.sam"
python3 "$compare" stable-sam --label "GATK mito BAM FixMateInformation" \
  --picard "$workdir/picard-fixmate.sam" --turbo "$workdir/turbo-fixmate.sam"

echo "GATK mitochondrial BAM parity passed: ViewSam, CleanSam, AddOrReplaceReadGroups, MarkDuplicates, SortSam, ValidateSamFile, quality, chart, GC-bias, BuildBamIndex, RevertSam, SamToFastq, CollectMultipleMetrics, CollectWgsMetrics, ReplaceSamHeader, MergeSamFiles, FixMateInformation"
