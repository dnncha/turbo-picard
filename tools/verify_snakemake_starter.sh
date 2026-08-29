#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/.." && pwd)"
snakemake_bin="${SNAKEMAKE_BIN:-snakemake}"
turbo_bin="${TURBO_PICARD_BIN:-turbo-picard}"

command -v "${snakemake_bin}" >/dev/null
command -v "${turbo_bin}" >/dev/null

turbo_path="$(command -v "${turbo_bin}")"
turbo_dir="$(dirname -- "${turbo_path}")"
fixture="${repo_root}/fixtures/markduplicates/basic/input.bam"
test -s "${fixture}"

workdir="$(mktemp -d "${TMPDIR:-/tmp}/turbo-picard-snakemake-starter.XXXXXX")"
trap 'rm -rf "${workdir}"' EXIT

mkdir -p "${workdir}/results"
ln -s "${fixture}" "${workdir}/results/basic.bam"

PATH="${turbo_dir}:${PATH}" "${snakemake_bin}" \
  --directory "${workdir}" \
  --snakefile "${repo_root}/packaging/workflows/Snakefile" \
  --cores 1 \
  --printshellcmds \
  --rerun-incomplete \
  results/basic.marked.bam \
  results/basic.metrics.txt

output_bam="${workdir}/results/basic.marked.bam"
metrics="${workdir}/results/basic.metrics.txt"
test -s "${output_bam}"
test -s "${metrics}"
grep -q '^## METRICS CLASS' "${metrics}"
grep -q 'picard.sam.DuplicationMetrics' "${metrics}"

PATH="${turbo_dir}:${PATH}" "${turbo_bin}" \
  ViewSam \
  "I=${output_bam}" \
  "O=${workdir}/results/basic.marked.sam" \
  HEADER_ONLY=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true
test -s "${workdir}/results/basic.marked.sam"

echo "turbo-picard Snakemake starter smoke passed"
