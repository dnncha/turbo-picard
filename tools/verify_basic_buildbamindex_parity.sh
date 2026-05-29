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
read-a	0	chr1	10	60	10M	*	0	0	AAAAAAAAAA	FFFFFFFFFF
read-b	0	chr1	50	60	10M	*	0	0	BBBBBBBBBB	FFFFFFFFFF
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  SortSam "I=$workdir/input.sam" "O=$workdir/input.bam" SORT_ORDER=coordinate

cargo run -q -p turbo-picard-cli --bin picard -- \
  BuildBamIndex "I=$workdir/input.bam"

cp "$workdir/input.bai" "$workdir/turbo.bai"

"${conda_runner[@]}" run -p "$conda_prefix" picard BuildBamIndex \
  "I=$workdir/input.bam" "O=$workdir/picard.bai" \
  VALIDATION_STRINGENCY=SILENT QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" samtools idxstats \
  "$workdir/input.bam" > "$workdir/idxstats.txt"
test -s "$workdir/turbo.bai"
test -s "$workdir/picard.bai"
grep -q '^chr1' "$workdir/idxstats.txt"
