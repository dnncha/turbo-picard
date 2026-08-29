#!/usr/bin/env bash
set -euo pipefail

wheel="${1:?usage: verify_pypi_wheel_install_smoke.sh PATH_TO_WHEEL}"
test -s "${wheel}"

tempdir="$(mktemp -d "${TMPDIR:-/tmp}/turbo-picard-wheel-smoke.XXXXXX")"
trap 'rm -rf "${tempdir}"' EXIT

python3 -m venv "${tempdir}/venv"
bin="${tempdir}/venv/bin"
"${bin}/python" -m pip install --no-index --no-deps "${wheel}"
"${bin}/python" -m pip check
"${bin}/turbo-picard" --version
"${bin}/turbo-picard" doctor
"${bin}/turbo-picard" trial MarkDuplicates I=input.bam O=marked.bam M=metrics.txt
"${bin}/picard" --version
PATH="${bin}:${PATH}" bash tools/verify_install_smoke.sh
PATH="${bin}:${PATH}" TURBO_PICARD_BIN="${bin}/turbo-picard" \
  bash tools/verify_mate_barcode_install_smoke.sh

hs_dir="${tempdir}/hs-metrics"
mkdir -p "${hs_dir}"
cat >"${hs_dir}/reference.fa" <<'EOF'
>chr1
ACGTACGTACGTACGTACGT
EOF
cat >"${hs_dir}/input.sam" <<'EOF'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:20
read-a	0	chr1	1	60	10M	*	0	0	ACGTACGTAC	FFFFFFFFFF
EOF
cat >"${hs_dir}/targets.interval_list" <<'EOF'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:20
chr1	1	10	+	target
EOF
"${bin}/turbo-picard" CollectHsMetrics \
  "I=${hs_dir}/input.sam" \
  "O=${hs_dir}/hs_metrics.txt" \
  "BAIT=${hs_dir}/targets.interval_list" \
  "TARGET=${hs_dir}/targets.interval_list" \
  "R=${hs_dir}/reference.fa" \
  "PER_TARGET_COVERAGE=${hs_dir}/per_target.txt" \
  "PER_BASE_COVERAGE=${hs_dir}/per_base.txt" \
  MINIMUM_MAPPING_QUALITY=0 \
  SAMPLE_SIZE=0 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true
test -s "${hs_dir}/hs_metrics.txt"
test -s "${hs_dir}/per_target.txt"
test -s "${hs_dir}/per_base.txt"
grep -q 'picard.analysis.directed.HsMetrics' "${hs_dir}/hs_metrics.txt"
grep -q $'chrom\tstart\tend\tlength\tname\t%gc' "${hs_dir}/per_target.txt"
grep -q $'chrom\tpos\ttarget\tcoverage' "${hs_dir}/per_base.txt"

echo "turbo-picard PyPI wheel install smoke passed: ${wheel}"
