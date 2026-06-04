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

cat > "$workdir/ref.fa" <<'FA'
>chr1
ACGTACGTACGT
FA

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

cargo run -q -p turbo-picard-cli --bin picard -- \
  AddOrReplaceReadGroups \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-extra.sam" \
  RGID=new \
  RGLB=library-a \
  RGPL=ILLUMINA \
  RGPU=unit-a \
  RGSM=sample-a \
  RGKS=ACGT \
  RGFO=TACG \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard AddOrReplaceReadGroups \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-extra.sam" \
  RGID=new \
  RGLB=library-a \
  RGPL=ILLUMINA \
  RGPU=unit-a \
  RGSM=sample-a \
  RGKS=ACGT \
  RGFO=TACG \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-extra.sam" "$workdir/turbo-extra.sam"
echo "AddOrReplaceReadGroups RGKS/RGFO output matches Picard"

cargo run -q -p turbo-picard-cli --bin picard -- \
  AddOrReplaceReadGroups \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-sidecars.sam" \
  RGID=new \
  RGLB=library-a \
  RGPL=ILLUMINA \
  RGPU=unit-a \
  RGSM=sample-a \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  MAX_RECORDS_IN_RAM=1000 \
  "TMP_DIR=$workdir" \
  "R=$workdir/ref.fa" \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard AddOrReplaceReadGroups \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-sidecars.sam" \
  RGID=new \
  RGLB=library-a \
  RGPL=ILLUMINA \
  RGPU=unit-a \
  RGSM=sample-a \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  MAX_RECORDS_IN_RAM=1000 \
  "TMP_DIR=$workdir" \
  "R=$workdir/ref.fa" \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-sidecars.sam" "$workdir/turbo-sidecars.sam"
test -f "$workdir/turbo-sidecars.sam.md5"
test -f "$workdir/picard-sidecars.sam.md5"
grep -Eq '^[0-9a-f]{32}$' "$workdir/turbo-sidecars.sam.md5"
grep -Eq '^[0-9a-f]{32}$' "$workdir/picard-sidecars.sam.md5"
test ! -e "$workdir/turbo-sidecars.sam.bai"
test ! -e "$workdir/turbo-sidecars.bai"
test ! -e "$workdir/picard-sidecars.sam.bai"
test ! -e "$workdir/picard-sidecars.bai"
echo "AddOrReplaceReadGroups CREATE_MD5_FILE/runtime compatibility matches Picard"
