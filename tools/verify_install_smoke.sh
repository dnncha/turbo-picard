#!/usr/bin/env bash
set -euo pipefail

binary="${TURBO_PICARD_BIN:-turbo-picard}"
command -v "${binary}" >/dev/null

workdir="$(mktemp -d "${TMPDIR:-/tmp}/turbo-picard-install-smoke.XXXXXX")"
trap 'rm -rf "${workdir}"' EXIT

input="${workdir}/input.sam"
output="${workdir}/marked.sam"
metrics="${workdir}/metrics.txt"

cat >"${input}" <<'EOF'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-a	0	chr1	1	60	4M	*	0	0	ACGT	FFFF
read-b	0	chr1	1	60	4M	*	0	0	ACGT	FFFF
EOF

"${binary}" MarkDuplicates \
  "I=${input}" \
  "O=${output}" \
  "M=${metrics}" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${output}"
test -s "${metrics}"
awk -F '\t' '$1 == "read-b" && ($2 + 0) >= 1024 { found = 1 } END { exit(found ? 0 : 1) }' "${output}"
grep -q '^## METRICS CLASS' "${metrics}"
grep -q 'UNPAIRED_READ_DUPLICATES' "${metrics}"

echo "turbo-picard install smoke passed"
