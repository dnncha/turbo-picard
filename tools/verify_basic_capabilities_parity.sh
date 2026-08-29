#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

output="$(cargo run -q -p turbo-picard-cli --bin turbo-picard -- capabilities --json)"

CAPABILITIES_JSON="$output" python3 - <<'PY'
import json
import os

report = json.loads(os.environ["CAPABILITIES_JSON"])
assert report["schema_version"] == 1
assert report["tool"] == "turbo-picard"
assert report["picard_reference_version"] == "3.4.0"
assert report["install_command"] == "python3 -m pip install turbo-picard"
assert report["benchmark_evidence"]["parity"] == "32/32 PASS"
assert report["benchmark_evidence"]["summary"]["geometric_mean_speedup"] == 24.94

commands = {entry["name"]: entry for entry in report["commands"]}
assert commands["MarkDuplicates"]["status"] == "partial-native"
assert commands["MarkDuplicates"]["trial_fit"] == "recommended-first-trial"
assert commands["EstimateLibraryComplexity"]["status"] == "fallback-only"
assert commands["capabilities"]["status"] == "native"
PY

echo "capabilities contract check passed"
