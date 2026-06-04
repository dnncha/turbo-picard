#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conda_prefix="${TURBO_PICARD_CONDA_PREFIX:-$repo_root/.conda-turbo-picard}"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

cat > "$workdir/Rscript" <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod +x "$workdir/Rscript"

if command -v mamba >/dev/null 2>&1; then
  conda_runner=(mamba)
elif command -v micromamba >/dev/null 2>&1; then
  conda_runner=(micromamba)
else
  echo "mamba or micromamba is required for Picard parity verification" >&2
  exit 127
fi

run_picard() {
  "${conda_runner[@]}" run -p "$conda_prefix" env "PATH=$workdir:$conda_prefix/bin:$PATH" picard "$@"
}

cat > "$workdir/input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-a	0	chr1	10	60	4M	*	0	0	ACGT	FFFF
read-b	512	chr1	20	60	4M	*	0	0	NNNN	!!!!
read-c	256	chr1	30	60	4M	*	0	0	ACGT	FFFF
read-d	16	chr1	40	60	4M	*	0	0	ACGT	!!!!	OQ:Z:"#$%
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  MeanQualityByCycle \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo.txt" \
  "CHART=$workdir/turbo.pdf" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

run_picard MeanQualityByCycle \
  "I=$workdir/input.sam" \
  "O=$workdir/picard.txt" \
  "CHART=$workdir/picard.pdf" \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo.txt" "$workdir/picard.txt" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def histogram(path):
    rows = []
    capture = False
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.rstrip("\n")
            if line.startswith("CYCLE\tMEAN_"):
                capture = True
                rows.append(line)
                continue
            if capture and line:
                rows.append(line)
    return rows

if histogram(turbo_path) != histogram(picard_path):
    raise SystemExit("MeanQualityByCycle histogram differs from Picard")
print("MeanQualityByCycle histogram matches Picard")
PY

test -s "$workdir/turbo.pdf"

cargo run -q -p turbo-picard-cli --bin picard -- \
  MeanQualityByCycle \
  "I=$workdir/input.sam" \
  "O=$workdir/turbo-temp-options.txt" \
  "CHART=$workdir/turbo-temp-options.pdf" \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

run_picard MeanQualityByCycle \
  "I=$workdir/input.sam" \
  "O=$workdir/picard-temp-options.txt" \
  "CHART=$workdir/picard-temp-options.pdf" \
  "TMP_DIR=$workdir" \
  MAX_RECORDS_IN_RAM=500 \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-temp-options.txt" "$workdir/picard-temp-options.txt" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def histogram(path):
    rows = []
    capture = False
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.rstrip("\n")
            if line.startswith("CYCLE\tMEAN_"):
                capture = True
                rows.append(line)
                continue
            if capture and line:
                rows.append(line)
    return rows

if histogram(turbo_path) != histogram(picard_path):
    raise SystemExit("MeanQualityByCycle temp-option histogram differs from Picard")
print("MeanQualityByCycle temp-option histogram matches Picard")
PY

cat > "$workdir/stop-after-input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:1000
read-a	0	chr1	10	60	4M	*	0	0	ACGT	FFFF
read-b	0	chr1	20	60	4M	*	0	0	ACGT	!!!!
SAM

cargo run -q -p turbo-picard-cli --bin picard -- \
  MeanQualityByCycle \
  "I=$workdir/stop-after-input.sam" \
  "O=$workdir/turbo-stop-after.txt" \
  "CHART=$workdir/turbo-stop-after.pdf" \
  STOP_AFTER=1 \
  AS=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

run_picard MeanQualityByCycle \
  "I=$workdir/stop-after-input.sam" \
  "O=$workdir/picard-stop-after.txt" \
  "CHART=$workdir/picard-stop-after.pdf" \
  STOP_AFTER=1 \
  AS=true \
  VALIDATION_STRINGENCY=SILENT \
  QUIET=true

python3 - "$workdir/turbo-stop-after.txt" "$workdir/picard-stop-after.txt" <<'PY'
import sys
turbo_path, picard_path = sys.argv[1:]

def histogram(path):
    rows = []
    capture = False
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.rstrip("\n")
            if line.startswith("CYCLE\tMEAN_"):
                capture = True
                rows.append(line)
                continue
            if capture and line:
                rows.append(line)
    return rows

turbo = histogram(turbo_path)
picard = histogram(picard_path)
if turbo != picard:
    raise SystemExit(
        f"MeanQualityByCycle STOP_AFTER/AS histogram differs from Picard:\n"
        f"turbo={turbo}\npicard={picard}"
    )
print("MeanQualityByCycle STOP_AFTER and AS alias histogram matches Picard")
PY
