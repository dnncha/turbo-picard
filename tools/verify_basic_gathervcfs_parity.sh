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
##contig=<ID=chr1,length=1000>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
chr1	1	.	A	C	.	PASS	.
VCF

cat > "$workdir/second.vcf" <<'VCF'
##fileformat=VCFv4.2
##contig=<ID=chr1,length=1000>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
chr1	5	.	G	T	.	PASS	.
VCF

cargo run -q -p turbo-picard-cli --bin picard -- \
  GatherVcfs \
  "I=$workdir/first.vcf" \
  "I=$workdir/second.vcf" \
  "O=$workdir/turbo.vcf" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard GatherVcfs \
  "I=$workdir/first.vcf" \
  "I=$workdir/second.vcf" \
  "O=$workdir/picard.vcf" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.vcf" "$workdir/picard.vcf" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if not line.startswith("#")]

if records(turbo_path) != records(picard_path):
    raise SystemExit("GatherVcfs records differ from Picard")
print("GatherVcfs basic records match Picard")
PY
