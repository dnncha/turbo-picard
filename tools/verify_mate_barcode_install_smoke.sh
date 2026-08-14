#!/usr/bin/env bash
set -euo pipefail

binary="${TURBO_PICARD_BIN:-turbo-picard}"
command -v "${binary}" >/dev/null

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/.." && pwd)"
fixture="${repo_root}/fixtures/markduplicates/read-barcode-tags/input.bam"
test -s "${fixture}"

workdir="$(mktemp -d "${TMPDIR:-/tmp}/turbo-picard-mate-barcode-smoke.XXXXXX")"
trap 'rm -rf "${workdir}"' EXIT

output="${workdir}/marked.bam"
metrics="${workdir}/metrics.txt"

"${binary}" MarkDuplicates \
  "I=${fixture}" \
  "O=${output}" \
  "M=${metrics}" \
  READ_ONE_BARCODE_TAG=BX \
  READ_TWO_BARCODE_TAG=BY \
  READ_NAME_REGEX=null \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

test -s "${output}"
test -s "${metrics}"

# This is the Picard 3.4.0 histogram contract for the checked-in fixture. The
# public 0.1.11 wheel previously reported all_sets=1 for the size-2 bin; keep
# the check here so a release artifact cannot silently reintroduce that drift.
awk -F '\t' '
$1 == "lib1" {
  library = ($2 + 0 == 0 && $3 + 0 == 3 && $6 + 0 == 0 && $7 + 0 == 1)
}
$1 == "1.0" { one = ($3 + 0 == 1 && $4 + 0 == 1) }
$1 == "2.0" { two = ($3 + 0 == 0 && $4 + 0 == 0) }
END { exit(library && one && two ? 0 : 1) }
' "${metrics}"

echo "turbo-picard mate-specific barcode install smoke passed"
