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
##contig=<ID=chr1,length=100>
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
chr1	10	.	A	C	.	PASS	.
chr1	11	.	G	T	.	PASS	.
VCF

cat > "$workdir/ref.fa" <<'FASTA'
>chr1
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
FASTA

cat > "$workdir/ref.dict" <<'DICT'
@HD	VN:1.6
@SQ	SN:chr1	LN:100
DICT

cat > "$workdir/identity.chain" <<'CHAIN'
chain 100 chr1 100 + 0 100 chr1 100 + 0 100 1
100
CHAIN

cargo run -q -p turbo-picard-cli --bin picard -- \
  LiftoverVcf \
  "I=$workdir/input.vcf" \
  "O=$workdir/turbo.vcf" \
  "CHAIN=$workdir/identity.chain" \
  "REJECT=$workdir/turbo-reject.vcf" \
  "R=$workdir/ref.fa" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

"${conda_runner[@]}" run -p "$conda_prefix" picard LiftoverVcf \
  "I=$workdir/input.vcf" \
  "O=$workdir/picard.vcf" \
  "CHAIN=$workdir/identity.chain" \
  "REJECT=$workdir/picard-reject.vcf" \
  "R=$workdir/ref.fa" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.vcf" "$workdir/picard.vcf" "$workdir/turbo-reject.vcf" "$workdir/picard-reject.vcf" <<'PY'
import sys
turbo, picard, turbo_reject, picard_reject = sys.argv[1:]

def records(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if not line.startswith("#")]

def filters(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line.startswith("##FILTER=")]

if records(turbo) != records(picard):
    raise SystemExit(f"LiftoverVcf lifted records differ:\nturbo={records(turbo)}\npicard={records(picard)}")
if records(turbo_reject) != records(picard_reject):
    raise SystemExit(f"LiftoverVcf reject records differ:\nturbo={records(turbo_reject)}\npicard={records(picard_reject)}")
if filters(turbo_reject) != filters(picard_reject):
    raise SystemExit("LiftoverVcf reject FILTER header differs from Picard")
print("LiftoverVcf basic lifted and rejected records match Picard")
PY
