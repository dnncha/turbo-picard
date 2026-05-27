#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conda_prefix="${TURBO_PICARD_CONDA_PREFIX:-$repo_root/.conda-turbo-picard}"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

if command -v mamba >/dev/null 2>&1; then
  conda_runner=(mamba)
elif command -v micromamba >/dev/null 2>&1; then
  conda_runner=(micromamba)
else
  echo "mamba or micromamba is required for Picard parity verification" >&2
  exit 127
fi

cat > "$workdir/input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-a	0	chr1	10	60	4M	*	0	0	ACGT	FFFF
read-b	4	*	0	0	*	*	0	0	NNNN	!!!!
read-c	16	chr1	20	30	4M	*	0	0	AACG	ABCD
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  CollectAlignmentSummaryMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo.txt" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CollectAlignmentSummaryMetrics \
  "I=$workdir/input.sam" \
  "O=$workdir/picard.txt" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.txt" "$workdir/picard.txt" <<'PY'
import sys

turbo_path, picard_path = sys.argv[1:]

def stable_metrics(path):
    lines = []
    with open(path, encoding="utf-8") as handle:
        keep = False
        for raw in handle:
            line = raw.rstrip("\n")
            if line.startswith("## METRICS CLASS") or line.startswith("CATEGORY\t"):
                keep = True
            if keep and line:
                lines.append(line)
    return lines

turbo = stable_metrics(turbo_path)
picard = stable_metrics(picard_path)
if turbo != picard:
    print("turbo stable metrics:", file=sys.stderr)
    print("\n".join(turbo), file=sys.stderr)
    print("picard stable metrics:", file=sys.stderr)
    print("\n".join(picard), file=sys.stderr)
    raise SystemExit("CollectAlignmentSummaryMetrics output differs from Picard")
print("CollectAlignmentSummaryMetrics stable output matches Picard")
PY
