#!/usr/bin/env bash
set -euo pipefail
# Review paths and fixed trial options before execution. No input is uploaded.
# Set TMPDIR to suitable scratch storage before running, if needed.
python3 -c 'import sys; assert sys.version_info >= (3, 11), "Python 3.11+ required"'
for tool in git java samtools; do command -v "$tool" >/dev/null; done
work="$(mktemp -d)"
printf 'Trial directory: %s\n' "$work"
python3 -m venv "$work/venv"
"$work/venv/bin/python" -m pip install --only-binary=:all: 'turbo-picard==0.1.13'
git clone --depth 1 --branch 'v0.1.13' \
  https://github.com/dnncha/turbo-picard.git "$work/source"
turbo_prefix="$("$work/venv/bin/python" -c 'import shlex,sys; from pathlib import Path; print(shlex.quote(str(Path(sys.executable).with_name("turbo-picard"))))')"
"$work/venv/bin/python" "$work/source/tools/compare_real_data.py" \
  --skip-build --commands MarkDuplicates \
  --input-bam '/data/sample.bam' \
  --picard-command 'java -jar '"'"'/opt/picard/picard.jar'"'"'' \
  --turbo-picard-command "$turbo_prefix" \
  --output-dir "$work/results" \
  --shareable-report "$work/results/shareable.md"
printf 'Evidence retained in %s/results\n' "$work"
# Compare status, scientific outputs and your downstream consumer.
# One timing per side is not a repeat-run benchmark. Failed runs are retained.
