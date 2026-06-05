#!/usr/bin/env bash
# Picard 3.4.x reads CRAM up to version 3.0. Recent samtools defaults to 3.1.
CRAM_ENCODE_OPTS=(--output-fmt-option version=3.0)

cram_encode_bam() {
  local reference=$1
  local output=$2
  local input=$3
  samtools view -T "$reference" -C -o "$output" "${CRAM_ENCODE_OPTS[@]}" "$input"
}

# Picard ViewSam writes to stdout only; use samtools for CRAM/BAM -> SAM conversion.
view_alignment_to_sam() {
  local reference=$1
  local input=$2
  local output=$3
  if [[ "$input" == *.cram ]]; then
    samtools view -h -T "$reference" -o "$output" "$input"
  else
    samtools view -h -o "$output" "$input"
  fi
}

# Shared Picard runner for parity scripts (mamba/micromamba or prefix on PATH).
parity_conda_setup() {
  local repo_root=$1
  local workdir=$2
  PARITY_WORKDIR=$workdir
  PARITY_CONDA_PREFIX="${TURBO_PICARD_CONDA_PREFIX:-$repo_root/.conda-turbo-picard}"
  export PATH="$workdir:${PARITY_CONDA_PREFIX}/bin:${PATH:-/usr/bin:/bin}"
  if command -v mamba >/dev/null 2>&1; then
    PARITY_CONDA_RUNNER=(mamba)
  elif command -v micromamba >/dev/null 2>&1; then
    PARITY_CONDA_RUNNER=(micromamba)
  elif [[ -x "${PARITY_CONDA_PREFIX}/bin/picard" ]]; then
    PARITY_CONDA_RUNNER=()
  else
    echo "mamba, micromamba, or ${PARITY_CONDA_PREFIX}/bin/picard is required" >&2
    return 127
  fi
}

parity_picard() {
  if ((${#PARITY_CONDA_RUNNER[@]} > 0)); then
    "${PARITY_CONDA_RUNNER[@]}" run -p "$PARITY_CONDA_PREFIX" env \
      "PATH=${PARITY_WORKDIR}:${PARITY_CONDA_PREFIX}/bin:${PATH:-/usr/bin:/bin}" \
      picard "$@" VALIDATION_STRINGENCY=SILENT QUIET=true
  else
    picard "$@" VALIDATION_STRINGENCY=SILENT QUIET=true
  fi
}

parity_view_to_sam() {
  local reference=$1
  local input=$2
  local output=$3
  local repo_root=$4
  if ((${#PARITY_CONDA_RUNNER[@]} > 0)); then
    "${PARITY_CONDA_RUNNER[@]}" run -p "$PARITY_CONDA_PREFIX" bash -c \
      'source "$1"; view_alignment_to_sam "$2" "$3" "$4"' bash \
      "$repo_root/tools/cram_compat.sh" "$reference" "$input" "$output"
  else
    view_alignment_to_sam "$reference" "$input" "$output"
  fi
}