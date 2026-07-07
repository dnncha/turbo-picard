#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fallback="$(mktemp "${TMPDIR:-/tmp}/turbo-picard-trial-fallback.XXXXXX")"
cat >"$fallback" <<'SCRIPT'
#!/usr/bin/env bash
exit 0
SCRIPT
chmod +x "$fallback"
trap 'rm -f "$fallback"' EXIT

native_output="$(TURBO_PICARD_FALLBACK_COMMAND="$fallback" cargo run -q -p turbo-picard-cli --bin turbo-picard -- trial MarkDuplicates I=input.bam O=marked.bam M=metrics.txt)"
printf '%s\n' "$native_output" | grep -q '^command=MarkDuplicates$'
printf '%s\n' "$native_output" | grep -q '^status=partial-native$'
printf '%s\n' "$native_output" | grep -q '^trial_fit=recommended-first-trial$'
printf '%s\n' "$native_output" | grep -q '^picard_command=picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt$'
printf '%s\n' "$native_output" | grep -q '^turbo_command=turbo-picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt$'
printf '%s\n' "$native_output" | grep -q '^compare='
printf '%s\n' "$native_output" | grep -q '^evidence='
printf '%s\n' "$native_output" | grep -q "^fallback_command=$fallback$"
printf '%s\n' "$native_output" | grep -q '^declared_outputs=O=marked.bam,M=metrics.txt$'

fallback_output="$(cargo run -q -p turbo-picard-cli --bin turbo-picard -- trial EstimateLibraryComplexity O=metrics.txt)"
printf '%s\n' "$fallback_output" | grep -q '^command=EstimateLibraryComplexity$'
printf '%s\n' "$fallback_output" | grep -q '^status=fallback-only$'
printf '%s\n' "$fallback_output" | grep -q '^trial_fit=fallback-only$'
printf '%s\n' "$fallback_output" | grep -q '^declared_outputs=O=metrics.txt$'

echo "trial parity check passed"
