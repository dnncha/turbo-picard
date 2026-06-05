#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

output="$(TURBO_PICARD_THREADS=2 TURBO_PICARD_ACCELERATOR=cpu cargo run -q -p turbo-picard-cli --bin turbo-picard -- AccelerationStatus)"

printf '%s\n' "$output" | grep -q '^backend=cpu$'
printf '%s\n' "$output" | grep -q '^policy=cpu$'
printf '%s\n' "$output" | grep -q '^htslib_worker_threads=2$'
printf '%s\n' "$output" | grep -q '^gpu_acceleration=not-enabled$'

if TURBO_PICARD_ACCELERATOR=gpu-required cargo run -q -p turbo-picard-cli --bin turbo-picard -- AccelerationStatus >/tmp/turbo-picard-acceleration-status.out 2>/tmp/turbo-picard-acceleration-status.err; then
  echo "AccelerationStatus gpu-required unexpectedly succeeded without a production GPU backend" >&2
  exit 1
fi

grep -q '^policy=gpu-required$' /tmp/turbo-picard-acceleration-status.out
grep -q 'this build has no production GPU backend' /tmp/turbo-picard-acceleration-status.err

echo "AccelerationStatus parity check passed"
