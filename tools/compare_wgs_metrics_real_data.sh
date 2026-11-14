#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 4 ]]; then
  echo "usage: $0 <input.bam|input.cram> <reference.fa> <output-dir> <picard command...>" >&2
  echo "environment: TURBO_PICARD_BIN (default: turbo-picard), WGS_INTERVALS (optional interval list), WGS_STOP_AFTER (default: 3000000)" >&2
  exit 64
fi

input="$1"
reference="$2"
output_dir="$3"
shift 3
picard_command=("$@")
turbo_bin="${TURBO_PICARD_BIN:-turbo-picard}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compare="$repo_root/tools/parity_compare.py"

if [[ -e "$output_dir" ]]; then
  echo "output directory already exists: $output_dir" >&2
  exit 73
fi
mkdir -p "$output_dir"

cases=(
  "default|"
  "count-unpaired|COUNT_UNPAIRED=true"
  "coverage-cap-8|COVERAGE_CAP=8 COUNT_UNPAIRED=true"
  "no-quality-floor|MINIMUM_BASE_QUALITY=0 MINIMUM_MAPPING_QUALITY=0"
  "accumulation-cap|COVERAGE_CAP=4 LOCUS_ACCUMULATION_CAP=6 COUNT_UNPAIRED=true"
  "stop-after|STOP_AFTER=${WGS_STOP_AFTER:-3000000} COUNT_UNPAIRED=true"
  "stop-after-paired|STOP_AFTER=${WGS_STOP_AFTER:-3000000}"
)
if [[ -n "${WGS_INTERVALS:-}" ]]; then
  cases+=("intervals|INTERVALS=$WGS_INTERVALS COUNT_UNPAIRED=true")
  cases+=("intervals-stop-after|INTERVALS=$WGS_INTERVALS STOP_AFTER=${WGS_INTERVALS_STOP_AFTER:-1000}")
fi

failures=0
for entry in "${cases[@]}"; do
  name="${entry%%|*}"
  read -r -a options <<< "${entry#*|}"
  common=(CollectWgsMetrics "I=$input" "R=$reference" INCLUDE_BQ_HISTOGRAM=true
    VALIDATION_STRINGENCY=SILENT QUIET=true ${options[@]+"${options[@]}"})
  "${picard_command[@]}" "${common[@]}" "O=$output_dir/picard.$name.txt" \
    > "$output_dir/picard.$name.log" 2>&1
  for workers in 1 auto; do
    TURBO_PICARD_REQUIRE_NATIVE=1 TURBO_PICARD_REGION_WORKERS="$workers" \
      "$turbo_bin" "${common[@]}" "O=$output_dir/turbo.$name.workers-$workers.txt" \
      > "$output_dir/turbo.$name.workers-$workers.log" 2>&1
    if python3 "$compare" wgs-metrics --label "$name workers=$workers" \
      --picard "$output_dir/picard.$name.txt" \
      --turbo "$output_dir/turbo.$name.workers-$workers.txt"; then
      echo "MATCH $name workers=$workers"
    else
      echo "DIFFERS $name workers=$workers"
      failures=$((failures + 1))
    fi
  done
done

if [[ "$failures" -ne 0 ]]; then
  echo "$failures CollectWgsMetrics comparison(s) differ from Picard" >&2
  exit 1
fi
echo "CollectWgsMetrics matches Picard for every case"
