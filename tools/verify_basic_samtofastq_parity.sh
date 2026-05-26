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
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
read-a	4	*	0	0	*	*	0	0	ACGT	FFFF
read-b	16	chr1	10	60	4M	*	0	0	AACG	ABCD
pair-a	77	*	0	0	*	*	0	0	AAAA	FFFF
pair-a	141	*	0	0	*	*	0	0	TTTT	HHHH
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  SamToFastq \
  "I=$workdir/input.sam" \
  "FASTQ=$workdir/turbo-r1.fastq" \
  "SECOND_END_FASTQ=$workdir/turbo-r2.fastq" \
  "UNPAIRED_FASTQ=$workdir/turbo-unpaired.fastq" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard SamToFastq \
  "I=$workdir/input.sam" \
  "FASTQ=$workdir/picard-r1.fastq" \
  "SECOND_END_FASTQ=$workdir/picard-r2.fastq" \
  "UNPAIRED_FASTQ=$workdir/picard-unpaired.fastq" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-r1.fastq" "$workdir/turbo-r1.fastq"
diff -u "$workdir/picard-r2.fastq" "$workdir/turbo-r2.fastq"
diff -u "$workdir/picard-unpaired.fastq" "$workdir/turbo-unpaired.fastq"
echo "SamToFastq FASTQ outputs match Picard"
