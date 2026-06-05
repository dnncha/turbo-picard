#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=tools/cram_compat.sh
source "$repo_root/tools/cram_compat.sh"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT
parity_conda_setup "$repo_root" "$workdir"

dataset_root="$repo_root/benchmarks/real-data/gatk-na12878-mito-cram"
input_bam="$repo_root/benchmarks/real-data/gatk-na12878-mito/input.bam"
reference="$repo_root/fixtures/reference/chrM.fa"
input_cram="$dataset_root/input.cram"
evidence_dir="$dataset_root/evidence"

if [[ ! -f "$reference" ]]; then
  echo "missing mitochondrial reference: $reference" >&2
  exit 1
fi

if [[ ! -f "$input_bam" ]]; then
  echo "missing GATK mitochondrial fixture: $input_bam" >&2
  exit 1
fi

mkdir -p "$dataset_root"
if [[ ! -f "$input_cram" ]]; then
  cram_encode_bam "$reference" "$input_cram" "$input_bam"
fi

cargo build --release -p turbo-picard-cli --bin picard

picard_prefix="${PARITY_CONDA_PREFIX}/bin/picard"
if ((${#PARITY_CONDA_RUNNER[@]} > 0)); then
  picard_command="${PARITY_CONDA_RUNNER[*]} run -p ${PARITY_CONDA_PREFIX} picard"
elif [[ -x "$picard_prefix" ]]; then
  picard_command="$picard_prefix"
else
  echo "mamba, micromamba, or ${PARITY_CONDA_PREFIX}/bin/picard is required to bootstrap CRAM evidence" >&2
  exit 127
fi

python3 "$repo_root/tools/compare_real_data.py" \
  --input-bam "$input_cram" \
  --reference-fasta "$reference" \
  --input-source-url "https://github.com/broadinstitute/gatk/blob/e8c49f600b06c658e0fa9bf67256340ebb46bc48/src/test/resources/org/broadinstitute/hellbender/tools/mutect/mito/NA12878.bam" \
  --input-source-commit "e8c49f600b06c658e0fa9bf67256340ebb46bc48" \
  --output-dir "$evidence_dir" \
  --dataset-id "gatk-na12878-mito-cram" \
  --scope-caveat "GATK public NA12878 mitochondrial test BAM converted to CRAM with assembly38 mt-only reference." \
  --release-tier release_candidate \
  --commands ViewSam CleanSam CollectQualityYieldMetrics CollectAlignmentSummaryMetrics CollectInsertSizeMetrics MeanQualityByCycle QualityScoreDistribution CollectBaseDistributionByCycle CollectGcBiasMetrics CollectWgsMetrics CollectMultipleMetrics MarkDuplicates SortSam AddOrReplaceReadGroups ValidateSamFile \
  --picard-command "$picard_command" \
  --turbo-picard-command "$repo_root/target/release/picard" \
  --skip-build

python3 "$repo_root/tools/update_real_data_manifest.py" \
  --entry "$evidence_dir/manifest-entry.json"

echo "CRAM evidence bundle written under $dataset_root"