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

cat > "$workdir/input.vcf" <<'VCF'
##fileformat=VCFv4.2
##contig=<ID=old,length=10>
##source=test
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
chr2	7	.	A	C	.	PASS	.
VCF

cat > "$workdir/reference.dict" <<'DICT'
@HD	VN:1.6
@SQ	SN:chr1	LN:1000	M5:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa	AS:GRCh38
@SQ	SN:chr2	LN:2000	UR:file:///ref.fa
DICT

cargo run -q -p turbo-picard-cli --bin picard -- \
  UpdateVcfSequenceDictionary \
  "I=$workdir/input.vcf" \
  "O=$workdir/turbo.vcf" \
  "SEQUENCE_DICTIONARY=$workdir/reference.dict" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard UpdateVcfSequenceDictionary \
  "I=$workdir/input.vcf" \
  "O=$workdir/picard.vcf" \
  "SEQUENCE_DICTIONARY=$workdir/reference.dict" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.vcf" "$workdir/picard.vcf" <<'PY'
import re
import sys
turbo_path, picard_path = sys.argv[1:]

def contig_ids(path):
    ids = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("##contig=<"):
                match = re.search(r"ID=([^,>]+)", line)
                if match:
                    ids.append(match.group(1))
    return ids

def records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if not line.startswith("#")]

if contig_ids(turbo_path) != contig_ids(picard_path):
    raise SystemExit("UpdateVcfSequenceDictionary contig IDs differ from Picard")
if records(turbo_path) != records(picard_path):
    raise SystemExit("UpdateVcfSequenceDictionary records differ from Picard")
print("UpdateVcfSequenceDictionary basic output matches Picard")
PY
