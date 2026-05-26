#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tempdir="$(mktemp -d "${TMPDIR:-/tmp}/turbo-picard-package.XXXXXX")"
trap 'rm -rf "${tempdir}"' EXIT

install_root="${tempdir}/install"
output_bam="${tempdir}/marked.bam"
metrics="${tempdir}/metrics.txt"

cargo install \
  --locked \
  --no-track \
  --root "${install_root}" \
  --path "${repo_root}/crates/turbo-picard-cli" \
  --bin turbo-picard \
  --bin picard

"${install_root}/bin/turbo-picard" --version
"${install_root}/bin/picard" --version

"${install_root}/bin/picard" MarkDuplicates \
  "I=${repo_root}/fixtures/markduplicates/basic/input.bam" \
  "O=${output_bam}" \
  "M=${metrics}" \
  ASSUME_SORTED=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true \
  READ_NAME_REGEX=null

test -s "${output_bam}"
test -s "${metrics}"
grep -q 'UNPAIRED_READ_DUPLICATES' "${metrics}"

sortsam_input="${tempdir}/sortsam-input.sam"
sortsam_output="${tempdir}/sortsam-output.sam"
cat > "${sortsam_input}" <<'SAM'
@HD	VN:1.6	SO:unsorted
@SQ	SN:chr1	LN:1000
read-c	0	chr1	90	60	10M	*	0	0	CCCCCCCCCC	FFFFFFFFFF
read-a	0	chr1	10	60	10M	*	0	0	AAAAAAAAAA	FFFFFFFFFF
read-b	0	chr1	50	60	10M	*	0	0	BBBBBBBBBB	FFFFFFFFFF
SAM

"${install_root}/bin/picard" SortSam \
  "I=${sortsam_input}" \
  "O=${sortsam_output}" \
  SORT_ORDER=coordinate \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${sortsam_output}"
grep -q $'@HD\tVN:1.6\tSO:coordinate' "${sortsam_output}"
awk '!/^@/ { print $1 }' "${sortsam_output}" | tr '\n' ' ' | grep -q '^read-a read-b read-c $'
