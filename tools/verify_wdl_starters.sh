#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/.." && pwd)"
workflows_dir="${repo_root}/packaging/workflows"
miniwdl_bin="${MINIWDL_BIN:-miniwdl}"
wdl_image="${TURBO_PICARD_WDL_IMAGE:-}"

command -v "${miniwdl_bin}" >/dev/null
wdl_files=("${workflows_dir}"/*.wdl)
"${miniwdl_bin}" check --strict "${wdl_files[@]}"

fixture="${repo_root}/fixtures/markduplicates/basic/input.bam"
test -s "${fixture}"

if [[ -z "${wdl_image}" ]]; then
  echo "turbo-picard WDL starter static smoke passed"
  exit 0
fi

workdir="$(mktemp -d "${TMPDIR:-/tmp}/turbo-picard-wdl-starter.XXXXXX")"
trap 'rm -rf -- "${workdir}"' EXIT
output_json="${workdir}/outputs.json"
runtime_defaults="$(python3 -c 'import json, sys; print(json.dumps({"docker": sys.argv[1]}))' "${wdl_image}")"

"${miniwdl_bin}" run \
  --no-color \
  --no-cache \
  --dir "${workdir}" \
  --runtime-defaults "${runtime_defaults}" \
  -o "${output_json}" \
  "${workflows_dir}/trial.wdl" \
  "input_bam=${fixture}" \
  sample_id=basic

python3 - "${output_json}" <<'PY'
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
outputs = payload.get("outputs")
if not isinstance(outputs, dict):
    raise SystemExit("miniwdl output JSON is missing outputs")

marked_bam = outputs.get("TurboPicardTrial.marked_bam")
metrics = outputs.get("TurboPicardTrial.metrics")
if not isinstance(marked_bam, str) or not isinstance(metrics, str):
    raise SystemExit("miniwdl output JSON is missing WDL trial outputs")

marked_bam_path = Path(marked_bam)
metrics_path = Path(metrics)
for path, label in ((marked_bam_path, "marked BAM"), (metrics_path, "metrics")):
    if not path.is_file() or path.stat().st_size == 0:
        raise SystemExit(f"WDL trial {label} is missing or empty: {path}")

if "picard.sam.DuplicationMetrics" not in metrics_path.read_text(encoding="utf-8"):
    raise SystemExit(f"WDL trial metrics do not contain DuplicationMetrics: {metrics_path}")
PY

echo "turbo-picard WDL starter smoke passed"
