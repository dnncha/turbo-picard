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

cat > "$workdir/first.vcf" <<'VCF'
##fileformat=VCFv4.2
##contig=<ID=chr1,length=1000,md5=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,assembly=GRCh38>
##contig=<ID=chr2,length=2000,URI=file:///ref.fa>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
chr2	3	.	A	C	.	PASS	.
VCF

cat > "$workdir/second.vcf" <<'VCF'
##fileformat=VCFv4.2
##contig=<ID=chr1,length=1000,md5=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,assembly=GRCh38>
##contig=<ID=chr2,length=2000,URI=file:///ref.fa>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
chr1	2	.	T	C	.	PASS	.
VCF

cat > "$workdir/reference.dict" <<'DICT'
@HD	VN:1.6
@SQ	SN:chr1	LN:1000	M5:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa	AS:GRCh38
@SQ	SN:chr2	LN:2000	UR:file:///ref.fa
DICT

cargo run -q -p turbo-picard-cli --bin picard -- \
  MergeVcfs \
  "I=$workdir/first.vcf" \
  "I=$workdir/second.vcf" \
  "O=$workdir/turbo.vcf" \
  "SEQUENCE_DICTIONARY=$workdir/reference.dict" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard MergeVcfs \
  "I=$workdir/first.vcf" \
  "I=$workdir/second.vcf" \
  "O=$workdir/picard.vcf" \
  "SEQUENCE_DICTIONARY=$workdir/reference.dict" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.vcf" "$workdir/picard.vcf" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if not line.startswith("#")]

def contig_lines(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line.startswith("##contig=<")]

if contig_lines(turbo_path) != contig_lines(picard_path):
    raise SystemExit("MergeVcfs contig headers differ from Picard")
if records(turbo_path) != records(picard_path):
    raise SystemExit("MergeVcfs records differ from Picard")
print("MergeVcfs basic output matches Picard")
PY
