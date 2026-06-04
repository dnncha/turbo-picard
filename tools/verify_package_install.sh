#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tempdir="$(mktemp -d "${TMPDIR:-/tmp}/turbo-picard-package.XXXXXX")"
trap 'rm -rf "${tempdir}"' EXIT

install_root="${tempdir}/install"
shim_install_root="${tempdir}/shim-install"
output_bam="${tempdir}/marked.bam"
metrics="${tempdir}/metrics.txt"

for release_artifact in \
  "CITATION.cff" \
  "docs/command-matrix.yml" \
  "docs/parity.rst" \
  "benchmarks/real-data/manifest.json" \
  "docs/site/assets/benchmark-data.json" \
  "packaging/bioconda/turbo-picard/meta.yaml" \
  "packaging/bioconda/turbo-picard-picard-shim/meta.yaml"
do
  test -s "${repo_root}/${release_artifact}"
done

grep -q 'repository-code: "https://github.com/dnncha/turbo-picard"' "${repo_root}/CITATION.cff"
grep -q '^cff-version: 1.2.0$' "${repo_root}/CITATION.cff"
grep -q '^type: software$' "${repo_root}/CITATION.cff"
grep -q 'archived release' "${repo_root}/CITATION.cff"
grep -q '^commands:' "${repo_root}/docs/command-matrix.yml"
grep -q '^picard_reference: "3.4.0"$' "${repo_root}/docs/command-matrix.yml"
grep -q 'What Parity Means' "${repo_root}/docs/parity.rst"
grep -q 'specific command' "${repo_root}/docs/parity.rst"
grep -q 'does not prove broad switching safety' "${repo_root}/docs/parity.rst"
grep -q 'python3 tools/verify_real_data_evidence.py --release-ready' "${repo_root}/docs/parity.rst"
grep -q '"datasets"' "${repo_root}/benchmarks/real-data/manifest.json"
grep -q '"release_tier": "release_candidate"' "${repo_root}/benchmarks/real-data/manifest.json"
grep -q '"gatk-na12878-mito"' "${repo_root}/benchmarks/real-data/manifest.json"
grep -q '"benchmarks"' "${repo_root}/docs/site/assets/benchmark-data.json"
grep -q '"parity": "32/32 PASS"' "${repo_root}/docs/site/assets/benchmark-data.json"
grep -q '"geometric_mean_speedup"' "${repo_root}/docs/site/assets/benchmark-data.json"

python3 - "${repo_root}" <<'PY'
import json
import sys
from pathlib import Path

repo = Path(sys.argv[1])
required_portfolio = {
    "AddOrReplaceReadGroups",
    "BuildBamIndex",
    "CleanSam",
    "CollectAlignmentSummaryMetrics",
    "CollectInsertSizeMetrics",
    "CollectQualityYieldMetrics",
    "MarkDuplicates",
    "RevertSam",
    "SamToFastq",
    "SortSam",
    "ValidateSamFile",
    "ViewSam",
}

manifest = json.loads((repo / "benchmarks/real-data/manifest.json").read_text())
datasets = manifest.get("datasets")
if not isinstance(datasets, list):
    raise SystemExit("real-data manifest datasets must be a list")
release_commands = set()
for dataset in datasets:
    if not isinstance(dataset, dict):
        continue
    if dataset.get("release_tier") != "release_candidate":
        continue
    expected_commands = dataset.get("expected_commands", {})
    if isinstance(expected_commands, dict):
        release_commands.update(expected_commands)
missing = sorted(required_portfolio - release_commands)
if missing:
    raise SystemExit(
        "release_candidate manifest missing package-smoke command evidence: "
        + ", ".join(missing)
    )

benchmark_data = json.loads((repo / "docs/site/assets/benchmark-data.json").read_text())
summary = benchmark_data.get("summary")
if benchmark_data.get("parity") != "32/32 PASS":
    raise SystemExit("benchmark-data parity must be 32/32 PASS")
if not isinstance(summary, dict):
    raise SystemExit("benchmark-data summary must be an object")
for key in ("command_count", "parity_pass_count", "floor_speedup", "geometric_mean_speedup", "top_speedup"):
    value = summary.get(key)
    if not isinstance(value, (int, float)):
        raise SystemExit(f"benchmark-data summary missing numeric {key}")
if summary["command_count"] != summary["parity_pass_count"]:
    raise SystemExit("benchmark-data command_count and parity_pass_count differ")
PY

cargo install \
  --locked \
  --no-track \
  --root "${install_root}" \
  --path "${repo_root}/crates/turbo-picard-cli" \
  --bin turbo-picard

"${install_root}/bin/turbo-picard" --version
test ! -e "${install_root}/bin/picard"

"${install_root}/bin/turbo-picard" --help > "${tempdir}/turbo-picard-help.txt"
python3 - "${repo_root}/docs/command-matrix.yml" "${tempdir}/turbo-picard-help.txt" <<'PY'
import re
import sys
from pathlib import Path

matrix = Path(sys.argv[1]).read_text(encoding="utf-8")
help_text = Path(sys.argv[2]).read_text(encoding="utf-8")
commands = []
current = None
for line in matrix.splitlines():
    name = re.match(r"\s*-\s+name:\s+(\S+)", line)
    if name:
        current = name.group(1)
        continue
    status = re.match(r"\s+status:\s+(native|partial-native)\s*$", line)
    if status and current:
        commands.append(current)
        current = None
missing = [command for command in commands if command not in help_text]
if missing:
    raise SystemExit("installed turbo-picard help missing commands: " + ", ".join(missing))
PY

recipe_smoke_dir="${tempdir}/recipe-smoke"
mkdir -p "${recipe_smoke_dir}"
(
  cd "${recipe_smoke_dir}"
  PATH="${install_root}/bin:/usr/bin:/bin" \
    bash "${repo_root}/packaging/bioconda/turbo-picard/run_test.sh"
)

"${install_root}/bin/turbo-picard" MarkDuplicates \
  "I=${repo_root}/fixtures/markduplicates/basic/input.bam" \
  "O=${output_bam}" \
  "M=${metrics}" \
  ASSUME_SORTED=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  READ_NAME_REGEX=null

test -s "${output_bam}"
test -s "${metrics}"
grep -q 'UNPAIRED_READ_DUPLICATES' "${metrics}"

"${install_root}/bin/turbo-picard" BuildBamIndex \
  "I=${output_bam}" \
  "O=${tempdir}/marked.bai" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${tempdir}/marked.bai"

sortsam_input="${tempdir}/sortsam-input.sam"
sortsam_output="${tempdir}/sortsam-output.sam"
cat > "${sortsam_input}" <<'SAM'
@HD	VN:1.6	SO:unsorted
@SQ	SN:chr1	LN:1000
read-c	0	chr1	90	60	10M	*	0	0	CCCCCCCCCC	FFFFFFFFFF
read-a	0	chr1	10	60	10M	*	0	0	AAAAAAAAAA	FFFFFFFFFF
read-b	0	chr1	50	60	10M	*	0	0	BBBBBBBBBB	FFFFFFFFFF
SAM

"${install_root}/bin/turbo-picard" SortSam \
  "I=${sortsam_input}" \
  "O=${sortsam_output}" \
  SORT_ORDER=coordinate \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${sortsam_output}"
grep -q $'@HD\tVN:1.6\tSO:coordinate' "${sortsam_output}"
awk '!/^@/ { print $1 }' "${sortsam_output}" | tr '\n' ' ' | grep -q '^read-a read-b read-c $'

dirty_sam="${tempdir}/dirty.sam"
cleaned_sam="${tempdir}/cleaned.sam"
cat > "${dirty_sam}" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
mapped	0	chr1	10	60	4M	*	0	0	ACGT	FFFF
unmapped	4	*	0	60	*	*	0	0	NNNN	!!!!
SAM

"${install_root}/bin/turbo-picard" CleanSam \
  "I=${dirty_sam}" \
  "O=${cleaned_sam}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${cleaned_sam}"
grep -Fq $'unmapped\t4\t*\t0\t0\t*' "${cleaned_sam}"

viewsam_output="${tempdir}/view.sam"
"${install_root}/bin/turbo-picard" ViewSam \
  "I=${sortsam_output}" \
  "O=${viewsam_output}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${viewsam_output}"
grep -q '^read-a' "${viewsam_output}"

replacement_header="${tempdir}/replacement-header.sam"
reheadered_output="${tempdir}/reheadered.sam"
cat > "${replacement_header}" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:2000
@CO	replacement header
SAM

"${install_root}/bin/turbo-picard" ReplaceSamHeader \
  "I=${sortsam_output}" \
  "O=${reheadered_output}" \
  "H=${replacement_header}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${reheadered_output}"
grep -q $'@SQ\tSN:chr1\tLN:2000' "${reheadered_output}"
grep -q '^read-a' "${reheadered_output}"

mergesam_input_a="${tempdir}/mergesam-a.sam"
mergesam_input_b="${tempdir}/mergesam-b.sam"
mergesam_output="${tempdir}/mergesam-output.sam"
cat > "${mergesam_input_a}" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-c	0	chr1	90	60	10M	*	0	0	CCCCCCCCCC	FFFFFFFFFF
SAM
cat > "${mergesam_input_b}" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-a	0	chr1	10	60	10M	*	0	0	AAAAAAAAAA	FFFFFFFFFF
read-b	0	chr1	50	60	10M	*	0	0	BBBBBBBBBB	FFFFFFFFFF
SAM

"${install_root}/bin/turbo-picard" MergeSamFiles \
  "I=${mergesam_input_a}" \
  "I=${mergesam_input_b}" \
  "O=${mergesam_output}" \
  SORT_ORDER=coordinate \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${mergesam_output}"
grep -q $'@HD\tVN:1.6\tSO:coordinate' "${mergesam_output}"
awk '!/^@/ { print $1 }' "${mergesam_output}" | tr '\n' ' ' | grep -q '^read-a read-b read-c $'

fastq_output="${tempdir}/reads.fastq"
"${install_root}/bin/turbo-picard" SamToFastq \
  "I=${sortsam_input}" \
  "FASTQ=${fastq_output}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${fastq_output}"
grep -q '^@read-c$' "${fastq_output}"

fastq_filter_input="${tempdir}/fastq-filter.sam"
filtered_fastq="${tempdir}/filtered.fastq"
cat > "${fastq_filter_input}" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
pf	4	*	0	0	*	*	0	0	AAAA	FFFF
nonpf	516	*	0	0	*	*	0	0	CCCC	FFFF
secondary	260	*	0	0	*	*	0	0	GGGG	FFFF
SAM

"${install_root}/bin/turbo-picard" SamToFastq \
  "I=${fastq_filter_input}" \
  "FASTQ=${filtered_fastq}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

grep -q '^@pf$' "${filtered_fastq}"
! grep -q '^@nonpf$' "${filtered_fastq}"
! grep -q '^@secondary$' "${filtered_fastq}"

fastqtosam_r1="${tempdir}/fastqtosam-r1.fastq"
fastqtosam_r2="${tempdir}/fastqtosam-r2.fastq"
fastqtosam_output="${tempdir}/fastqtosam.sam"
cat > "${fastqtosam_r1}" <<'FQ'
@read1
ACGT
+
FFFF
FQ
cat > "${fastqtosam_r2}" <<'FQ'
@read1
TTTT
+
IIII
FQ

"${install_root}/bin/turbo-picard" FastqToSam \
  "F1=${fastqtosam_r1}" \
  "F2=${fastqtosam_r2}" \
  "O=${fastqtosam_output}" \
  SM=sample \
  RG=rg1 \
  QUALITY_FORMAT=Standard \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${fastqtosam_output}"
grep -q $'@RG\tID:rg1\tSM:sample' "${fastqtosam_output}"
grep -Fq $'read1\t77\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1' "${fastqtosam_output}"
grep -Fq $'read1\t141\t*\t0\t0\t*\t*\t0\t0\tTTTT\tIIII\tRG:Z:rg1' "${fastqtosam_output}"

readgroups_output="${tempdir}/readgroups.sam"
"${install_root}/bin/turbo-picard" AddOrReplaceReadGroups \
  "I=${sortsam_input}" \
  "O=${readgroups_output}" \
  RGID=new \
  RGLB=library-a \
  RGPL=ILLUMINA \
  RGPU=unit-a \
  RGSM=sample-a \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${readgroups_output}"
grep -q $'@RG\tID:new\tLB:library-a\tPL:ILLUMINA\tSM:sample-a\tPU:unit-a' "${readgroups_output}"
grep -q $'RG:Z:new' "${readgroups_output}"

alignment_metrics="${tempdir}/alignment_metrics.txt"
"${install_root}/bin/turbo-picard" CollectAlignmentSummaryMetrics \
  "I=${sortsam_input}" \
  "O=${alignment_metrics}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${alignment_metrics}"
grep -q 'picard.analysis.AlignmentSummaryMetrics' "${alignment_metrics}"
grep -q '^UNPAIRED' "${alignment_metrics}"

quality_metrics="${tempdir}/quality_yield_metrics.txt"
"${install_root}/bin/turbo-picard" CollectQualityYieldMetrics \
  "I=${sortsam_input}" \
  "O=${quality_metrics}" \
  STOP_AFTER=1 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${quality_metrics}"
grep -q 'CollectQualityYieldMetrics' "${quality_metrics}"

quality_distribution="${tempdir}/quality_score_distribution.txt"
quality_distribution_chart="${tempdir}/quality_score_distribution.pdf"
"${install_root}/bin/turbo-picard" QualityScoreDistribution \
  "I=${sortsam_input}" \
  "O=${quality_distribution}" \
  "CHART=${quality_distribution_chart}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${quality_distribution}"
test -s "${quality_distribution_chart}"
grep -q $'QUALITY\tCOUNT_OF_Q' "${quality_distribution}"

mean_quality="${tempdir}/mean_quality_by_cycle.txt"
mean_quality_chart="${tempdir}/mean_quality_by_cycle.pdf"
"${install_root}/bin/turbo-picard" MeanQualityByCycle \
  "I=${sortsam_input}" \
  "O=${mean_quality}" \
  "CHART=${mean_quality_chart}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${mean_quality}"
test -s "${mean_quality_chart}"
grep -q $'CYCLE\tMEAN_QUALITY' "${mean_quality}"

gc_reference="${tempdir}/gc_ref.fa"
gc_input="${tempdir}/gc_input.sam"
cat > "${gc_reference}" <<'FA'
>low
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
>high
CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC
FA
cat > "${gc_input}" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:low	LN:40
@SQ	SN:high	LN:40
low1	0	low	1	60	20M	*	0	0	AAAAAAAAAAAAAAAAAAAA	FFFFFFFFFFFFFFFFFFFF
high1	0	high	1	60	20M	*	0	0	CCCCCCCCCCCCCCCCCCCC	FFFFFFFFFFFFFFFFFFFF
SAM
gc_detail="${tempdir}/gc_bias.detail.txt"
gc_summary="${tempdir}/gc_bias.summary.txt"
gc_chart="${tempdir}/gc_bias.pdf"
"${install_root}/bin/turbo-picard" CollectGcBiasMetrics \
  "I=${gc_input}" \
  "O=${gc_detail}" \
  "S=${gc_summary}" \
  "CHART=${gc_chart}" \
  "R=${gc_reference}" \
  SCAN_WINDOW_SIZE=20 \
  MINIMUM_GENOME_FRACTION=0 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${gc_detail}"
test -s "${gc_summary}"
test -s "${gc_chart}"
grep -q 'picard.analysis.GcBiasDetailMetrics' "${gc_detail}"
grep -q 'picard.analysis.GcBiasSummaryMetrics' "${gc_summary}"

paired_input="${tempdir}/paired.sam"
cat > "${paired_input}" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
pair1	99	chr1	10	60	4M	=	30	24	ACGT	FFFF
pair1	147	chr1	30	60	4M	=	10	-24	TGCA	FFFF
pair2	99	chr1	100	60	4M	=	130	34	AAAA	FFFF
pair2	147	chr1	130	60	4M	=	100	-34	TTTT	FFFF
dup1	1123	chr1	200	60	4M	=	240	44	CCCC	FFFF
dup1	1171	chr1	240	60	4M	=	200	-44	GGGG	FFFF
SAM

insert_metrics="${tempdir}/insert_size_metrics.txt"
insert_histogram="${tempdir}/insert_size_histogram.pdf"
"${install_root}/bin/turbo-picard" CollectInsertSizeMetrics \
  "I=${paired_input}" \
  "O=${insert_metrics}" \
  "H=${insert_histogram}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${insert_metrics}"
test -s "${insert_histogram}"
grep -q 'picard.analysis.InsertSizeMetrics' "${insert_metrics}"
grep -q $'insert_size\tAll_Reads.fr_count' "${insert_metrics}"

insert_metrics_with_duplicates="${tempdir}/insert_size_metrics_with_duplicates.txt"
insert_histogram_with_duplicates="${tempdir}/insert_size_histogram_with_duplicates.pdf"
"${install_root}/bin/turbo-picard" CollectInsertSizeMetrics \
  "I=${paired_input}" \
  "O=${insert_metrics_with_duplicates}" \
  "H=${insert_histogram_with_duplicates}" \
  INCLUDE_DUPLICATES=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

grep -q $'44\t1' "${insert_metrics_with_duplicates}"

"${install_root}/bin/turbo-picard" CollectMultipleMetrics \
  "I=${paired_input}" \
  "O=${tempdir}/multiple" \
  PROGRAM=null \
  PROGRAM=CollectInsertSizeMetrics \
  PROGRAM=CollectBaseDistributionByCycle \
  PROGRAM=QualityScoreDistribution \
  PROGRAM=MeanQualityByCycle \
  PROGRAM=CollectQualityYieldMetrics \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${tempdir}/multiple.insert_size_metrics"
test -s "${tempdir}/multiple.insert_size_histogram.pdf"
test -s "${tempdir}/multiple.base_distribution_by_cycle_metrics"
test -s "${tempdir}/multiple.base_distribution_by_cycle.pdf"
test -s "${tempdir}/multiple.quality_distribution_metrics"
test -s "${tempdir}/multiple.quality_distribution.pdf"
test -s "${tempdir}/multiple.quality_by_cycle_metrics"
test -s "${tempdir}/multiple.quality_by_cycle.pdf"
test -s "${tempdir}/multiple.quality_yield_metrics"

"${install_root}/bin/turbo-picard" CollectMultipleMetrics \
  "I=${gc_input}" \
  "O=${tempdir}/multiple_gc" \
  "R=${gc_reference}" \
  PROGRAM=null \
  PROGRAM=CollectGcBiasMetrics \
  EXTRA_ARGUMENT=CollectGcBiasMetrics::SCAN_WINDOW_SIZE=20 \
  EXTRA_ARGUMENT=CollectGcBiasMetrics::MINIMUM_GENOME_FRACTION=0 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${tempdir}/multiple_gc.gc_bias.detail_metrics"
test -s "${tempdir}/multiple_gc.gc_bias.summary_metrics"
test -s "${tempdir}/multiple_gc.gc_bias.pdf"

"${install_root}/bin/turbo-picard" CollectMultipleMetrics \
  "I=${paired_input}" \
  "O=${tempdir}/multiple_default" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${tempdir}/multiple_default.alignment_summary_metrics"
test -s "${tempdir}/multiple_default.base_distribution_by_cycle_metrics"
test -s "${tempdir}/multiple_default.insert_size_metrics"
test -s "${tempdir}/multiple_default.quality_by_cycle_metrics"
test -s "${tempdir}/multiple_default.quality_distribution_metrics"
test ! -e "${tempdir}/multiple_default.quality_yield_metrics"

fixmate_input="${tempdir}/fixmate.sam"
fixmate_output="${tempdir}/fixed_mate.sam"
cat > "${fixmate_input}" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
pair1	99	chr1	10	60	4M	*	0	0	ACGT	FFFF
pair1	147	chr1	30	60	4M	*	0	0	TGCA	FFFF
SAM

"${install_root}/bin/turbo-picard" FixMateInformation \
  "I=${fixmate_input}" \
  "O=${fixmate_output}" \
  ASSUME_SORTED=true \
  SORT_ORDER=queryname \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${fixmate_output}"
grep -q $'pair1\t99\tchr1\t10\t60\t4M\t=\t30\t24' "${fixmate_output}"
grep -q $'MC:Z:4M\tMQ:i:60' "${fixmate_output}"

aligned_input="${tempdir}/aligned.sam"
reverted_output="${tempdir}/reverted.sam"
cat > "${aligned_input}" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
@RG	ID:rg1	SM:sample	LB:lib	PL:ILLUMINA
read1	1024	chr1	10	60	4M	*	0	0	ACGT	!!!!	RG:Z:rg1	OQ:Z:FFFF	NM:i:0	MD:Z:4	PG:Z:align
SAM

"${install_root}/bin/turbo-picard" RevertSam \
  "I=${aligned_input}" \
  "O=${reverted_output}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${reverted_output}"
grep -Fq $'read1\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1' "${reverted_output}"

reference="${tempdir}/reference.fa"
dictionary="${tempdir}/reference.dict"
cat > "${reference}" <<'FASTA'
>chr1
ACGTACGT
FASTA

"${install_root}/bin/turbo-picard" CreateSequenceDictionary \
  "R=${reference}" \
  "O=${dictionary}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${dictionary}"
grep -q $'@SQ\tSN:chr1\tLN:8\tM5:cc0af3a4fedb18378b4b57b98068e69f' "${dictionary}"

needs_tags="${tempdir}/needs_tags.sam"
tagged_sam="${tempdir}/tagged.sam"
cat > "${needs_tags}" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:8
read1	0	chr1	1	60	4M	*	0	0	ACGA	FFFF
SAM

"${install_root}/bin/turbo-picard" SetNmMdAndUqTags \
  "I=${needs_tags}" \
  "O=${tagged_sam}" \
  "R=${reference}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${tagged_sam}"
grep -q $'MD:Z:3T0\tNM:i:1\tUQ:i:37' "${tagged_sam}"

wgs_input="${tempdir}/wgs.sam"
wgs_metrics="${tempdir}/wgs_metrics.txt"
cat > "${wgs_input}" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:8
read1	0	chr1	1	60	4M	*	0	0	ACGT	FFFF
read2	0	chr1	5	60	4M	*	0	0	ACGT	FFFF
SAM

"${install_root}/bin/turbo-picard" CollectWgsMetrics \
  "I=${wgs_input}" \
  "O=${wgs_metrics}" \
  "R=${reference}" \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=0 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

grep -q $'GENOME_TERRITORY\tMEAN_COVERAGE' "${wgs_metrics}"
grep -q $'8\t1\t0\t1\t0\t0\t0\t0\t0\t0\t0\t0\t0\t1' "${wgs_metrics}"

wgs_intervals="${tempdir}/wgs_targets.interval_list"
wgs_interval_metrics="${tempdir}/wgs_interval_metrics.txt"
cat > "${wgs_intervals}" <<'INTERVALS'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:8
chr1	3	6	+	target
INTERVALS

"${install_root}/bin/turbo-picard" CollectWgsMetrics \
  "I=${wgs_input}" \
  "O=${wgs_interval_metrics}" \
  "R=${reference}" \
  "INTERVALS=${wgs_intervals}" \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=0 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

grep -q $'4\t1\t0\t1\t0\t0\t0\t0\t0\t0\t0\t0\t0\t1' "${wgs_interval_metrics}"

valid_for_validation="${tempdir}/valid_for_validation.sam"
validation_summary="${tempdir}/validation_summary.txt"
cat > "${valid_for_validation}" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:8
@RG	ID:rg1	SM:sample	PL:ILLUMINA
read1	0	chr1	1	60	4M	*	0	0	ACGT	FFFF	RG:Z:rg1	NM:i:0
SAM

"${install_root}/bin/turbo-picard" ValidateSamFile \
  "I=${valid_for_validation}" \
  "O=${validation_summary}" \
  MODE=SUMMARY \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

grep -q 'No errors found' "${validation_summary}"

chain="${tempdir}/identity.chain"
liftover_input="${tempdir}/liftover_input.vcf"
lifted_vcf="${tempdir}/lifted.vcf"
lifted_reject="${tempdir}/lifted.reject.vcf"
cat > "${chain}" <<'CHAIN'
chain 100 chr1 8 + 0 8 chr1 8 + 0 8 1
8
CHAIN

cat > "${liftover_input}" <<'VCF'
##fileformat=VCFv4.2
##contig=<ID=chr1,length=8>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
chr1	2	.	C	G	.	PASS	.
VCF

"${install_root}/bin/turbo-picard" LiftoverVcf \
  "I=${liftover_input}" \
  "O=${lifted_vcf}" \
  "CHAIN=${chain}" \
  "REJECT=${lifted_reject}" \
  "R=${reference}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

grep -q $'chr1\t2\t.\tC\tG\t.\tPASS\t.' "${lifted_vcf}"
test -s "${lifted_reject}"

normalized="${tempdir}/normalized.fa"
"${install_root}/bin/turbo-picard" NormalizeFasta \
  "I=${reference}" \
  "O=${normalized}" \
  LINE_LENGTH=4

test -s "${normalized}"
grep -q '^ACGT$' "${normalized}"

bed="${tempdir}/targets.bed"
interval_list="${tempdir}/targets.interval_list"
cat > "${bed}" <<'BED'
chr1	0	4	target	0	+
BED

"${install_root}/bin/turbo-picard" BedToIntervalList \
  "I=${bed}" \
  "O=${interval_list}" \
  "SD=${dictionary}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${interval_list}"
grep -q $'chr1\t1\t4\t+\ttarget' "${interval_list}"

extra_intervals="${tempdir}/extra.interval_list"
merged_intervals="${tempdir}/merged.interval_list"
cat > "${extra_intervals}" <<'EOF'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:8
chr1	4	8	+	extra
EOF

"${install_root}/bin/turbo-picard" IntervalListTools \
  "I=${interval_list}" \
  "I=${extra_intervals}" \
  "O=${merged_intervals}" \
  ACTION=CONCAT \
  SORT=true \
  UNIQUE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${merged_intervals}"
grep -q $'chr1\t1\t8\t+\ttarget|extra' "${merged_intervals}"

input_vcf="${tempdir}/input.vcf"
updated_vcf="${tempdir}/updated.vcf"
cat > "${input_vcf}" <<'VCF'
##fileformat=VCFv4.2
##contig=<ID=old,length=10>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
chr1	2	.	A	C	.	PASS	.
VCF

"${install_root}/bin/turbo-picard" UpdateVcfSequenceDictionary \
  "I=${input_vcf}" \
  "O=${updated_vcf}" \
  "SD=${dictionary}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${updated_vcf}"
grep -q '##contig=<ID=chr1,length=8' "${updated_vcf}"
grep -q $'chr1\t2\t.\tA\tC\t.\tPASS\t.' "${updated_vcf}"

shard2_raw_vcf="${tempdir}/shard2-raw.vcf"
shard2_vcf="${tempdir}/shard2.vcf"
gathered_vcf="${tempdir}/gathered.vcf"
sorted_vcf="${tempdir}/sorted.vcf"
cat > "${shard2_raw_vcf}" <<'VCF'
##fileformat=VCFv4.2
##contig=<ID=old,length=10>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
chr1	7	.	G	T	.	PASS	.
VCF

"${install_root}/bin/turbo-picard" UpdateVcfSequenceDictionary \
  "I=${shard2_raw_vcf}" \
  "O=${shard2_vcf}" \
  "SD=${dictionary}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${install_root}/bin/turbo-picard" GatherVcfs \
  "I=${updated_vcf}" \
  "I=${shard2_vcf}" \
  "O=${gathered_vcf}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${gathered_vcf}"
grep -q $'chr1\t7\t.\tG\tT\t.\tPASS\t.' "${gathered_vcf}"

"${install_root}/bin/turbo-picard" SortVcf \
  "I=${gathered_vcf}" \
  "O=${sorted_vcf}" \
  "SD=${dictionary}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${sorted_vcf}"
awk '!/^#/ { print $2 }' "${sorted_vcf}" | tr '\n' ' ' | grep -q '^2 7 $'

merged_vcf="${tempdir}/merged.vcf"
"${install_root}/bin/turbo-picard" MergeVcfs \
  "I=${shard2_vcf}" \
  "I=${updated_vcf}" \
  "O=${merged_vcf}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${merged_vcf}"
awk '!/^#/ { print $2 }' "${merged_vcf}" | tr '\n' ' ' | grep -q '^2 7 $'

cargo install \
  --locked \
  --no-track \
  --root "${shim_install_root}" \
  --path "${repo_root}/crates/turbo-picard-cli" \
  --bin picard

"${shim_install_root}/bin/picard" --version
"${shim_install_root}/bin/picard" MarkDuplicates --help
"${shim_install_root}/bin/picard" --help > "${tempdir}/picard-shim-help.txt"
python3 - "${repo_root}/docs/command-matrix.yml" "${tempdir}/picard-shim-help.txt" <<'PY'
import re
import sys
from pathlib import Path

matrix = Path(sys.argv[1]).read_text(encoding="utf-8")
help_text = Path(sys.argv[2]).read_text(encoding="utf-8")
commands = []
current = None
for line in matrix.splitlines():
    name = re.match(r"\s*-\s+name:\s+(\S+)", line)
    if name:
        current = name.group(1)
        continue
    status = re.match(r"\s+status:\s+(native|partial-native)\s*$", line)
    if status and current:
        commands.append(current)
        current = None
missing = [command for command in commands if command not in help_text]
if missing:
    raise SystemExit("installed picard shim help missing commands: " + ", ".join(missing))
PY

shim_input="${tempdir}/shim-input.sam"
shim_marked="${tempdir}/shim-marked.sam"
shim_metrics="${tempdir}/shim-metrics.txt"
shim_view="${tempdir}/shim-view.sam"
cat > "${shim_input}" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-a	0	chr1	1	60	8M	*	0	0	ACGTACGT	FFFFFFFF
read-b	0	chr1	1	60	8M	*	0	0	ACGTACGT	FFFFFFFF
SAM

PATH="${shim_install_root}/bin:/usr/bin:/bin" \
  picard MarkDuplicates \
    "I=${shim_input}" \
    "O=${shim_marked}" \
    "M=${shim_metrics}" \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true

test -s "${shim_marked}"
test -s "${shim_metrics}"
grep -q 'UNPAIRED_READ_DUPLICATES' "${shim_metrics}"

PATH="${shim_install_root}/bin:/usr/bin:/bin" \
  picard ViewSam \
    "I=${shim_marked}" \
    "O=${shim_view}" \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true

test -s "${shim_view}"
grep -q '^read-a' "${shim_view}"

shim_recipe_smoke_dir="${tempdir}/shim-recipe-smoke"
mkdir -p "${shim_recipe_smoke_dir}"
(
  cd "${shim_recipe_smoke_dir}"
  PATH="${shim_install_root}/bin:/usr/bin:/bin" \
    bash "${repo_root}/packaging/bioconda/turbo-picard-picard-shim/run_test.sh"
)
