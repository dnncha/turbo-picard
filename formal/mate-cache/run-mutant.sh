#!/usr/bin/env bash
# Self-test: the model must reject a build with the was_spilled guard deleted.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
set +e
output=$("$here/run-tlc.sh" "$here/mutants/Broken.cfg" "$here/mutants/MateCacheBroken.tla" 2>&1)
status=$?
set -e

if [[ $status -eq 0 ]]; then
    echo "error: the mutant passed, so the harness cannot fail" >&2
    exit 1
fi
if ! grep -q "Invariant PairsCanonical is violated" <<<"$output"; then
    echo "error: the mutant failed for an unexpected reason" >&2
    printf '%s\n' "$output" >&2
    exit 1
fi
echo "mutant rejected as expected: Invariant PairsCanonical is violated"
