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
