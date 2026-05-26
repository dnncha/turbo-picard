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
@RG	ID:old	LB:old-lib	PL:ILLUMINA	PU:old-unit	SM:old-sample
read-a	0	chr1	10	60	4M	*	0	0	ACGT	FFFF	RG:Z:old
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  AddOrReplaceReadGroups \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo.sam" \
  RGID=new \
  RGLB=library-a \
  RGPL=ILLUMINA \
  RGPU=unit-a \
  RGSM=sample-a \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard AddOrReplaceReadGroups \
  "I=$workdir/input.sam" \
  "O=$workdir/picard.sam" \
  RGID=new \
  RGLB=library-a \
  RGPL=ILLUMINA \
  RGPU=unit-a \
  RGSM=sample-a \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard.sam" "$workdir/turbo.sam"
echo "AddOrReplaceReadGroups output matches Picard"
