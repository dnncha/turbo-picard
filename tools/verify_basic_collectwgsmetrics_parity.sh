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

cat > "$workdir/reference.fa" <<'FASTA'
>chr1
ACGTACGTACGT
FASTA

cat > "$workdir/input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:12
read-a	0	chr1	1	60	4M	*	0	0	ACGT	FFFF
read-b	0	chr1	3	20	4M	*	0	0	GTAC	FFFF
read-c	0	chr1	9	60	4M	*	0	0	ACGT	FFFF
SAM

cat > "$workdir/targets.interval_list" <<'INTERVALS'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:12
chr1	3	6	+	target
INTERVALS

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectWgsMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo.txt" \
  "R=$workdir/reference.fa" \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=1 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectWgsMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard.txt" \
  "R=$workdir/reference.fa" \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=1 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.txt" "$workdir/picard.txt" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]

def tables(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("GENOME_TERRITORY\t"):
            header = line.split("\t")
            row = lines[index + 1].split("\t")
            metrics = dict(zip(header, row))
        if line.startswith("coverage\thigh_quality_coverage_count"):
            for raw in lines[index + 1:]:
                if raw:
                    histogram.append(raw)
            break
    if metrics is None:
        raise SystemExit(f"no WgsMetrics table in {path}")
    return metrics, histogram

turbo, turbo_histogram = tables(turbo_path)
picard, picard_histogram = tables(picard_path)

excluded = {"HET_SNP_SENSITIVITY", "HET_SNP_Q"}
for key in picard:
    if key in excluded:
        continue
    if turbo.get(key) != picard[key]:
        raise SystemExit(f"CollectWgsMetrics {key} differs: turbo={turbo.get(key)} picard={picard[key]}")

if turbo_histogram != picard_histogram:
    raise SystemExit(
        "CollectWgsMetrics coverage histogram differs:\n"
        f"turbo={turbo_histogram[:8]}\n"
        f"picard={picard_histogram[:8]}"
    )

print("CollectWgsMetrics stable coverage metrics match Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectWgsMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-temp-options.txt" \
  "R=$workdir/reference.fa" \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=1 \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectWgsMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-temp-options.txt" \
  "R=$workdir/reference.fa" \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=1 \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-temp-options.txt" "$workdir/picard-temp-options.txt" <<'PY'
import sys

def tables(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("GENOME_TERRITORY\t"):
            header = line.split("\t")
            row = lines[index + 1].split("\t")
            metrics = dict(zip(header, row))
        if line.startswith("coverage\thigh_quality_coverage_count"):
            for raw in lines[index + 1:]:
                if raw:
                    histogram.append(raw)
            break
    if metrics is None:
        raise SystemExit(f"no WgsMetrics table in {path}")
    return metrics, histogram

turbo, turbo_histogram = tables(sys.argv[1])
picard, picard_histogram = tables(sys.argv[2])
excluded = {"HET_SNP_SENSITIVITY", "HET_SNP_Q"}
for key in picard:
    if key in excluded:
        continue
    if turbo.get(key) != picard[key]:
        raise SystemExit(f"CollectWgsMetrics temp options {key} differs: turbo={turbo.get(key)} picard={picard[key]}")
if turbo_histogram != picard_histogram:
    raise SystemExit(
        "CollectWgsMetrics temp options coverage histogram differs:\n"
        f"turbo={turbo_histogram[:8]}\n"
        f"picard={picard_histogram[:8]}"
    )
print("CollectWgsMetrics temp-option metrics match Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectWgsMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-aliases.txt" \
  "R=$workdir/reference.fa" \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=1 \
  MQ=60 \
  Q=30 \
  CAP=2 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectWgsMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-aliases.txt" \
  "R=$workdir/reference.fa" \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=1 \
  MQ=60 \
  Q=30 \
  CAP=2 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-aliases.txt" "$workdir/picard-aliases.txt" <<'PY'
import sys

def tables(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("GENOME_TERRITORY\t"):
            header = line.split("\t")
            row = lines[index + 1].split("\t")
            metrics = dict(zip(header, row))
        if line.startswith("coverage\thigh_quality_coverage_count"):
            for raw in lines[index + 1:]:
                if raw:
                    histogram.append(raw)
            break
    if metrics is None:
        raise SystemExit(f"no WgsMetrics table in {path}")
    return metrics, histogram

turbo, turbo_histogram = tables(sys.argv[1])
picard, picard_histogram = tables(sys.argv[2])
excluded = {"HET_SNP_SENSITIVITY", "HET_SNP_Q"}
for key in picard:
    if key in excluded:
        continue
    if turbo.get(key) != picard[key]:
        raise SystemExit(f"CollectWgsMetrics aliases {key} differs: turbo={turbo.get(key)} picard={picard[key]}")
if turbo_histogram != picard_histogram:
    raise SystemExit(
        "CollectWgsMetrics aliases coverage histogram differs:\n"
        f"turbo={turbo_histogram[:8]}\n"
        f"picard={picard_histogram[:8]}"
    )
print("CollectWgsMetrics MQ/Q/CAP alias metrics match Picard")
PY

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectWgsMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-intervals.txt" \
  "R=$workdir/reference.fa" \
  "INTERVALS=$workdir/targets.interval_list" \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=1 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectWgsMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-intervals.txt" \
  "R=$workdir/reference.fa" \
  "INTERVALS=$workdir/targets.interval_list" \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=1 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-intervals.txt" "$workdir/picard-intervals.txt" <<'PY'
import sys

def tables(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("GENOME_TERRITORY\t"):
            header = line.split("\t")
            row = lines[index + 1].split("\t")
            metrics = dict(zip(header, row))
        if line.startswith("coverage\thigh_quality_coverage_count"):
            for raw in lines[index + 1:]:
                if raw:
                    histogram.append(raw)
            break
    if metrics is None:
        raise SystemExit(f"no WgsMetrics table in {path}")
    return metrics, histogram

turbo, turbo_histogram = tables(sys.argv[1])
picard, picard_histogram = tables(sys.argv[2])
excluded = {"HET_SNP_SENSITIVITY", "HET_SNP_Q"}
for key in picard:
    if key in excluded:
        continue
    if turbo.get(key) != picard[key]:
        raise SystemExit(f"CollectWgsMetrics INTERVALS {key} differs: turbo={turbo.get(key)} picard={picard[key]}")
if turbo_histogram != picard_histogram:
    raise SystemExit(
        "CollectWgsMetrics INTERVALS coverage histogram differs:\n"
        f"turbo={turbo_histogram[:8]}\n"
        f"picard={picard_histogram[:8]}"
    )
print("CollectWgsMetrics INTERVALS coverage metrics match Picard")
PY

cat > "$workdir/bq-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:12
read-a	0	chr1	1	60	4M	*	0	0	ACGT	FFFF
read-b	0	chr1	3	60	4M	*	0	0	GTAC	!5F?
read-c	0	chr1	9	60	4M	*	0	0	ACGT	5555
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectWgsMetrics \
  "I=$workdir/bq-input.sam" \
  "O=$workdir/turbo-bq.txt" \
  "R=$workdir/reference.fa" \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=1 \
  INCLUDE_BQ_HISTOGRAM=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectWgsMetrics \
  "I=$workdir/bq-input.sam" \
  "O=$workdir/picard-bq.txt" \
  "R=$workdir/reference.fa" \
  COUNT_UNPAIRED=true \
  SAMPLE_SIZE=1 \
  INCLUDE_BQ_HISTOGRAM=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-bq.txt" "$workdir/picard-bq.txt" <<'PY'
import sys

def tables(path):
    lines = [line.rstrip("\n") for line in open(path, encoding="utf-8")]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("GENOME_TERRITORY\t"):
            header = line.split("\t")
            row = lines[index + 1].split("\t")
            metrics = dict(zip(header, row))
        if line.startswith("coverage\thigh_quality_coverage_count\tunfiltered_baseq_count"):
            for raw in lines[index + 1:]:
                if raw:
                    histogram.append(raw)
            break
    if metrics is None:
        raise SystemExit(f"no WgsMetrics table in {path}")
    return metrics, histogram

turbo, turbo_histogram = tables(sys.argv[1])
picard, picard_histogram = tables(sys.argv[2])
excluded = {"HET_SNP_SENSITIVITY", "HET_SNP_Q"}
for key in picard:
    if key in excluded:
        continue
    if turbo.get(key) != picard[key]:
        raise SystemExit(f"CollectWgsMetrics BQ {key} differs: turbo={turbo.get(key)} picard={picard[key]}")
if turbo_histogram != picard_histogram:
    raise SystemExit(
        "CollectWgsMetrics BQ histogram differs:\n"
        f"turbo={turbo_histogram[:45]}\n"
        f"picard={picard_histogram[:45]}"
    )
print("CollectWgsMetrics BQ histogram metrics match Picard")
PY
