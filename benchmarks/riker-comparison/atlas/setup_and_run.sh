#!/usr/bin/env bash
# Run on atlas (or any Linux host) to stage riker-compatible WGS smoke data and
# execute the three-way QC benchmark.
set -euo pipefail

export LC_ALL=C
export LANG=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WORKDIR="${TURBO_PICARD_ATLAS_WORKDIR:-/root/turbo-picard-bench}"
STAGE="${WORKDIR}/stage"
REFS="${STAGE}/refs"
SAMPLE_ID="${TURBO_PICARD_BENCH_SAMPLE:-HG02675_4x}"
SAMPLE_DIR="${STAGE}/${SAMPLE_ID}"
BAM="${SAMPLE_DIR}/input.bam"
REF="${REFS}/grch38_decoyhla.fa"
OUTPUT_DIR="${ROOT}/benchmarks/riker-comparison/evidence/${SAMPLE_ID}-atlas"
MICROMAMBA="${WORKDIR}/micromamba"
CONDA_PREFIX="${WORKDIR}/conda-env"
CRAM_URL="${TURBO_PICARD_BENCH_CRAM_URL:-s3://1000genomes/1000G_2504_high_coverage/data/ERR3242389/HG02675.final.cram}"
REF_URL="${TURBO_PICARD_BENCH_REF_URL:-s3://1000genomes/technical/reference/GRCh38_reference_genome/GRCh38_full_analysis_set_plus_decoy_hla.fa}"
SUBSAMPLE_FRAC="${TURBO_PICARD_BENCH_SUBSAMPLE_FRAC:-0.1333}"
SUBSAMPLE_SEED="${TURBO_PICARD_BENCH_SUBSAMPLE_SEED:-3}"
SKIP_STAGE="${TURBO_PICARD_BENCH_SKIP_STAGE:-0}"
SKIP_BUILD="${TURBO_PICARD_BENCH_SKIP_BUILD:-0}"
MIN_FREE_GB="${TURBO_PICARD_ATLAS_MIN_FREE_GB:-8}"

log() {
  printf '[atlas-bench] %s\n' "$*"
}

avail_gb_on_root() {
  df -BG / | awk 'NR==2 {gsub(/G/,"",$4); print $4}'
}

reclaim_safe_disk() {
  log "reclaiming safe caches before benchmark"
  export DEBIAN_FRONTEND=noninteractive
  apt-get clean 2>/dev/null || true
  apt-get autoremove -y -qq 2>/dev/null || true
  journalctl --vacuum-size=50M 2>/dev/null || true
  npm cache clean --force 2>/dev/null || true
  pip3 cache purge 2>/dev/null || true
  rm -rf \
    /root/.cache/ms-playwright \
    /root/.cache/uv \
    /root/.cache/node-gyp \
    /root/.cache/pnpm \
    2>/dev/null || true
  for mamba_bin in \
    "${MICROMAMBA}/bin/micromamba" \
    "$(command -v micromamba || true)" \
    "$(command -v mamba || true)" \
    "$(command -v conda || true)"; do
    [[ -n "${mamba_bin}" && -x "${mamba_bin}" ]] || continue
    "${mamba_bin}" clean --all -y 2>/dev/null || true
  done
  # mamba clean often leaves pkgs/ behind; env prefixes are elsewhere.
  rm -rf \
    /root/.local/share/mamba/pkgs \
    /root/.mamba/pkgs \
    "${HOME}/.conda/pkgs" \
    "${MICROMAMBA}/pkgs" \
    2>/dev/null || true
  docker image prune -f 2>/dev/null || true
  rm -rf "${WORKDIR}/tmp"/* 2>/dev/null || true
  if [[ -d "${ROOT}/target/debug" ]]; then
    rm -rf "${ROOT}/target/debug"
  fi
}

ensure_disk_space() {
  reclaim_safe_disk
  local avail_gb
  avail_gb="$(avail_gb_on_root)"
  log "disk available on / after cleanup: ${avail_gb}GB (need >= ${MIN_FREE_GB}GB)"
  df -h / | tail -1
  if [[ "${avail_gb}" -lt "${MIN_FREE_GB}" ]]; then
    cat >&2 <<EOF
not enough free disk on atlas root filesystem.
need at least ${MIN_FREE_GB}GB free; have ${avail_gb}GB.
free space manually (large caches under /root/.local/share/mamba, /root/.npm,
/root/dotmatch*, old staging dirs) or raise TURBO_PICARD_ATLAS_MIN_FREE_GB.
EOF
    exit 1
  fi
  mkdir -p "${WORKDIR}/tmp"
  export TMPDIR="${WORKDIR}/tmp"
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 127
  fi
}

install_system_packages() {
  if ! command -v apt-get >/dev/null 2>&1; then
    echo "apt-get is required on this host" >&2
    exit 1
  fi
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  apt-get install -y -qq \
    build-essential \
    libc6-dev \
    clang \
    pkg-config \
    libssl-dev \
    zlib1g-dev \
    libbz2-dev \
    liblzma-dev \
    curl \
    ca-certificates \
    samtools \
    time \
    openjdk-21-jre-headless \
    python3-pip \
    >/dev/null
  if ! command -v aws >/dev/null 2>&1; then
    log "installing awscli via pip"
    pip3 install --break-system-packages -q awscli
  fi
}

install_micromamba() {
  if [[ -x "${MICROMAMBA}/bin/micromamba" ]]; then
    return
  fi
  log "installing micromamba"
  mkdir -p "${MICROMAMBA}/bin"
  curl -Ls "https://micro.mamba.pm/api/micromamba/linux-64/latest" \
    | tar -xvj -C "${MICROMAMBA}" bin/micromamba >/dev/null
}

ensure_tool_env() {
  install_micromamba
  if [[ ! -x "${CONDA_PREFIX}/bin/picard" || ! -x "${CONDA_PREFIX}/bin/riker" ]]; then
    log "creating conda env with picard 3.4.0 and riker"
    "${MICROMAMBA}/bin/micromamba" create -y -p "${CONDA_PREFIX}" \
      -c conda-forge -c bioconda \
      "picard=3.4.0" \
      "riker" \
      "samtools>=1.19" \
      "openjdk=21" \
      >/dev/null
  fi
  export PATH="${CONDA_PREFIX}/bin:${PATH}"
}

ensure_rust() {
  if [[ -x "${HOME}/.cargo/bin/rustc" ]]; then
    # shellcheck disable=SC1091
    source "${HOME}/.cargo/env"
  fi
  if rustc --version 2>/dev/null | grep -Eq 'rustc 1\.(8[7-9]|[9][0-9])'; then
    return
  fi
  if [[ ! -x "${HOME}/.cargo/bin/rustup" ]]; then
    log "installing rustup toolchain for turbo-picard build"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain stable --profile minimal
  fi
  # shellcheck disable=SC1091
  source "${HOME}/.cargo/env"
  rustup toolchain install stable --profile minimal >/dev/null
  rustup default stable >/dev/null
}

ensure_turbo_picard() {
  if [[ "${SKIP_BUILD}" == "1" && -x "${ROOT}/target/release/picard" ]]; then
    return
  fi
  ensure_rust
  log "building turbo-picard release binary"
  (
    cd "${ROOT}"
    cargo build --release -p turbo-picard-cli --bin picard 2>&1 | tail -20
  )
}

stage_reference() {
  mkdir -p "${REFS}"
  if [[ -s "${REF}" ]]; then
    log "reusing reference ${REF}"
    return
  fi
  log "fetching reference FASTA"
  unset AWS_PROFILE AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN || true
  aws s3 cp --no-sign-request "${REF_URL}" "${REF}"
  samtools faidx "${REF}"
}

stage_bam() {
  mkdir -p "${SAMPLE_DIR}"
  if [[ -s "${BAM}" ]]; then
    log "reusing BAM ${BAM}"
    if [[ ! -f "${BAM}.bai" ]]; then
      samtools index "${BAM}"
    fi
    return
  fi
  log "streaming ${CRAM_URL} to ${SAMPLE_ID} BAM (subsample=${SUBSAMPLE_FRAC}, seed=${SUBSAMPLE_SEED})"
  unset AWS_PROFILE AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN || true
  aws s3 cp --no-sign-request "${CRAM_URL}" - \
    | samtools view \
      -@1 \
      -b \
      --subsample "${SUBSAMPLE_FRAC}" \
      --subsample-seed "${SUBSAMPLE_SEED}" \
      -T "${REF}" \
      - \
      -o "${BAM}"
  samtools index "${BAM}"
  log "staged BAM size: $(du -h "${BAM}" | awk '{print $1}')"
}

run_benchmark() {
  mkdir -p "${OUTPUT_DIR}"
  export TURBO_PICARD_CONDA_PREFIX="${CONDA_PREFIX}"
  export TURBO_PICARD_BENCH_PICARD_COMMAND="${CONDA_PREFIX}/bin/picard"
  export TURBO_PICARD_BENCH_TURBO_COMMAND="${ROOT}/target/release/picard"
  export TURBO_PICARD_BENCH_RIKER_COMMAND="${CONDA_PREFIX}/bin/riker"
  export TURBO_PICARD_CMM_THREADS="$(python3 - <<'PY'
import os
print(min(4, os.cpu_count() or 1))
PY
)"
  python3 "${ROOT}/tools/bench_qc_vs_riker.py" \
    --sample-id "${SAMPLE_ID}" \
    --input-bam "${BAM}" \
    --reference-fasta "${REF}" \
    --output-dir "${OUTPUT_DIR}" \
    --measure-rss \
    --riker-threads "$(python3 - <<'PY'
import os
print(min(4, os.cpu_count() or 1))
PY
)" \
    --skip-build
}

main() {
  log "host=$(hostname) cpus=$(nproc) workdir=${WORKDIR}"
  ensure_disk_space
  install_system_packages
  ensure_disk_space
  ensure_tool_env
  ensure_turbo_picard
  ensure_disk_space
  if [[ "${SKIP_STAGE}" != "1" ]]; then
    stage_reference
    stage_bam
    ensure_disk_space
  else
    require_cmd samtools
    [[ -s "${BAM}" && -s "${REF}" ]] || {
      echo "SKIP_STAGE=1 but staged BAM/reference are missing" >&2
      exit 1
    }
  fi
  run_benchmark
  log "results in ${OUTPUT_DIR}"
  ls -la "${OUTPUT_DIR}"
}

main "$@"