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
@SQ	SN:chr1	LN:10
mapped	0	chr1	2	60	4M	*	0	0	ACGT	FFFF
overhang	0	chr1	8	60	5M	*	0	0	ACGTA	FFFFF
unmapped	4	*	0	60	*	*	0	0	NNNN	!!!!
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  CleanSam \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard CleanSam \
  "I=$workdir/input.sam" \
  "O=$workdir/picard.sam" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.sam" "$workdir/picard.sam" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def records(path):
    data = {}
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("@"):
                continue
            fields = line.rstrip("\n").split("\t")
            data[fields[0]] = (fields[4], fields[5])
    return data

if records(turbo_path) != records(picard_path):
    raise SystemExit(f"CleanSam MAPQ/CIGAR differs:\nturbo={records(turbo_path)}\npicard={records(picard_path)}")
print("CleanSam basic MAPQ/CIGAR output matches Picard")
PY
