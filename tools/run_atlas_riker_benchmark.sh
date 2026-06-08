#!/usr/bin/env bash
# Sync turbo-picard to atlas over Tailscale and run riker-compatible WGS benchmarks.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ATLAS_HOST="${TURBO_PICARD_ATLAS_HOST:-root@100.69.16.54}"
ATLAS_IDENTITY="${TURBO_PICARD_ATLAS_IDENTITY:-${HOME}/.ssh/tankful_codex}"
REMOTE_DIR="${TURBO_PICARD_ATLAS_REMOTE_DIR:-/root/turbo-picard}"
SSH_OPTS=(-i "${ATLAS_IDENTITY}" -o IdentitiesOnly=yes -o BatchMode=yes)

rsync -az --delete \
  --exclude '.git/' \
  --exclude 'target/' \
  --exclude '.conda-turbo-picard/' \
  --exclude 'benchmarks/real-data/' \
  --exclude 'fixtures/markduplicates/' \
  -e "ssh ${SSH_OPTS[*]}" \
  "${ROOT}/" "${ATLAS_HOST}:${REMOTE_DIR}/"

ssh "${SSH_OPTS[@]}" "${ATLAS_HOST}" \
  "TURBO_PICARD_BENCH_SKIP_STAGE=\${TURBO_PICARD_BENCH_SKIP_STAGE:-1} \
   TURBO_PICARD_ATLAS_MIN_FREE_GB=\${TURBO_PICARD_ATLAS_MIN_FREE_GB:-8} \
   bash ${REMOTE_DIR}/benchmarks/riker-comparison/atlas/setup_and_run.sh"

mkdir -p "${ROOT}/benchmarks/riker-comparison/evidence"
rsync -az \
  -e "ssh ${SSH_OPTS[*]}" \
  "${ATLAS_HOST}:${REMOTE_DIR}/benchmarks/riker-comparison/evidence/" \
  "${ROOT}/benchmarks/riker-comparison/evidence/"

echo "local evidence synced to ${ROOT}/benchmarks/riker-comparison/evidence/"