#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conda_prefix="${TURBO_PICARD_CONDA_PREFIX:-$repo_root/.conda-turbo-picard}"
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

if [[ -n "${TURBO_PICARD_PICARD_JAR:-}" ]]; then
  conda_runner=()
elif command -v mamba >/dev/null 2>&1; then
  conda_runner=(mamba)
elif command -v micromamba >/dev/null 2>&1; then
  conda_runner=(micromamba)
else
  echo "mamba or micromamba is required for Picard parity verification" >&2
  exit 127
fi

cat > "$workdir/reference.fa" <<'FASTA'
>chr1
ACGTACGTACGTACGTACGT
FASTA

python3 - "$workdir/reference.fa" <<'PY'
from pathlib import Path
import sys

fasta = Path(sys.argv[1])
lines = fasta.read_bytes().splitlines(keepends=True)
name = None
sequence = bytearray()
sequence_offset = None
line_bases = None
line_width = None
offset = 0
for line in lines:
    if line.startswith(b">"):
        name = line[1:].strip().decode()
        sequence_offset = offset + len(line)
    elif name is not None:
        bases = line.rstrip(b"\r\n")
        if line_bases is None:
            line_bases = len(bases)
            line_width = len(line)
        sequence.extend(bases)
    offset += len(line)
if name is None or sequence_offset is None or line_bases is None or line_width is None:
    raise SystemExit("failed to build FASTA index")
fasta.with_suffix(fasta.suffix + ".fai").write_text(
    f"{name}\t{len(sequence)}\t{sequence_offset}\t{line_bases}\t{line_width}\n",
    encoding="utf-8",
)
PY

cat > "$workdir/input.sam" <<'SAM'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:20
good	0	chr1	1	60	10M	*	0	0	ACGTACGTAC	FFFFFFFFFF
low-mapq	0	chr1	1	10	10M	*	0	0	ACGTACGTAC	FFFFFFFFFF
low-baseq	0	chr1	1	60	4M	*	0	0	ACGT	!!!!
duplicate	1024	chr1	1	60	4M	*	0	0	ACGT	FFFF
off-target	0	chr1	16	60	4M	*	0	0	ACGT	FFFF
pair	67	chr1	1	60	10M	=	1	20	ACGTACGTAC	FFFFFFFFFF
pair	131	chr1	1	60	10M	=	1	-20	ACGTACGTAC	FFFFFFFFFF
SAM

cat > "$workdir/targets.interval_list" <<'INTERVALS'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:20
chr1	1	10	+	target
chr1	12	15	+	untouched-target
INTERVALS

run_turbo() {
  cargo run -q -p turbo-picard-cli --bin picard -- "$@"
}

run_picard() {
  if [[ -n "${TURBO_PICARD_PICARD_JAR:-}" ]]; then
    "${TURBO_PICARD_JAVA:-java}" -jar "$TURBO_PICARD_PICARD_JAR" "$@"
  else
    "${conda_runner[@]}" run -p "$conda_prefix" picard "$@"
  fi
}

run_case() {
  clip="$1"
  run_turbo \
    CollectHsMetrics \
    "I=$workdir/input.sam" \
    "O=$workdir/turbo-$clip.txt" \
    "BAIT=$workdir/targets.interval_list" \
    "TARGET=$workdir/targets.interval_list" \
    "R=$workdir/reference.fa" \
    BAIT_SET_NAME=fixture \
    PER_TARGET_COVERAGE="$workdir/turbo-$clip.per-target.txt" \
    PER_BASE_COVERAGE="$workdir/turbo-$clip.per-base.txt" \
    MINIMUM_MAPPING_QUALITY=20 \
    MINIMUM_BASE_QUALITY=20 \
    NEAR_DISTANCE=0 \
    CLIP_OVERLAPPING_READS="$clip" \
    SAMPLE_SIZE=0 \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true

  run_picard \
    CollectHsMetrics \
    "I=$workdir/input.sam" \
    "O=$workdir/picard-$clip.txt" \
    "BAIT_INTERVALS=$workdir/targets.interval_list" \
    "TARGET_INTERVALS=$workdir/targets.interval_list" \
    "R=$workdir/reference.fa" \
    BAIT_SET_NAME=fixture \
    PER_TARGET_COVERAGE="$workdir/picard-$clip.per-target.txt" \
    PER_BASE_COVERAGE="$workdir/picard-$clip.per-base.txt" \
    MINIMUM_MAPPING_QUALITY=20 \
    MINIMUM_BASE_QUALITY=20 \
    NEAR_DISTANCE=0 \
    CLIP_OVERLAPPING_READS="$clip" \
    SAMPLE_SIZE=0 \
    VALIDATION_STRINGENCY=SILENT \
    QUIET=true
}

run_case true
run_case false

python3 - "$workdir" <<'PY'
from pathlib import Path
import sys

workdir = Path(sys.argv[1])


def tables(path: Path):
    lines = [line.rstrip("\n") for line in path.read_text(encoding="utf-8").splitlines()]
    metrics = None
    histogram = []
    for index, line in enumerate(lines):
        if line.startswith("BAIT_SET\t"):
            metrics = dict(zip(line.split("\t"), lines[index + 1].split("\t")))
        if line.startswith("coverage_or_base_quality\t"):
            histogram = [line for line in lines[index + 1 :] if line]
            break
    if metrics is None:
        raise SystemExit(f"no HsMetrics table in {path}")
    return metrics, histogram


for clip in ("true", "false"):
    turbo, turbo_histogram = tables(workdir / f"turbo-{clip}.txt")
    picard, picard_histogram = tables(workdir / f"picard-{clip}.txt")
    if turbo != picard:
        differing = [key for key in picard if turbo.get(key) != picard[key]]
        raise SystemExit(
            f"CollectHsMetrics CLIP_OVERLAPPING_READS={clip} differs in {differing}: "
            f"turbo={turbo} picard={picard}"
        )
    if turbo_histogram != picard_histogram:
        raise SystemExit(
            f"CollectHsMetrics CLIP_OVERLAPPING_READS={clip} histogram differs: "
            f"turbo={turbo_histogram[:8]} picard={picard_histogram[:8]}"
        )
    for suffix in ("per-target", "per-base"):
        turbo_sidecar = (workdir / f"turbo-{clip}.{suffix}.txt").read_text(encoding="utf-8")
        picard_sidecar = (workdir / f"picard-{clip}.{suffix}.txt").read_text(encoding="utf-8")
        if turbo_sidecar != picard_sidecar:
            raise SystemExit(
                f"CollectHsMetrics CLIP_OVERLAPPING_READS={clip} {suffix} sidecar differs:\n"
                f"turbo={turbo_sidecar}\npicard={picard_sidecar}"
            )

print("CollectHsMetrics core metrics, histogram, and sidecars match Picard")
PY
