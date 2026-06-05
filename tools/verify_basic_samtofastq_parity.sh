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
non-pf	516	*	0	0	*	*	0	0	CCCC	FFFF
secondary	260	*	0	0	*	*	0	0	GGGG	FFFF
pair-a	77	*	0	0	*	*	0	0	AAAA	FFFF
pair-a	141	*	0	0	*	*	0	0	TTTT	HHHH
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  SamToFastq \
  "I=$workdir/input.sam" \
  "FASTQ=$workdir/turbo-r1.fastq" \
  "SECOND_END_FASTQ=$workdir/turbo-r2.fastq" \
  "UNPAIRED_FASTQ=$workdir/turbo-unpaired.fastq" \
  CREATE_MD5_FILE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard SamToFastq \
  "I=$workdir/input.sam" \
  "FASTQ=$workdir/picard-r1.fastq" \
  "SECOND_END_FASTQ=$workdir/picard-r2.fastq" \
  "UNPAIRED_FASTQ=$workdir/picard-unpaired.fastq" \
  CREATE_MD5_FILE=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-r1.fastq" "$workdir/turbo-r1.fastq"
diff -u "$workdir/picard-r2.fastq" "$workdir/turbo-r2.fastq"
diff -u "$workdir/picard-unpaired.fastq" "$workdir/turbo-unpaired.fastq"
cmp "$workdir/picard-r1.fastq.md5" "$workdir/turbo-r1.fastq.md5"
cmp "$workdir/picard-r2.fastq.md5" "$workdir/turbo-r2.fastq.md5"
cmp "$workdir/picard-unpaired.fastq.md5" "$workdir/turbo-unpaired.fastq.md5"

cat > "$workdir/rereverse-input.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
read-a	16	chr1	10	60	4M	*	0	0	AACG	ABCD
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  SamToFastq \
  "I=$workdir/rereverse-input.sam" \
  "FASTQ=$workdir/turbo-rereverse-false.fastq" \
  RE_REVERSE=false \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard SamToFastq \
  "I=$workdir/rereverse-input.sam" \
  "FASTQ=$workdir/picard-rereverse-false.fastq" \
  RE_REVERSE=false \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-rereverse-false.fastq" "$workdir/turbo-rereverse-false.fastq"
echo "SamToFastq RE_REVERSE=false output matches Picard"

cat > "$workdir/trim-input.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
pair-a	77	*	0	0	*	*	0	0	AACCGG	ABCDEF
pair-a	141	*	0	0	*	*	0	0	TTGGCC	UVWXYZ
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  SamToFastq \
  "I=$workdir/trim-input.sam" \
  "FASTQ=$workdir/turbo-trim-r1.fastq" \
  "SECOND_END_FASTQ=$workdir/turbo-trim-r2.fastq" \
  READ1_TRIM=1 \
  READ1_MAX_BASES_TO_WRITE=3 \
  READ2_TRIM=2 \
  READ2_MAX_BASES_TO_WRITE=2 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard SamToFastq \
  "I=$workdir/trim-input.sam" \
  "FASTQ=$workdir/picard-trim-r1.fastq" \
  "SECOND_END_FASTQ=$workdir/picard-trim-r2.fastq" \
  READ1_TRIM=1 \
  READ1_MAX_BASES_TO_WRITE=3 \
  READ2_TRIM=2 \
  READ2_MAX_BASES_TO_WRITE=2 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-trim-r1.fastq" "$workdir/turbo-trim-r1.fastq"
diff -u "$workdir/picard-trim-r2.fastq" "$workdir/turbo-trim-r2.fastq"

cat > "$workdir/quality-input.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
read-a	4	*	0	0	*	*	0	0	ACGTAC	FFF!!!
read-b	4	*	0	0	*	*	0	0	TGCA	!!!!
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  SamToFastq \
  "I=$workdir/quality-input.sam" \
  "FASTQ=$workdir/turbo-quality.fastq" \
  Q=20 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard SamToFastq \
  "I=$workdir/quality-input.sam" \
  "FASTQ=$workdir/picard-quality.fastq" \
  Q=20 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-quality.fastq" "$workdir/turbo-quality.fastq"

cat > "$workdir/clipping-input.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
read-a	4	*	0	0	*	*	0	0	AACCGG	FFFFFF	XT:i:4
read-b	16	chr1	10	60	6M	*	0	0	AACCGG	ABCDEF	XT:i:4
SAM

for action in N X 2; do
  cargo run -q -p turbo-picard-cli --bin picard -- \
    SamToFastq \
    "I=$workdir/clipping-input.sam" \
    "FASTQ=$workdir/turbo-clipping-${action}.fastq" \
    CLIP_ATTR=XT \
    "CLIP_ACT=$action" \
    CLIP_MIN=2 \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true

  "${conda_runner[@]}" run -p "$conda_prefix" picard SamToFastq \
    "I=$workdir/clipping-input.sam" \
    "FASTQ=$workdir/picard-clipping-${action}.fastq" \
    CLIP_ATTR=XT \
    "CLIP_ACT=$action" \
    CLIP_MIN=2 \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true

  diff -u "$workdir/picard-clipping-${action}.fastq" "$workdir/turbo-clipping-${action}.fastq"
done

cat > "$workdir/runtime-input.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
read-a	4	*	0	0	*	*	0	0	ACGT	FFFF
SAM

cat > "$workdir/ref.fa" <<'FA'
>chr1
ACGTACGTACGT
FA

cargo run -q -p turbo-picard-cli --bin picard -- \
  SamToFastq \
  "I=$workdir/runtime-input.sam" \
  "FASTQ=$workdir/turbo-runtime.fastq" \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  MAX_RECORDS_IN_RAM=1000 \
  "TMP_DIR=$workdir" \
  "R=$workdir/ref.fa" \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard SamToFastq \
  "I=$workdir/runtime-input.sam" \
  "FASTQ=$workdir/picard-runtime.fastq" \
  CREATE_MD5_FILE=true \
  CREATE_INDEX=true \
  MAX_RECORDS_IN_RAM=1000 \
  "TMP_DIR=$workdir" \
  "R=$workdir/ref.fa" \
  USE_JDK_DEFLATER=true \
  USE_JDK_INFLATER=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-runtime.fastq" "$workdir/turbo-runtime.fastq"
cmp "$workdir/picard-runtime.fastq.md5" "$workdir/turbo-runtime.fastq.md5"
test ! -e "$workdir/turbo-runtime.fastq.bai"
test ! -e "$workdir/turbo-runtime.bai"
test ! -e "$workdir/picard-runtime.fastq.bai"
test ! -e "$workdir/picard-runtime.bai"
echo "SamToFastq runtime sidecar compatibility matches Picard"

cat > "$workdir/filter-input.sam" <<'SAM'
@HD	VN:1.6	SO:queryname
@SQ	SN:chr1	LN:1000
pf	4	*	0	0	*	*	0	0	AAAA	FFFF
non-pf	516	*	0	0	*	*	0	0	CCCC	FFFF
secondary	260	*	0	0	*	*	0	0	GGGG	FFFF
supplementary	2048	chr1	10	60	4M	*	0	0	TTTT	FFFF
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  SamToFastq \
  "I=$workdir/filter-input.sam" \
  "FASTQ=$workdir/turbo-included.fastq" \
  INCLUDE_NON_PF_READS=true \
  INCLUDE_NON_PRIMARY_ALIGNMENTS=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard SamToFastq \
  "I=$workdir/filter-input.sam" \
  "FASTQ=$workdir/picard-included.fastq" \
  INCLUDE_NON_PF_READS=true \
  INCLUDE_NON_PRIMARY_ALIGNMENTS=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

diff -u "$workdir/picard-included.fastq" "$workdir/turbo-included.fastq"
echo "SamToFastq FASTQ outputs match Picard"
