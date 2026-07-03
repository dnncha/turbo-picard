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

picard ViewSam "I=$input_cram" "R=$reference" > "$workdir/picard-viewsam.sam"
turbo ViewSam "I=$input_cram" "R=$reference" > "$workdir/turbo-viewsam.sam"
python3 "$compare" stable-sam-ignore-md-nm --label "GATK mito CRAM ViewSam" \
  --picard "$workdir/picard-viewsam.sam" --turbo "$workdir/turbo-viewsam.sam"

picard CleanSam "I=$input_cram" "O=$workdir/picard-cleansam.cram" "R=$reference"
turbo CleanSam "I=$input_cram" "O=$workdir/turbo-cleansam.cram" "R=$reference"
view_to_sam "$workdir/picard-cleansam.cram" "$workdir/picard-cleansam.sam"
view_to_sam "$workdir/turbo-cleansam.cram" "$workdir/turbo-cleansam.sam"
python3 "$compare" cleansam --label "GATK mito CRAM CleanSam" \
  --picard "$workdir/picard-cleansam.sam" --turbo "$workdir/turbo-cleansam.sam"

picard AddOrReplaceReadGroups \
  "I=$input_cram" "O=$workdir/picard-addrg.cram" "R=$reference" \
  RGID=turbo RGLB=library RGPL=ILLUMINA RGPU=unit RGSM=sample
turbo AddOrReplaceReadGroups \
  "I=$input_cram" "O=$workdir/turbo-addrg.cram" "R=$reference" \
  RGID=turbo RGLB=library RGPL=ILLUMINA RGPU=unit RGSM=sample
view_to_sam "$workdir/picard-addrg.cram" "$workdir/picard-addrg.sam"
view_to_sam "$workdir/turbo-addrg.cram" "$workdir/turbo-addrg.sam"
PYTHONPATH="$repo_root/tools" python3 - "$workdir/picard-addrg.sam" "$workdir/turbo-addrg.sam" <<'PY'
import sys
from pathlib import Path

import compare_real_data

picard = Path(sys.argv[1])
turbo = Path(sys.argv[2])
if compare_real_data.digest_sam_records_and_read_groups(picard) != (
    compare_real_data.digest_sam_records_and_read_groups(turbo)
):
    raise SystemExit("GATK mito CRAM AddOrReplaceReadGroups differs from Picard")
print("GATK mito CRAM AddOrReplaceReadGroups parity check passed")
PY

picard MarkDuplicates \
  "I=$input_cram" "O=$workdir/picard-markdup.cram" "M=$workdir/picard-markdup.metrics.txt" \
  "R=$reference" ASSUME_SORTED=true
turbo MarkDuplicates \
  "I=$input_cram" "O=$workdir/turbo-markdup.cram" "M=$workdir/turbo-markdup.metrics.txt" \
  "R=$reference" ASSUME_SORTED=true
view_to_sam "$workdir/picard-markdup.cram" "$workdir/picard-markdup.sam"
view_to_sam "$workdir/turbo-markdup.cram" "$workdir/turbo-markdup.sam"
python3 "$compare" markduplicates --label "GATK mito CRAM MarkDuplicates" --repo-root "$repo_root" \
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
python3 "$compare" merge-multiset --label "GATK mito CRAM SortSam" \
  --picard "$workdir/picard-sorted.sam" --turbo "$workdir/turbo-sorted.sam"

picard_validate_exit=0
picard ValidateSamFile \
  "I=$input_cram" "O=$workdir/picard-validate.txt" "R=$reference" MODE=SUMMARY \
  || picard_validate_exit=$?
turbo_validate_exit=0
turbo ValidateSamFile \
  "I=$input_cram" "O=$workdir/turbo-validate.txt" "R=$reference" MODE=SUMMARY \
  || turbo_validate_exit=$?
if [[ "$picard_validate_exit" != "$turbo_validate_exit" ]]; then
  echo "GATK mito CRAM ValidateSamFile exit differs from Picard: Picard=$picard_validate_exit turbo=$turbo_validate_exit" >&2
  exit 1
fi
python3 "$compare" validate-summary --label "GATK mito CRAM ValidateSamFile" \
  --picard "$workdir/picard-validate.txt" --turbo "$workdir/turbo-validate.txt"

picard CollectQualityYieldMetrics \
  "I=$input_cram" "O=$workdir/picard-quality-yield.txt" "R=$reference"
turbo CollectQualityYieldMetrics \
  "I=$input_cram" "O=$workdir/turbo-quality-yield.txt" "R=$reference"
python3 "$compare" metrics --label "GATK mito CRAM CollectQualityYieldMetrics" \
  --picard "$workdir/picard-quality-yield.txt" --turbo "$workdir/turbo-quality-yield.txt"

picard CollectAlignmentSummaryMetrics \
  "I=$input_cram" "O=$workdir/picard-alignment-summary.txt" "R=$reference"
turbo CollectAlignmentSummaryMetrics \
  "I=$input_cram" "O=$workdir/turbo-alignment-summary.txt" "R=$reference"
python3 "$compare" metrics --label "GATK mito CRAM CollectAlignmentSummaryMetrics" \
  --picard "$workdir/picard-alignment-summary.txt" --turbo "$workdir/turbo-alignment-summary.txt"

picard CollectInsertSizeMetrics \
  "I=$input_cram" "O=$workdir/picard-insert-size.txt" "H=$workdir/picard-insert-size.pdf" \
  "R=$reference"
turbo CollectInsertSizeMetrics \
  "I=$input_cram" "O=$workdir/turbo-insert-size.txt" "H=$workdir/turbo-insert-size.pdf" \
  "R=$reference"
python3 "$compare" metrics --label "GATK mito CRAM CollectInsertSizeMetrics" \
  --picard "$workdir/picard-insert-size.txt" --turbo "$workdir/turbo-insert-size.txt"

picard MeanQualityByCycle \
  "I=$input_cram" "O=$workdir/picard-mean-quality.txt" "CHART=$workdir/picard-mean-quality.pdf" \
  "R=$reference"
turbo MeanQualityByCycle \
  "I=$input_cram" "O=$workdir/turbo-mean-quality.txt" "CHART=$workdir/turbo-mean-quality.pdf" \
  "R=$reference"
python3 "$compare" metrics --label "GATK mito CRAM MeanQualityByCycle" \
  --picard "$workdir/picard-mean-quality.txt" --turbo "$workdir/turbo-mean-quality.txt"

picard QualityScoreDistribution \
  "I=$input_cram" "O=$workdir/picard-quality-score.txt" "CHART=$workdir/picard-quality-score.pdf" \
  "R=$reference"
turbo QualityScoreDistribution \
  "I=$input_cram" "O=$workdir/turbo-quality-score.txt" "CHART=$workdir/turbo-quality-score.pdf" \
  "R=$reference"
python3 "$compare" metrics --label "GATK mito CRAM QualityScoreDistribution" \
  --picard "$workdir/picard-quality-score.txt" --turbo "$workdir/turbo-quality-score.txt"

picard CollectBaseDistributionByCycle \
  "I=$input_cram" "O=$workdir/picard-base-cycle.txt" "CHART=$workdir/picard-base-cycle.pdf" \
  "R=$reference"
turbo CollectBaseDistributionByCycle \
  "I=$input_cram" "O=$workdir/turbo-base-cycle.txt" "CHART=$workdir/turbo-base-cycle.pdf" \
  "R=$reference"
python3 "$compare" metrics --label "GATK mito CRAM CollectBaseDistributionByCycle" \
  --picard "$workdir/picard-base-cycle.txt" --turbo "$workdir/turbo-base-cycle.txt"

picard CollectGcBiasMetrics \
  "I=$input_cram" "O=$workdir/picard-gc-detail.txt" \
  "S=$workdir/picard-gc-summary.txt" "CHART=$workdir/picard-gc.pdf" \
  "R=$reference"
turbo CollectGcBiasMetrics \
  "I=$input_cram" "O=$workdir/turbo-gc-detail.txt" \
  "S=$workdir/turbo-gc-summary.txt" "CHART=$workdir/turbo-gc.pdf" \
  "R=$reference"
python3 "$compare" metrics --label "GATK mito CRAM CollectGcBiasMetrics detail" \
  --picard "$workdir/picard-gc-detail.txt" --turbo "$workdir/turbo-gc-detail.txt"
python3 "$compare" metrics --label "GATK mito CRAM CollectGcBiasMetrics summary" \
  --picard "$workdir/picard-gc-summary.txt" --turbo "$workdir/turbo-gc-summary.txt"

picard RevertSam "I=$input_cram" "O=$workdir/picard-revert.sam" "R=$reference"
turbo RevertSam "I=$input_cram" "O=$workdir/turbo-revert.sam" "R=$reference"
python3 "$compare" stable-sam-ignore-md-nm --label "GATK mito CRAM RevertSam" \
  --picard "$workdir/picard-revert.sam" --turbo "$workdir/turbo-revert.sam"

picard SamToFastq \
  "I=$input_cram" "R=$reference" "FASTQ=$workdir/picard-r1.fastq" \
  "SECOND_END_FASTQ=$workdir/picard-r2.fastq" "UNPAIRED_FASTQ=$workdir/picard-unpaired.fastq"
turbo SamToFastq \
  "I=$input_cram" "R=$reference" "FASTQ=$workdir/turbo-r1.fastq" \
  "SECOND_END_FASTQ=$workdir/turbo-r2.fastq" "UNPAIRED_FASTQ=$workdir/turbo-unpaired.fastq"
python3 "$compare" fastq-trio --label "GATK mito CRAM SamToFastq" \
  --picard-r1 "$workdir/picard-r1.fastq" --picard-r2 "$workdir/picard-r2.fastq" \
  --picard-unpaired "$workdir/picard-unpaired.fastq" \
  --turbo-r1 "$workdir/turbo-r1.fastq" --turbo-r2 "$workdir/turbo-r2.fastq" \
  --turbo-unpaired "$workdir/turbo-unpaired.fastq"

picard CollectMultipleMetrics \
  "I=$input_cram" "O=$workdir/picard-multi" "R=$reference" \
  PROGRAM=null PROGRAM=CollectQualityYieldMetrics PROGRAM=CollectInsertSizeMetrics \
  "TMP_DIR=$workdir"
turbo CollectMultipleMetrics \
  "I=$input_cram" "O=$workdir/turbo-multi" "R=$reference" \
  PROGRAM=null PROGRAM=CollectQualityYieldMetrics PROGRAM=CollectInsertSizeMetrics \
  "TMP_DIR=$workdir"
python3 "$compare" metrics --label "GATK mito CRAM CollectMultipleMetrics quality yield" \
  --picard "$workdir/picard-multi.quality_yield_metrics" \
  --turbo "$workdir/turbo-multi.quality_yield_metrics"
python3 "$compare" metrics --label "GATK mito CRAM CollectMultipleMetrics insert size" \
  --picard "$workdir/picard-multi.insert_size_metrics" \
  --turbo "$workdir/turbo-multi.insert_size_metrics"

header_sam="$workdir/replacement-header.sam"
printf '@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chrM\tLN:16569\n@RG\tID:rg1\tSM:sample\tLB:lib\tPL:ILLUMINA\n' > "$header_sam"
picard ReplaceSamHeader \
  "I=$input_cram" "O=$workdir/picard-reheader.cram" "HEADER=$header_sam" "R=$reference"
turbo ReplaceSamHeader \
  "I=$input_cram" "O=$workdir/turbo-reheader.cram" "HEADER=$header_sam" "R=$reference"
view_to_sam "$workdir/picard-reheader.cram" "$workdir/picard-reheader.sam"
view_to_sam "$workdir/turbo-reheader.cram" "$workdir/turbo-reheader.sam"
python3 "$compare" stable-sam-ignore-md-nm --label "GATK mito CRAM ReplaceSamHeader" \
  --picard "$workdir/picard-reheader.sam" --turbo "$workdir/turbo-reheader.sam"

picard MergeSamFiles \
  "I=$input_cram" "I=$input_cram" "O=$workdir/picard-merged.cram" \
  "R=$reference" ASSUME_SORTED=true
turbo MergeSamFiles \
  "I=$input_cram" "I=$input_cram" "O=$workdir/turbo-merged.cram" \
  "R=$reference" ASSUME_SORTED=true
view_to_sam "$workdir/picard-merged.cram" "$workdir/picard-merged.sam"
view_to_sam "$workdir/turbo-merged.cram" "$workdir/turbo-merged.sam"
python3 "$compare" merge-multiset --label "GATK mito CRAM MergeSamFiles" \
  --picard "$workdir/picard-merged.sam" --turbo "$workdir/turbo-merged.sam"

picard CollectWgsMetrics \
  "I=$input_cram" "O=$workdir/picard-wgs.txt" "R=$reference"
turbo CollectWgsMetrics \
  "I=$input_cram" "O=$workdir/turbo-wgs.txt" "R=$reference"
python3 "$compare" wgs-metrics --label "GATK mito CRAM CollectWgsMetrics" \
  --picard "$workdir/picard-wgs.txt" --turbo "$workdir/turbo-wgs.txt"

picard SortSam \
  "I=$input_cram" "O=$workdir/picard-qn.cram" "R=$reference" SORT_ORDER=queryname
turbo SortSam \
  "I=$input_cram" "O=$workdir/turbo-qn.cram" "R=$reference" SORT_ORDER=queryname
picard FixMateInformation \
  "I=$workdir/picard-qn.cram" "O=$workdir/picard-fixmate.cram" \
  "R=$reference" SORT_ORDER=coordinate
turbo FixMateInformation \
  "I=$workdir/turbo-qn.cram" "O=$workdir/turbo-fixmate.cram" \
  "R=$reference" SORT_ORDER=coordinate
view_to_sam "$workdir/picard-fixmate.cram" "$workdir/picard-fixmate.sam"
view_to_sam "$workdir/turbo-fixmate.cram" "$workdir/turbo-fixmate.sam"
python3 "$compare" stable-sam-ignore-md-nm --label "GATK mito CRAM FixMateInformation" \
  --picard "$workdir/picard-fixmate.sam" --turbo "$workdir/turbo-fixmate.sam"

echo "GATK mitochondrial CRAM parity passed: ViewSam, CleanSam, AddOrReplaceReadGroups, MarkDuplicates, SortSam, ValidateSamFile, CollectQualityYieldMetrics, CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics, MeanQualityByCycle, QualityScoreDistribution, CollectBaseDistributionByCycle, CollectGcBiasMetrics, RevertSam, SamToFastq, CollectMultipleMetrics, CollectWgsMetrics, ReplaceSamHeader, MergeSamFiles, FixMateInformation"
