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

echo "turbo-picard PyPI wheel install smoke passed: ${wheel}"
