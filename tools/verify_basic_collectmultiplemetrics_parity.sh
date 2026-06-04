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

cat > "$workdir/input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
pair1	99	chr1	10	60	4M	=	30	24	ACGT	FFFF
pair1	147	chr1	30	60	4M	=	10	-24	TGCA	FFFF
pair2	99	chr1	100	60	4M	=	130	34	AAAA	FFFF
pair2	147	chr1	130	60	4M	=	100	-34	TTTT	FFFF
SAM

programs=(
  PROGRAM=null
  PROGRAM=CollectInsertSizeMetrics
  PROGRAM=CollectBaseDistributionByCycle
  PROGRAM=QualityScoreDistribution
  PROGRAM=MeanQualityByCycle
  PROGRAM=CollectQualityYieldMetrics
)

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectMultipleMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-runtime" \
  PROGRAM=null \
  PROGRAM=CollectQualityYieldMetrics \
  CREATE_INDEX=true \
  CREATE_MD5_FILE=true \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  COMPRESSION_LEVEL=5 \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectMultipleMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-runtime" \
  PROGRAM=null \
  PROGRAM=CollectQualityYieldMetrics \
  CREATE_INDEX=true \
  CREATE_MD5_FILE=true \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  COMPRESSION_LEVEL=5 \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir" <<'PY'
import pathlib
import sys

workdir = pathlib.Path(sys.argv[1])
expected = [
    "turbo-runtime.quality_yield_metrics",
    "picard-runtime.quality_yield_metrics",
]
missing = [name for name in expected if not (workdir / name).exists()]
if missing:
    raise SystemExit(f"missing CollectMultipleMetrics runtime output: {missing}")
unexpected = [
    "turbo-runtime.quality_yield_metrics.md5",
    "picard-runtime.quality_yield_metrics.md5",
    "turbo-runtime.quality_yield_metrics.idx",
    "picard-runtime.quality_yield_metrics.idx",
]
present = [name for name in unexpected if (workdir / name).exists()]
if present:
    raise SystemExit(f"unexpected CollectMultipleMetrics runtime sidecar: {present}")
print("CollectMultipleMetrics runtime option side effects match Picard")
PY

if ! command -v Rscript >/dev/null 2>&1; then
  echo "Skipping chart-producing CollectMultipleMetrics parity checks because upstream Picard requires Rscript for those child programs" >&2
  exit 0
fi

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectMultipleMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo" \
  "${programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectMultipleMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard" \
  "${programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo" "$workdir/picard" <<'PY'
import sys

turbo_prefix, picard_prefix = sys.argv[1:]

def table_after(path, header):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line == header:
            rows = []
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                rows.append(lines[cursor])
                cursor += 1
            return rows
    raise SystemExit(f"missing table {header!r} in {path}")

checks = [
    (".insert_size_metrics", "MEDIAN_INSERT_SIZE\tMODE_INSERT_SIZE\tMEDIAN_ABSOLUTE_DEVIATION\tMIN_INSERT_SIZE\tMAX_INSERT_SIZE\tMEAN_INSERT_SIZE\tSTANDARD_DEVIATION\tREAD_PAIRS\tPAIR_ORIENTATION\tWIDTH_OF_10_PERCENT\tWIDTH_OF_20_PERCENT\tWIDTH_OF_30_PERCENT\tWIDTH_OF_40_PERCENT\tWIDTH_OF_50_PERCENT\tWIDTH_OF_60_PERCENT\tWIDTH_OF_70_PERCENT\tWIDTH_OF_80_PERCENT\tWIDTH_OF_90_PERCENT\tWIDTH_OF_95_PERCENT\tWIDTH_OF_99_PERCENT\tSAMPLE\tLIBRARY\tREAD_GROUP"),
    (".insert_size_metrics", "insert_size\tAll_Reads.fr_count"),
    (".base_distribution_by_cycle_metrics", "READ_END\tCYCLE\tPCT_A\tPCT_C\tPCT_G\tPCT_T\tPCT_N"),
    (".quality_distribution_metrics", "QUALITY\tCOUNT_OF_Q"),
    (".quality_by_cycle_metrics", "CYCLE\tMEAN_QUALITY"),
    (".quality_yield_metrics", "TOTAL_READS\tPF_READS\tREAD_LENGTH\tTOTAL_BASES\tPF_BASES\tQ20_BASES\tPF_Q20_BASES\tQ30_BASES\tPF_Q30_BASES\tQ20_EQUIVALENT_YIELD\tPF_Q20_EQUIVALENT_YIELD"),
]

for suffix, header in checks:
    turbo = table_after(turbo_prefix + suffix, header)
    picard = table_after(picard_prefix + suffix, header)
    if turbo != picard:
        raise SystemExit(f"CollectMultipleMetrics {suffix} table differs for {header!r}:\nturbo={turbo}\npicard={picard}")

print("CollectMultipleMetrics stable metric tables match Picard")
PY

test -s "$workdir/turbo.insert_size_histogram.pdf"
test -s "$workdir/turbo.base_distribution_by_cycle.pdf"
test -s "$workdir/turbo.quality_distribution.pdf"
test -s "$workdir/turbo.quality_by_cycle.pdf"

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectMultipleMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-ext" \
  PROGRAM=null \
  PROGRAM=CollectQualityYieldMetrics \
  EXT=.txt \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectMultipleMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-ext" \
  PROGRAM=null \
  PROGRAM=CollectQualityYieldMetrics \
  EXT=.txt \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-ext.quality_yield_metrics.txt" "$workdir/picard-ext.quality_yield_metrics.txt" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]

def stable_rows(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    header = "TOTAL_READS\tPF_READS\tREAD_LENGTH\tTOTAL_BASES\tPF_BASES\tQ20_BASES\tPF_Q20_BASES\tQ30_BASES\tPF_Q30_BASES\tQ20_EQUIVALENT_YIELD\tPF_Q20_EQUIVALENT_YIELD"
    index = lines.index(header)
    rows = []
    cursor = index
    while cursor < len(lines) and lines[cursor]:
        rows.append(lines[cursor])
        cursor += 1
    return rows

if stable_rows(turbo_path) != stable_rows(picard_path):
    raise SystemExit("CollectMultipleMetrics FILE_EXTENSION quality yield output differs from Picard")
print("CollectMultipleMetrics FILE_EXTENSION output matches Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectMultipleMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-default" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectMultipleMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-default" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-default" "$workdir/picard-default" <<'PY'
import sys

turbo_prefix, picard_prefix = sys.argv[1:]

checks = [
    (".alignment_summary_metrics", "CATEGORY\tTOTAL_READS\tPF_READS\tPCT_PF_READS\tPF_NOISE_READS\tPF_READS_ALIGNED\tPCT_PF_READS_ALIGNED\tPF_ALIGNED_BASES\tPF_HQ_ALIGNED_READS\tPF_HQ_ALIGNED_BASES\tPF_HQ_ALIGNED_Q20_BASES\tPF_HQ_MEDIAN_MISMATCHES\tPF_MISMATCH_RATE\tPF_HQ_ERROR_RATE\tPF_INDEL_RATE\tMEAN_READ_LENGTH\tSD_READ_LENGTH\tMEDIAN_READ_LENGTH\tMAD_READ_LENGTH\tMIN_READ_LENGTH\tMAX_READ_LENGTH\tMEAN_ALIGNED_READ_LENGTH\tREADS_ALIGNED_IN_PAIRS\tPCT_READS_ALIGNED_IN_PAIRS\tPF_READS_IMPROPER_PAIRS\tPCT_PF_READS_IMPROPER_PAIRS\tBAD_CYCLES\tSTRAND_BALANCE\tPCT_CHIMERAS\tPCT_ADAPTER\tPCT_SOFTCLIP\tPCT_HARDCLIP\tAVG_POS_3PRIME_SOFTCLIP_LENGTH\tSAMPLE\tLIBRARY\tREAD_GROUP"),
    (".base_distribution_by_cycle_metrics", "READ_END\tCYCLE\tPCT_A\tPCT_C\tPCT_G\tPCT_T\tPCT_N"),
    (".insert_size_metrics", "MEDIAN_INSERT_SIZE\tMODE_INSERT_SIZE\tMEDIAN_ABSOLUTE_DEVIATION\tMIN_INSERT_SIZE\tMAX_INSERT_SIZE\tMEAN_INSERT_SIZE\tSTANDARD_DEVIATION\tREAD_PAIRS\tPAIR_ORIENTATION\tWIDTH_OF_10_PERCENT\tWIDTH_OF_20_PERCENT\tWIDTH_OF_30_PERCENT\tWIDTH_OF_40_PERCENT\tWIDTH_OF_50_PERCENT\tWIDTH_OF_60_PERCENT\tWIDTH_OF_70_PERCENT\tWIDTH_OF_80_PERCENT\tWIDTH_OF_90_PERCENT\tWIDTH_OF_95_PERCENT\tWIDTH_OF_99_PERCENT\tSAMPLE\tLIBRARY\tREAD_GROUP"),
    (".quality_by_cycle_metrics", "CYCLE\tMEAN_QUALITY"),
    (".quality_distribution_metrics", "QUALITY\tCOUNT_OF_Q"),
]

def table_after(path, header):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line == header:
            rows = []
            cursor = index + 1
            while cursor < len(lines) and lines[cursor]:
                rows.append(lines[cursor])
                cursor += 1
            return rows
    raise SystemExit(f"missing table {header!r} in {path}")

for suffix, header in checks:
    turbo = table_after(turbo_prefix + suffix, header)
    picard = table_after(picard_prefix + suffix, header)
    if turbo != picard:
        raise SystemExit(f"CollectMultipleMetrics default {suffix} differs:\nturbo={turbo}\npicard={picard}")

print("CollectMultipleMetrics default program tables match Picard")
PY

cat > "$workdir/alignment-read-group-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
@RG	ID:rg1	SM:sampleA	LB:lib1	PL:ILLUMINA	PU:unit1
read-a	0	chr1	10	60	4M	*	0	0	ACGT	FFFF	RG:Z:rg1
read-b	4	*	0	0	*	*	0	0	NNNN	!!!!	RG:Z:rg1
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectMultipleMetrics \
  "I=$workdir/alignment-read-group-input.sam" \
  "O=$workdir/turbo-alignment-read-group" \
  PROGRAM=null \
  PROGRAM=CollectAlignmentSummaryMetrics \
  METRIC_ACCUMULATION_LEVEL=READ_GROUP \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectMultipleMetrics \
  "I=$workdir/alignment-read-group-input.sam" \
  "O=$workdir/picard-alignment-read-group" \
  PROGRAM=null \
  PROGRAM=CollectAlignmentSummaryMetrics \
  METRIC_ACCUMULATION_LEVEL=READ_GROUP \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-alignment-read-group.alignment_summary_metrics" "$workdir/picard-alignment-read-group.alignment_summary_metrics" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]
header = "CATEGORY\tTOTAL_READS\tPF_READS\tPCT_PF_READS\tPF_NOISE_READS\tPF_READS_ALIGNED\tPCT_PF_READS_ALIGNED\tPF_ALIGNED_BASES\tPF_HQ_ALIGNED_READS\tPF_HQ_ALIGNED_BASES\tPF_HQ_ALIGNED_Q20_BASES\tPF_HQ_MEDIAN_MISMATCHES\tPF_MISMATCH_RATE\tPF_HQ_ERROR_RATE\tPF_INDEL_RATE\tMEAN_READ_LENGTH\tSD_READ_LENGTH\tMEDIAN_READ_LENGTH\tMAD_READ_LENGTH\tMIN_READ_LENGTH\tMAX_READ_LENGTH\tMEAN_ALIGNED_READ_LENGTH\tREADS_ALIGNED_IN_PAIRS\tPCT_READS_ALIGNED_IN_PAIRS\tPF_READS_IMPROPER_PAIRS\tPCT_PF_READS_IMPROPER_PAIRS\tBAD_CYCLES\tSTRAND_BALANCE\tPCT_CHIMERAS\tPCT_ADAPTER\tPCT_SOFTCLIP\tPCT_HARDCLIP\tAVG_POS_3PRIME_SOFTCLIP_LENGTH\tSAMPLE\tLIBRARY\tREAD_GROUP"

def table(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    index = lines.index(header)
    rows = []
    cursor = index
    while cursor < len(lines) and lines[cursor]:
        rows.append(lines[cursor])
        cursor += 1
    return rows

if table(turbo_path) != table(picard_path):
    raise SystemExit("CollectMultipleMetrics alignment READ_GROUP output differs from Picard")
print("CollectMultipleMetrics alignment READ_GROUP output matches Picard")
PY

cat > "$workdir/gc-ref.fa" <<'FA'
>low
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
>high
CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC
FA

cat > "$workdir/gc-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:low	LN:40
@SQ	SN:high	LN:40
low1	0	low	1	60	20M	*	0	0	AAAAAAAAAAAAAAAAAAAA	FFFFFFFFFFFFFFFFFFFF
high1	0	high	1	60	20M	*	0	0	CCCCCCCCCCCCCCCCCCCC	FFFFFFFFFFFFFFFFFFFF
SAM

gc_programs=(
  PROGRAM=null
  PROGRAM=CollectGcBiasMetrics
  EXTRA_ARGUMENT=CollectGcBiasMetrics::SCAN_WINDOW_SIZE=20
  EXTRA_ARGUMENT=CollectGcBiasMetrics::MINIMUM_GENOME_FRACTION=0
)

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectMultipleMetrics \
  "I=$workdir/gc-input.sam" \
  "O=$workdir/turbo-gc" \
  "R=$workdir/gc-ref.fa" \
  "${gc_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectMultipleMetrics \
  "I=$workdir/gc-input.sam" \
  "O=$workdir/picard-gc" \
  "R=$workdir/gc-ref.fa" \
  "${gc_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-gc" "$workdir/picard-gc" <<'PY'
import sys

turbo_prefix, picard_prefix = sys.argv[1:]

checks = [
    (".gc_bias.detail_metrics", "ACCUMULATION_LEVEL\tREADS_USED\tGC\tWINDOWS\tREAD_STARTS\tMEAN_BASE_QUALITY\tNORMALIZED_COVERAGE\tERROR_BAR_WIDTH\tSAMPLE\tLIBRARY\tREAD_GROUP"),
    (".gc_bias.summary_metrics", "ACCUMULATION_LEVEL\tREADS_USED\tWINDOW_SIZE\tTOTAL_CLUSTERS\tALIGNED_READS\tAT_DROPOUT\tGC_DROPOUT\tGC_NC_0_19\tGC_NC_20_39\tGC_NC_40_59\tGC_NC_60_79\tGC_NC_80_100\tSAMPLE\tLIBRARY\tREAD_GROUP"),
]

def table(path, header):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line == header:
            rows = []
            cursor = index
            while cursor < len(lines) and lines[cursor]:
                rows.append(lines[cursor])
                cursor += 1
            return rows
    raise SystemExit(f"missing table {header!r} in {path}")

for suffix, header in checks:
    turbo = table(turbo_prefix + suffix, header)
    picard = table(picard_prefix + suffix, header)
    if turbo != picard:
        raise SystemExit(f"CollectMultipleMetrics explicit GC {suffix} differs:\nturbo={turbo}\npicard={picard}")

print("CollectMultipleMetrics explicit CollectGcBiasMetrics tables match Picard")
PY

test -s "$workdir/turbo-gc.gc_bias.pdf"

cat > "$workdir/gc-duplicates-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:low	LN:40
@SQ	SN:high	LN:40
low1	0	low	1	60	20M	*	0	0	AAAAAAAAAAAAAAAAAAAA	FFFFFFFFFFFFFFFFFFFF
lowdup	1024	low	1	60	20M	*	0	0	AAAAAAAAAAAAAAAAAAAA	!!!!!!!!!!!!!!!!!!!!
high1	0	high	1	60	20M	*	0	0	CCCCCCCCCCCCCCCCCCCC	FFFFFFFFFFFFFFFFFFFF
SAM

gc_duplicate_programs=(
  PROGRAM=null
  PROGRAM=CollectGcBiasMetrics
  EXTRA_ARGUMENT=CollectGcBiasMetrics::SCAN_WINDOW_SIZE=20
  EXTRA_ARGUMENT=CollectGcBiasMetrics::MINIMUM_GENOME_FRACTION=0
  EXTRA_ARGUMENT=CollectGcBiasMetrics::ALSO_IGNORE_DUPLICATES=true
)

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectMultipleMetrics \
  "I=$workdir/gc-duplicates-input.sam" \
  "O=$workdir/turbo-gc-duplicates" \
  "R=$workdir/gc-ref.fa" \
  "${gc_duplicate_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectMultipleMetrics \
  "I=$workdir/gc-duplicates-input.sam" \
  "O=$workdir/picard-gc-duplicates" \
  "R=$workdir/gc-ref.fa" \
  "${gc_duplicate_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-gc-duplicates" "$workdir/picard-gc-duplicates" <<'PY'
import sys

turbo_prefix, picard_prefix = sys.argv[1:]

checks = [
    (".gc_bias.detail_metrics", "ACCUMULATION_LEVEL\tREADS_USED\tGC\tWINDOWS\tREAD_STARTS\tMEAN_BASE_QUALITY\tNORMALIZED_COVERAGE\tERROR_BAR_WIDTH\tSAMPLE\tLIBRARY\tREAD_GROUP"),
    (".gc_bias.summary_metrics", "ACCUMULATION_LEVEL\tREADS_USED\tWINDOW_SIZE\tTOTAL_CLUSTERS\tALIGNED_READS\tAT_DROPOUT\tGC_DROPOUT\tGC_NC_0_19\tGC_NC_20_39\tGC_NC_40_59\tGC_NC_60_79\tGC_NC_80_100\tSAMPLE\tLIBRARY\tREAD_GROUP"),
]

def table(path, header):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line == header:
            rows = []
            cursor = index
            while cursor < len(lines) and lines[cursor]:
                rows.append(lines[cursor])
                cursor += 1
            return rows
    raise SystemExit(f"missing table {header!r} in {path}")

for suffix, header in checks:
    turbo = table(turbo_prefix + suffix, header)
    picard = table(picard_prefix + suffix, header)
    if turbo != picard:
        raise SystemExit(f"CollectMultipleMetrics duplicate GC {suffix} differs:\nturbo={turbo}\npicard={picard}")

print("CollectMultipleMetrics duplicate-filtered CollectGcBiasMetrics tables match Picard")
PY

test -s "$workdir/turbo-gc-duplicates.gc_bias.pdf"

cat > "$workdir/quality-extra-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
mapped	0	chr1	10	60	4M	*	0	0	ACGT	FFFF
unmapped	4	*	0	0	*	*	0	0	TGCA	!!!!
SAM

quality_programs=(
  PROGRAM=null
  PROGRAM=QualityScoreDistribution
  EXTRA_ARGUMENT=QualityScoreDistribution::ALIGNED_READS_ONLY=true
)

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectMultipleMetrics \
  "I=$workdir/quality-extra-input.sam" \
  "O=$workdir/turbo-quality-extra" \
  "${quality_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectMultipleMetrics \
  "I=$workdir/quality-extra-input.sam" \
  "O=$workdir/picard-quality-extra" \
  "${quality_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-quality-extra.quality_distribution_metrics" "$workdir/picard-quality-extra.quality_distribution_metrics" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]
header = "QUALITY\tCOUNT_OF_Q"

def table(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line == header:
            rows = []
            cursor = index
            while cursor < len(lines) and lines[cursor]:
                rows.append(lines[cursor])
                cursor += 1
            return rows
    raise SystemExit(f"missing table {header!r} in {path}")

turbo = table(turbo_path)
picard = table(picard_path)
if turbo != picard:
    raise SystemExit(f"CollectMultipleMetrics QualityScoreDistribution EXTRA_ARGUMENT differs:\nturbo={turbo}\npicard={picard}")
print("CollectMultipleMetrics QualityScoreDistribution EXTRA_ARGUMENT output matches Picard")
PY

mean_quality_programs=(
  PROGRAM=null
  PROGRAM=MeanQualityByCycle
  EXTRA_ARGUMENT=MeanQualityByCycle::ALIGNED_READS_ONLY=true
)

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectMultipleMetrics \
  "I=$workdir/quality-extra-input.sam" \
  "O=$workdir/turbo-mean-quality-extra" \
  "${mean_quality_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectMultipleMetrics \
  "I=$workdir/quality-extra-input.sam" \
  "O=$workdir/picard-mean-quality-extra" \
  "${mean_quality_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-mean-quality-extra.quality_by_cycle_metrics" "$workdir/picard-mean-quality-extra.quality_by_cycle_metrics" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]
header = "CYCLE\tMEAN_QUALITY"

def table(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line == header:
            rows = []
            cursor = index
            while cursor < len(lines) and lines[cursor]:
                rows.append(lines[cursor])
                cursor += 1
            return rows
    raise SystemExit(f"missing table {header!r} in {path}")

turbo = table(turbo_path)
picard = table(picard_path)
if turbo != picard:
    raise SystemExit(f"CollectMultipleMetrics MeanQualityByCycle EXTRA_ARGUMENT differs:\nturbo={turbo}\npicard={picard}")
print("CollectMultipleMetrics MeanQualityByCycle EXTRA_ARGUMENT output matches Picard")
PY

cat > "$workdir/quality-yield-extra-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
primary	0	chr1	1	60	4M	*	0	0	ACGT	FFFF
secondary	256	chr1	2	60	4M	*	0	0	ACGT	FFFF
supplemental	2048	chr1	3	60	4M	*	0	0	ACGT	EEEE
SAM

quality_yield_programs=(
  PROGRAM=null
  PROGRAM=CollectQualityYieldMetrics
  EXTRA_ARGUMENT=CollectQualityYieldMetrics::INCLUDE_SECONDARY_ALIGNMENTS=true
  EXTRA_ARGUMENT=CollectQualityYieldMetrics::INCLUDE_SUPPLEMENTAL_ALIGNMENTS=true
)

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectMultipleMetrics \
  "I=$workdir/quality-yield-extra-input.sam" \
  "O=$workdir/turbo-quality-yield-extra" \
  "${quality_yield_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectMultipleMetrics \
  "I=$workdir/quality-yield-extra-input.sam" \
  "O=$workdir/picard-quality-yield-extra" \
  "${quality_yield_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-quality-yield-extra.quality_yield_metrics" "$workdir/picard-quality-yield-extra.quality_yield_metrics" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]
header = "TOTAL_READS\tPF_READS\tREAD_LENGTH\tTOTAL_BASES\tPF_BASES\tQ20_BASES\tPF_Q20_BASES\tQ30_BASES\tPF_Q30_BASES\tQ20_EQUIVALENT_YIELD\tPF_Q20_EQUIVALENT_YIELD"

def table(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line == header:
            rows = []
            cursor = index
            while cursor < len(lines) and lines[cursor]:
                rows.append(lines[cursor])
                cursor += 1
            return rows
    raise SystemExit(f"missing table {header!r} in {path}")

turbo = table(turbo_path)
picard = table(picard_path)
if turbo != picard:
    raise SystemExit(f"CollectMultipleMetrics CollectQualityYieldMetrics EXTRA_ARGUMENT differs:\nturbo={turbo}\npicard={picard}")
print("CollectMultipleMetrics CollectQualityYieldMetrics EXTRA_ARGUMENT output matches Picard")
PY

cat > "$workdir/insert-extra-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
pair1	99	chr1	10	60	4M	=	30	24	ACGT	FFFF
pair1	147	chr1	30	60	4M	=	10	-24	TGCA	FFFF
pairdup	1123	chr1	100	60	4M	=	130	34	AAAA	FFFF
pairdup	1171	chr1	130	60	4M	=	100	-34	TTTT	FFFF
SAM

insert_programs=(
  PROGRAM=null
  PROGRAM=CollectInsertSizeMetrics
  EXTRA_ARGUMENT=CollectInsertSizeMetrics::INCLUDE_DUPLICATES=true
)

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectMultipleMetrics \
  "I=$workdir/insert-extra-input.sam" \
  "O=$workdir/turbo-insert-extra" \
  "${insert_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectMultipleMetrics \
  "I=$workdir/insert-extra-input.sam" \
  "O=$workdir/picard-insert-extra" \
  "${insert_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-insert-extra.insert_size_metrics" "$workdir/picard-insert-extra.insert_size_metrics" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]
headers = [
    "MEDIAN_INSERT_SIZE\tMODE_INSERT_SIZE\tMEDIAN_ABSOLUTE_DEVIATION\tMIN_INSERT_SIZE\tMAX_INSERT_SIZE\tMEAN_INSERT_SIZE\tSTANDARD_DEVIATION\tREAD_PAIRS\tPAIR_ORIENTATION\tWIDTH_OF_10_PERCENT\tWIDTH_OF_20_PERCENT\tWIDTH_OF_30_PERCENT\tWIDTH_OF_40_PERCENT\tWIDTH_OF_50_PERCENT\tWIDTH_OF_60_PERCENT\tWIDTH_OF_70_PERCENT\tWIDTH_OF_80_PERCENT\tWIDTH_OF_90_PERCENT\tWIDTH_OF_95_PERCENT\tWIDTH_OF_99_PERCENT\tSAMPLE\tLIBRARY\tREAD_GROUP",
    "insert_size\tAll_Reads.fr_count",
]

def table(path, header):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line == header:
            rows = []
            cursor = index
            while cursor < len(lines) and lines[cursor]:
                rows.append(lines[cursor])
                cursor += 1
            return rows
    raise SystemExit(f"missing table {header!r} in {path}")

for header in headers:
    turbo = table(turbo_path, header)
    picard = table(picard_path, header)
    if turbo != picard:
        raise SystemExit(f"CollectMultipleMetrics CollectInsertSizeMetrics EXTRA_ARGUMENT differs for {header!r}:\nturbo={turbo}\npicard={picard}")
print("CollectMultipleMetrics CollectInsertSizeMetrics EXTRA_ARGUMENT output matches Picard")
PY

cat > "$workdir/insert-read-group-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
@RG	ID:rg1	SM:sampleA	LB:lib1	PL:ILLUMINA	PU:unit1
pair1	99	chr1	10	60	4M	=	30	24	ACGT	FFFF	RG:Z:rg1
pair1	147	chr1	30	60	4M	=	10	-24	TGCA	FFFF	RG:Z:rg1
pair2	99	chr1	100	60	4M	=	130	34	AAAA	FFFF	RG:Z:rg1
pair2	147	chr1	130	60	4M	=	100	-34	TTTT	FFFF	RG:Z:rg1
SAM

insert_read_group_programs=(
  PROGRAM=null
  PROGRAM=CollectInsertSizeMetrics
  METRIC_ACCUMULATION_LEVEL=READ_GROUP
)

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectMultipleMetrics \
  "I=$workdir/insert-read-group-input.sam" \
  "O=$workdir/turbo-insert-read-group" \
  "${insert_read_group_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectMultipleMetrics \
  "I=$workdir/insert-read-group-input.sam" \
  "O=$workdir/picard-insert-read-group" \
  "${insert_read_group_programs[@]}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-insert-read-group.insert_size_metrics" "$workdir/picard-insert-read-group.insert_size_metrics" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]
headers = [
    "MEDIAN_INSERT_SIZE\tMODE_INSERT_SIZE\tMEDIAN_ABSOLUTE_DEVIATION\tMIN_INSERT_SIZE\tMAX_INSERT_SIZE\tMEAN_INSERT_SIZE\tSTANDARD_DEVIATION\tREAD_PAIRS\tPAIR_ORIENTATION\tWIDTH_OF_10_PERCENT\tWIDTH_OF_20_PERCENT\tWIDTH_OF_30_PERCENT\tWIDTH_OF_40_PERCENT\tWIDTH_OF_50_PERCENT\tWIDTH_OF_60_PERCENT\tWIDTH_OF_70_PERCENT\tWIDTH_OF_80_PERCENT\tWIDTH_OF_90_PERCENT\tWIDTH_OF_95_PERCENT\tWIDTH_OF_99_PERCENT\tSAMPLE\tLIBRARY\tREAD_GROUP",
    "insert_size\tAll_Reads.fr_count\tunit1.fr_count",
]

def table(path, header):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    for index, line in enumerate(lines):
        if line == header:
            rows = []
            cursor = index
            while cursor < len(lines) and lines[cursor]:
                rows.append(lines[cursor])
                cursor += 1
            return rows
    raise SystemExit(f"missing table {header!r} in {path}")

for header in headers:
    turbo = table(turbo_path, header)
    picard = table(picard_path, header)
    if turbo != picard:
        raise SystemExit(f"CollectMultipleMetrics CollectInsertSizeMetrics READ_GROUP differs for {header!r}:\nturbo={turbo}\npicard={picard}")
print("CollectMultipleMetrics CollectInsertSizeMetrics READ_GROUP output matches Picard")
PY
