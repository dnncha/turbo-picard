#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=tools/cram_compat.sh
source "$repo_root/tools/cram_compat.sh"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT
parity_conda_setup "$repo_root" "$workdir"

cat > "$workdir/Rscript" <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod +x "$workdir/Rscript"

reference="$repo_root/fixtures/reference/chr1.fa"
input_bam="$repo_root/fixtures/markduplicates/paired/input.bam"
input_cram="$workdir/input.cram"
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

sam_to_cram() {
  local input=$1
  local output=$2
  local temp_bam="${output%.cram}.bam"
  samtools view -T "$reference" -bS -o "$temp_bam" "$input"
  bam_to_cram "$temp_bam" "$output"
}

view_to_sam() {
  parity_view_to_sam "$reference" "$1" "$2" "$repo_root"
}

cargo test -q -p turbo-picard-markdup marks_duplicates_on_cram_input_and_output

bam_to_cram "$input_bam" "$input_cram"

cat > "$workdir/clean.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
mapped	0	chr1	2	60	4M	*	0	0	ACGT	FFFF
overhang	0	chr1	998	60	5M	*	0	0	ACGTA	FFFFF
unmapped	4	*	0	0	*	*	0	0	NNNN	!!!!
SAM

cat > "$workdir/merge-a.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-c	0	chr1	90	60	10M	*	0	0	CCCCCCCCCC	FFFFFFFFFF
SAM

cat > "$workdir/merge-b.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-a	0	chr1	10	60	10M	*	0	0	AAAAAAAAAA	FFFFFFFFFF
SAM

cat > "$workdir/fastq.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
read-a	77	*	0	0	*	*	0	0	AAAA	FFFF
read-a	141	*	0	0	*	*	0	0	TTTT	HHHH
SAM

cat > "$workdir/revert.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
@RG	ID:rg1	SM:sample	LB:lib	PL:ILLUMINA
pair1	1123	chr1	10	60	4M	=	30	24	ACGT	!!!!	RG:Z:rg1	OQ:Z:FFFF	NM:i:0	MD:Z:4	PG:Z:align
pair1	1171	chr1	30	60	4M	=	10	-24	TGCA	!!!!	RG:Z:rg1	OQ:Z:EEEE	NM:i:0	MD:Z:4	PG:Z:align
SAM

cat > "$workdir/header.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
@RG	ID:old	LB:old-lib	PL:ILLUMINA	PU:old-unit	SM:old-sample
@CO	replacement header
SAM

cat > "$workdir/rg.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
@RG	ID:old	LB:old-lib	PL:ILLUMINA	PU:old-unit	SM:old-sample
read-a	0	chr1	10	60	4M	*	0	0	ACGT	FFFF	RG:Z:old
SAM

cat > "$workdir/valid.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
@RG	ID:rg1	SM:sample	PL:ILLUMINA
read1	0	chr1	1	60	4M	*	0	0	ACGT	FFFF	RG:Z:rg1	NM:i:0
SAM

cat > "$workdir/metrics.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
@RG	ID:rg1	SM:sample	LB:lib	PL:ILLUMINA
pair1	99	chr1	1	60	10M	=	21	30	NNNNNNNNNN	FFFFFFFFFF	RG:Z:rg1	NM:i:0
pair1	147	chr1	21	60	10M	=	1	-30	NNNNNNNNNN	FFFFFFFFFF	RG:Z:rg1	NM:i:0
SAM

for sam in clean merge-a merge-b fastq revert rg valid metrics; do
  sam_to_cram "$workdir/${sam}.sam" "$workdir/${sam}.cram"
done

# MarkDuplicates (BAM control + CRAM semantic parity)
picard MarkDuplicates \
  "I=$input_bam" "O=$workdir/picard-markdup.bam" "M=$workdir/picard-markdup-bam.metrics.txt" \
  ASSUME_SORTED=true
turbo MarkDuplicates \
  "I=$input_bam" "O=$workdir/turbo-markdup.bam" "M=$workdir/turbo-markdup-bam.metrics.txt" \
  ASSUME_SORTED=true
python3 "$compare" metrics --label "MarkDuplicates BAM" \
  --picard "$workdir/picard-markdup-bam.metrics.txt" \
  --turbo "$workdir/turbo-markdup-bam.metrics.txt"

picard MarkDuplicates \
  "I=$input_cram" "O=$workdir/picard-markdup.cram" "M=$workdir/picard-markdup-cram.metrics.txt" \
  "R=$reference" ASSUME_SORTED=true
turbo MarkDuplicates \
  "I=$input_cram" "O=$workdir/turbo-markdup.cram" "M=$workdir/turbo-markdup-cram.metrics.txt" \
  "R=$reference" ASSUME_SORTED=true
view_to_sam "$workdir/picard-markdup.cram" "$workdir/picard-markdup.cram.sam"
view_to_sam "$workdir/turbo-markdup.cram" "$workdir/turbo-markdup.cram.sam"
python3 "$compare" markduplicates --label "MarkDuplicates CRAM" --repo-root "$repo_root" \
  --picard-alignment "$workdir/picard-markdup.cram.sam" \
  --turbo-alignment "$workdir/turbo-markdup.cram.sam" \
  --picard-metrics "$workdir/picard-markdup-cram.metrics.txt" \
  --turbo-metrics "$workdir/turbo-markdup-cram.metrics.txt"

# SortSam
picard SortSam "I=$input_cram" "O=$workdir/picard-SortSam.cram" "R=$reference" SORT_ORDER=coordinate
turbo SortSam "I=$input_cram" "O=$workdir/turbo-SortSam.cram" "R=$reference" SORT_ORDER=coordinate
view_to_sam "$workdir/picard-SortSam.cram" "$workdir/picard-SortSam.sam"
view_to_sam "$workdir/turbo-SortSam.cram" "$workdir/turbo-SortSam.sam"
python3 "$compare" merge-multiset --label SortSam \
  --picard "$workdir/picard-SortSam.sam" --turbo "$workdir/turbo-SortSam.sam"

# ViewSam
picard ViewSam "I=$input_cram" "R=$reference" > "$workdir/picard-ViewSam.sam"
turbo ViewSam "I=$input_cram" "R=$reference" > "$workdir/turbo-ViewSam.sam"
python3 "$compare" records-ignore-md-nm --label ViewSam --reference "$reference" \
  --picard "$workdir/picard-ViewSam.sam" --turbo "$workdir/turbo-ViewSam.sam"

# CleanSam
picard CleanSam "I=$workdir/clean.cram" "O=$workdir/picard-CleanSam.cram" "R=$reference"
turbo CleanSam "I=$workdir/clean.cram" "O=$workdir/turbo-CleanSam.cram" "R=$reference"
view_to_sam "$workdir/picard-CleanSam.cram" "$workdir/picard-CleanSam.sam"
view_to_sam "$workdir/turbo-CleanSam.cram" "$workdir/turbo-CleanSam.sam"
python3 "$compare" cleansam --label CleanSam \
  --picard "$workdir/picard-CleanSam.sam" --turbo "$workdir/turbo-CleanSam.sam"

# MergeSamFiles
picard MergeSamFiles \
  "I=$workdir/merge-a.cram" "I=$workdir/merge-b.cram" \
  "O=$workdir/picard-MergeSamFiles.cram" "R=$reference" SORT_ORDER=coordinate AS=true
turbo MergeSamFiles \
  "I=$workdir/merge-a.cram" "I=$workdir/merge-b.cram" \
  "O=$workdir/turbo-MergeSamFiles.cram" "R=$reference" SORT_ORDER=coordinate AS=true
view_to_sam "$workdir/picard-MergeSamFiles.cram" "$workdir/picard-MergeSamFiles.sam"
view_to_sam "$workdir/turbo-MergeSamFiles.cram" "$workdir/turbo-MergeSamFiles.sam"
python3 "$compare" merge-multiset --label MergeSamFiles \
  --picard "$workdir/picard-MergeSamFiles.sam" --turbo "$workdir/turbo-MergeSamFiles.sam"

# SamToFastq
picard SamToFastq \
  "I=$workdir/fastq.cram" "FASTQ=$workdir/picard-r1.fastq" \
  "SECOND_END_FASTQ=$workdir/picard-r2.fastq" "R=$reference"
turbo SamToFastq \
  "I=$workdir/fastq.cram" "FASTQ=$workdir/turbo-r1.fastq" \
  "SECOND_END_FASTQ=$workdir/turbo-r2.fastq" "R=$reference"
diff -u "$workdir/picard-r1.fastq" "$workdir/turbo-r1.fastq"
diff -u "$workdir/picard-r2.fastq" "$workdir/turbo-r2.fastq"

# RevertSam
picard RevertSam "I=$workdir/revert.cram" "O=$workdir/picard-RevertSam.cram" "R=$reference"
turbo RevertSam "I=$workdir/revert.cram" "O=$workdir/turbo-RevertSam.cram" "R=$reference"
python3 "$compare" records --label RevertSam --reference "$reference" \
  --picard "$workdir/picard-RevertSam.cram" --turbo "$workdir/turbo-RevertSam.cram"

# ReplaceSamHeader
picard ReplaceSamHeader \
  "I=$workdir/rg.cram" "HEADER=$workdir/header.sam" "O=$workdir/picard-ReplaceSamHeader.cram" \
  "R=$reference"
turbo ReplaceSamHeader \
  "I=$workdir/rg.cram" "HEADER=$workdir/header.sam" "O=$workdir/turbo-ReplaceSamHeader.cram" \
  "R=$reference"
view_to_sam "$workdir/picard-ReplaceSamHeader.cram" "$workdir/picard-ReplaceSamHeader.sam"
view_to_sam "$workdir/turbo-ReplaceSamHeader.cram" "$workdir/turbo-ReplaceSamHeader.sam"
python3 - "$workdir/picard-ReplaceSamHeader.sam" "$workdir/turbo-ReplaceSamHeader.sam" <<'PY'
import sys

def header_lines(path):
    with open(path, encoding="utf-8") as handle:
        return [
            line.rstrip("\n")
            for line in handle
            if line.startswith("@") and not line.startswith("@PG")
        ]

def record_names(path):
    with open(path, encoding="utf-8") as handle:
        return [line.split("\t", 1)[0] for line in handle if not line.startswith("@")]

if header_lines(sys.argv[1]) != header_lines(sys.argv[2]):
    raise SystemExit("ReplaceSamHeader CRAM header differs from Picard")
if record_names(sys.argv[1]) != record_names(sys.argv[2]):
    raise SystemExit("ReplaceSamHeader CRAM records differ from Picard")
print("ReplaceSamHeader CRAM parity check passed")
PY

# AddOrReplaceReadGroups
picard AddOrReplaceReadGroups \
  "I=$workdir/rg.cram" "O=$workdir/picard-AddOrReplaceReadGroups.cram" "R=$reference" \
  RGID=new RGLB=library-a RGPL=ILLUMINA RGPU=unit-a RGSM=sample-a
turbo AddOrReplaceReadGroups \
  "I=$workdir/rg.cram" "O=$workdir/turbo-AddOrReplaceReadGroups.cram" "R=$reference" \
  RGID=new RGLB=library-a RGPL=ILLUMINA RGPU=unit-a RGSM=sample-a
view_to_sam "$workdir/picard-AddOrReplaceReadGroups.cram" "$workdir/picard-AddOrReplaceReadGroups.sam"
view_to_sam "$workdir/turbo-AddOrReplaceReadGroups.cram" "$workdir/turbo-AddOrReplaceReadGroups.sam"
python3 "$compare" stable-sam --label AddOrReplaceReadGroups \
  --picard "$workdir/picard-AddOrReplaceReadGroups.sam" --turbo "$workdir/turbo-AddOrReplaceReadGroups.sam"

# ValidateSamFile
picard_validate_exit=0
picard ValidateSamFile "I=$workdir/valid.cram" "O=$workdir/picard-validate.txt" "R=$reference" MODE=SUMMARY \
  || picard_validate_exit=$?
turbo_validate_exit=0
turbo ValidateSamFile "I=$workdir/valid.cram" "O=$workdir/turbo-validate.txt" "R=$reference" MODE=SUMMARY \
  || turbo_validate_exit=$?
if [[ "$picard_validate_exit" != "$turbo_validate_exit" ]]; then
  echo "ValidateSamFile CRAM exit differs from Picard: Picard=$picard_validate_exit turbo=$turbo_validate_exit" >&2
  exit 1
fi
python3 "$compare" validate-summary --label ValidateSamFile \
  --picard "$workdir/picard-validate.txt" --turbo "$workdir/turbo-validate.txt"

# FixMateInformation
cat > "$workdir/fixmate.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
pair1	99	chr1	10	60	4M	*	0	0	ACGT	FFFF
pair1	147	chr1	30	60	4M	*	0	0	TGCA	FFFF
single	0	chr1	50	60	4M	*	0	0	AAAA	FFFF
SAM
sam_to_cram "$workdir/fixmate.sam" "$workdir/fixmate.cram"
picard FixMateInformation \
  "I=$workdir/fixmate.cram" "O=$workdir/picard-FixMateInformation.cram" \
  "R=$reference" ASSUME_SORTED=true SORT_ORDER=queryname
turbo FixMateInformation \
  "I=$workdir/fixmate.cram" "O=$workdir/turbo-FixMateInformation.cram" \
  "R=$reference" ASSUME_SORTED=true SORT_ORDER=queryname
view_to_sam "$workdir/picard-FixMateInformation.cram" "$workdir/picard-FixMateInformation.sam"
view_to_sam "$workdir/turbo-FixMateInformation.cram" "$workdir/turbo-FixMateInformation.sam"
python3 "$compare" stable-sam --label FixMateInformation \
  --picard "$workdir/picard-FixMateInformation.sam" --turbo "$workdir/turbo-FixMateInformation.sam"

# SetNmMdAndUqTags
cat > "$workdir/setnmmd.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read1	0	chr1	1	60	4M	*	0	0	ACGA	FFFF
read2	0	chr1	5	60	2M1I2M	*	0	0	ACGTA	FFFFF
read3	0	chr1	8	60	2M1D2M	*	0	0	TACG	FFFF
SAM
sam_to_cram "$workdir/setnmmd.sam" "$workdir/setnmmd.cram"
picard SetNmMdAndUqTags \
  "I=$workdir/setnmmd.cram" "O=$workdir/picard-SetNmMd.cram" \
  "R=$reference"
turbo SetNmMdAndUqTags \
  "I=$workdir/setnmmd.cram" "O=$workdir/turbo-SetNmMd.cram" \
  "R=$reference"
view_to_sam "$workdir/picard-SetNmMd.cram" "$workdir/picard-SetNmMd.sam"
view_to_sam "$workdir/turbo-SetNmMd.cram" "$workdir/turbo-SetNmMd.sam"
python3 "$compare" stable-sam-sorted-tags --label SetNmMdAndUqTags \
  --picard "$workdir/picard-SetNmMd.sam" --turbo "$workdir/turbo-SetNmMd.sam"

# BuildBamIndex
# CollectQualityYieldMetrics
picard CollectQualityYieldMetrics \
  "I=$workdir/metrics.cram" "O=$workdir/picard-quality-yield.txt" "R=$reference"
turbo CollectQualityYieldMetrics \
  "I=$workdir/metrics.cram" "O=$workdir/turbo-quality-yield.txt" "R=$reference"
python3 "$compare" metrics --label CollectQualityYieldMetrics \
  --picard "$workdir/picard-quality-yield.txt" --turbo "$workdir/turbo-quality-yield.txt"

# CollectAlignmentSummaryMetrics
picard CollectAlignmentSummaryMetrics \
  "I=$workdir/metrics.cram" "O=$workdir/picard-alignment-summary.txt" "R=$reference"
turbo CollectAlignmentSummaryMetrics \
  "I=$workdir/metrics.cram" "O=$workdir/turbo-alignment-summary.txt" "R=$reference"
python3 "$compare" metrics --label CollectAlignmentSummaryMetrics \
  --picard "$workdir/picard-alignment-summary.txt" --turbo "$workdir/turbo-alignment-summary.txt"

# CollectInsertSizeMetrics
picard CollectInsertSizeMetrics \
  "I=$workdir/metrics.cram" "O=$workdir/picard-insert-size.txt" "H=$workdir/picard-insert-size.pdf" \
  "R=$reference"
turbo CollectInsertSizeMetrics \
  "I=$workdir/metrics.cram" "O=$workdir/turbo-insert-size.txt" "H=$workdir/turbo-insert-size.pdf" \
  "R=$reference"
python3 "$compare" metrics --label CollectInsertSizeMetrics \
  --picard "$workdir/picard-insert-size.txt" --turbo "$workdir/turbo-insert-size.txt"

# CollectBaseDistributionByCycle
picard CollectBaseDistributionByCycle \
  "I=$input_cram" "O=$workdir/picard-base-cycle.txt" "CHART=$workdir/picard-base-cycle.pdf" \
  "R=$reference"
turbo CollectBaseDistributionByCycle \
  "I=$input_cram" "O=$workdir/turbo-base-cycle.txt" "CHART=$workdir/turbo-base-cycle.pdf" \
  "R=$reference"
python3 "$compare" metrics --label CollectBaseDistributionByCycle \
  --picard "$workdir/picard-base-cycle.txt" --turbo "$workdir/turbo-base-cycle.txt"

# MeanQualityByCycle
picard MeanQualityByCycle \
  "I=$input_cram" "O=$workdir/picard-mean-quality.txt" "CHART=$workdir/picard-mean-quality.pdf" \
  "R=$reference"
turbo MeanQualityByCycle \
  "I=$input_cram" "O=$workdir/turbo-mean-quality.txt" "CHART=$workdir/turbo-mean-quality.pdf" \
  "R=$reference"
python3 "$compare" metrics --label MeanQualityByCycle \
  --picard "$workdir/picard-mean-quality.txt" --turbo "$workdir/turbo-mean-quality.txt"

# QualityScoreDistribution
picard QualityScoreDistribution \
  "I=$input_cram" "O=$workdir/picard-quality-score.txt" "CHART=$workdir/picard-quality-score.pdf" \
  "R=$reference"
turbo QualityScoreDistribution \
  "I=$input_cram" "O=$workdir/turbo-quality-score.txt" "CHART=$workdir/turbo-quality-score.pdf" \
  "R=$reference"
python3 "$compare" metrics --label QualityScoreDistribution \
  --picard "$workdir/picard-quality-score.txt" --turbo "$workdir/turbo-quality-score.txt"

echo "CRAM hot-path parity passed for: MarkDuplicates, SortSam, ViewSam, CleanSam, MergeSamFiles, SamToFastq, RevertSam, ReplaceSamHeader, AddOrReplaceReadGroups, ValidateSamFile, FixMateInformation, SetNmMdAndUqTags, CollectQualityYieldMetrics, CollectAlignmentSummaryMetrics, CollectInsertSizeMetrics, CollectBaseDistributionByCycle, MeanQualityByCycle, QualityScoreDistribution"
