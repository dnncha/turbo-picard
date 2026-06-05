#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dataset_root="$repo_root/benchmarks/real-data/gatk-na12878-mito-cram"
input_cram="$dataset_root/input.cram"
reference="$repo_root/fixtures/reference/chrM.fa"
evidence_json="$dataset_root/evidence/real-data-comparison.json"
manifest="$repo_root/benchmarks/real-data/manifest.json"

if [[ ! -f "$input_cram" ]]; then
  echo "missing checked-in CRAM fixture: $input_cram" >&2
  exit 1
fi

if [[ ! -f "$reference" ]]; then
  echo "missing mitochondrial reference: $reference" >&2
  exit 1
fi

if [[ ! -f "$evidence_json" ]]; then
  echo "gatk-na12878-mito-cram evidence not checked in yet; run tools/bootstrap_gatk_mito_cram_evidence.sh" >&2
  exit 1
fi

python3 - "$repo_root" "$evidence_json" "$input_cram" "$manifest" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
evidence_json = Path(sys.argv[2])
input_cram = Path(sys.argv[3])
manifest_path = Path(sys.argv[4])
data = json.loads(evidence_json.read_text(encoding="utf-8"))
if data.get("parity") != "PASS":
    raise SystemExit("CRAM evidence parity is not PASS")
input_summary = data.get("input") or {}
digest = hashlib.sha256(input_cram.read_bytes()).hexdigest()
if input_summary.get("sha256") and input_summary["sha256"] != digest:
    raise SystemExit("CRAM evidence input SHA-256 does not match checked-in input.cram")
commands = {
    row["command"]
    for row in data.get("commands", [])
    if row.get("status") == "PASS"
}
required_core = {
    "CleanSam",
    "CollectQualityYieldMetrics",
    "CollectInsertSizeMetrics",
    "MarkDuplicates",
    "SortSam",
    "AddOrReplaceReadGroups",
}
required_extended = required_core | {
    "ViewSam",
    "ValidateSamFile",
    "CollectAlignmentSummaryMetrics",
    "CollectBaseDistributionByCycle",
    "CollectGcBiasMetrics",
    "MeanQualityByCycle",
    "QualityScoreDistribution",
}
required = (
    required_extended
    if required_extended.issubset(commands)
    else required_core
)
missing = sorted(required - commands)
if missing:
    raise SystemExit(f"CRAM evidence missing commands: {', '.join(missing)}")
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
dataset_id = "gatk-na12878-mito-cram"
entry = next((item for item in manifest["datasets"] if item["id"] == dataset_id), None)
if entry is None:
    raise SystemExit(f"{dataset_id} missing from {manifest_path}")
for key in ("input_path", "evidence_json", "evidence_markdown"):
    path = root / entry[key]
    if not path.exists():
        raise SystemExit(f"{dataset_id} manifest {key} missing file: {path}")
manifest_entry = root / Path(entry["evidence_json"]).parent / "manifest-entry.json"
if manifest_entry.exists():
    manifest_entry_data = json.loads(manifest_entry.read_text(encoding="utf-8"))
    if manifest_entry_data.get("id") != dataset_id:
        raise SystemExit("manifest-entry.json id mismatch")
print(f"{dataset_id} evidence bundle is internally consistent")
PY

python3 "$repo_root/tools/verify_real_data_evidence.py"

echo "gatk-na12878-mito-cram evidence validation passed"