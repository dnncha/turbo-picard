#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fallback="$(mktemp "${TMPDIR:-/tmp}/turbo-picard-doctor-fallback.XXXXXX")"
cat >"$fallback" <<'SCRIPT'
#!/usr/bin/env bash
exit 0
SCRIPT
chmod +x "$fallback"
trap 'rm -f "$fallback"' EXIT

output="$(TURBO_PICARD_THREADS=2 TURBO_PICARD_ACCELERATOR=cpu TURBO_PICARD_FALLBACK_COMMAND="$fallback" cargo run -q -p turbo-picard-cli --bin turbo-picard -- doctor)"

printf '%s\n' "$output" | grep -q '^turbo_picard_version='
printf '%s\n' "$output" | grep -q '^program_name=turbo-picard$'
printf '%s\n' "$output" | grep -q '^picard_reference_version=3.4.0$'
printf '%s\n' "$output" | grep -q '^backend=cpu$'
printf '%s\n' "$output" | grep -q '^policy=cpu$'
printf '%s\n' "$output" | grep -q '^htslib_worker_threads=2$'
printf '%s\n' "$output" | grep -q "^fallback_command=$fallback$"
printf '%s\n' "$output" | grep -q '^auto_fallback=enabled$'

echo "doctor parity check passed"
