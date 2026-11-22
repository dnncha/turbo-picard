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

python3 - "$workdir" <<'PY'
from pathlib import Path
import sys

workdir = Path(sys.argv[1])
seq = ("ACGTACGTAC" * 600)[:6000]

# refFlat:
# G_PLUS on chr1 (+): 2 exons [100, 700), [1200, 1800), cds [200, 1700) -> 1200bp exons
# G_PLUS_ALT shorter transcript of same gene G_PLUS: [100, 500), cds [200, 400) -> tests picking best tx per gene
# G_MINUS on chr2 (-): 2 exons [500, 1200), [1600, 2300), cds [600, 2200) -> 1400bp exons
# G_RRNA on chr1 (+): [3000, 3500) (covered by ribosomal interval)
(workdir / "refFlat.txt").write_text(
    "G_PLUS\tT_LONG\tchr1\t+\t100\t1800\t200\t1700\t2\t100,1200,\t700,1800,\n"
    "G_PLUS\tT_SHORT\tchr1\t+\t100\t500\t200\t400\t1\t100,\t500,\n"
    "G_MINUS\tT_NEG\tchr2\t-\t500\t2300\t600\t2200\t2\t500,1600,\t1200,2300,\n",
    encoding="utf-8",
)

(workdir / "ribo.interval_list").write_text(
    "@HD\tVN:1.6\tSO:coordinate\n"
    "@SQ\tSN:chr1\tLN:6000\n"
    "@SQ\tSN:chr2\tLN:6000\n"
    "@SQ\tSN:chrM\tLN:1000\n"
    "chr1\t3001\t3500\t+\trRNA_1\n",
    encoding="utf-8",
)

lines = [
    "@HD\tVN:1.6\tSO:coordinate",
    "@SQ\tSN:chr1\tLN:6000",
    "@SQ\tSN:chr2\tLN:6000",
    "@SQ\tSN:chrM\tLN:1000",
    "@RG\tID:RG1\tSM:SM1\tLB:LB1\tPU:PU1",
    "@RG\tID:RG2\tSM:SM1\tLB:LB1",
]
recs = []
# Spliced reads on chr1 (+) gene G_PLUS (R1 forward, R2 reverse -> matches FIRST_READ_TRANSCRIPTION_STRAND?
# Wait on + gene (gene_neg=False): FIRST_READ_TRANSCRIPTION_STRAND expects R1 forward (read_neg=False -> same_strand=True == expect_same=True)
# R2 reverse (read_neg=True -> same_strand=False == expect_same=False))
for i in range(18):
    pos = 101 + (i % 9) * 60
    rnext_pos = 1201 + (i % 9) * 60
    # Spliced R1: 40M500N40M spanning exon1->exon2
    recs.append((0, pos, f"spliced_{i}\t99\tchr1\t{pos}\t60\t40M500N40M\t=\t{rnext_pos}\t1140\t{seq[:80]}\t{'I'*80}\tRG:Z:RG1\tMC:Z:60M"))
    recs.append((0, rnext_pos, f"spliced_{i}\t147\tchr1\t{rnext_pos}\t60\t60M\t=\t{pos}\t-1140\t{seq[:60]}\t{'I'*60}\tRG:Z:RG1\tMC:Z:40M500N40M"))

# Unpaired reads across G_PLUS exon 1 & 2 (to guarantee coverage >= 1.0 across full transcript)
for i in range(14):
    pos = 101 + i * 40
    recs.append((0, pos, f"p1_{i}\t0\tchr1\t{pos}\t60\t50M\t*\t0\t0\t{seq[:50]}\t{'I'*50}\tRG:Z:RG2"))
    pos2 = 1201 + i * 40
    recs.append((0, pos2, f"p2_{i}\t0\tchr1\t{pos2}\t60\t50M\t*\t0\t0\t{seq[:50]}\t{'I'*50}\tRG:Z:RG1"))

# Intronic read on chr1 (900..950)
recs.append((0, 900, f"intron_1\t0\tchr1\t900\t60\t50M\t*\t0\t0\t{seq[:50]}\t{'I'*50}\tRG:Z:RG1"))
# Intergenic read on chr1 (2500..2550)
recs.append((0, 2500, f"inter_1\t0\tchr1\t2500\t60\t50M\t*\t0\t0\t{seq[:50]}\t{'I'*50}\tRG:Z:RG1"))
# Ribosomal paired read overlapping chr1:3001-3500
recs.append((0, 3050, f"ribo_p\t99\tchr1\t3050\t60\t50M\t=\t3200\t200\t{seq[:50]}\t{'I'*50}\tRG:Z:RG1\tMC:Z:50M"))
recs.append((0, 3200, f"ribo_p\t147\tchr1\t3200\t60\t50M\t=\t3050\t-200\t{seq[:50]}\t{'I'*50}\tRG:Z:RG1\tMC:Z:50M"))
# Secondary, supplementary, QCFAIL, duplicate reads
recs.append((0, 150, f"sec_1\t256\tchr1\t150\t60\t50M\t*\t0\t0\t{seq[:50]}\t{'I'*50}\tRG:Z:RG1"))
recs.append((0, 150, f"supp_1\t2048\tchr1\t150\t60\t50M\t*\t0\t0\t{seq[:50]}\t{'I'*50}\tRG:Z:RG1"))
recs.append((0, 150, f"qcfail_1\t512\tchr1\t150\t60\t50M\t*\t0\t0\t{seq[:50]}\t{'I'*50}\tRG:Z:RG1"))
recs.append((0, 150, f"dup_1\t1024\tchr1\t150\t60\t50M\t*\t0\t0\t{seq[:50]}\t{'I'*50}\tRG:Z:RG1"))

# chr2 negative-strand gene G_MINUS: 1400bp exons [500,1200) and [1600,2300)
for i in range(12):
    pos = 501 + i * 50
    recs.append((1, pos, f"neg1_{i}\t16\tchr2\t{pos}\t60\t50M\t*\t0\t0\t{seq[:50]}\t{'I'*50}\tRG:Z:RG1"))
    pos2 = 1601 + i * 50
    recs.append((1, pos2, f"neg2_{i}\t16\tchr2\t{pos2}\t60\t50M\t*\t0\t0\t{seq[:50]}\t{'I'*50}\tRG:Z:RG2"))

# Ignored contig chrM
recs.append((2, 10, f"mito_1\t0\tchrM\t10\t60\t50M\t*\t0\t0\t{seq[:50]}\t{'I'*50}\tRG:Z:RG1"))
# Unmapped read
recs.append((99, 0, f"unmap_1\t4\t*\t0\t0\t*\t*\t0\t0\t{seq[:40]}\t{'I'*40}\tRG:Z:RG1"))

recs.sort(key=lambda item: (item[0], item[1]))
(workdir / "input.sam").write_text("\n".join(lines + [r[2] for r in recs]) + "\n", encoding="utf-8")
PY

if command -v samtools >/dev/null 2>&1; then
  samtools view -bS "$workdir/input.sam" -o "$workdir/input.bam"
  samtools index "$workdir/input.bam"
elif [[ -n "${TURBO_PICARD_PICARD_JAR:-}" ]]; then
  run_picard SortSam "I=$workdir/input.sam" "O=$workdir/input.bam" SO=coordinate CREATE_INDEX=true QUIET=true
else
  "${conda_runner[@]}" run -p "$conda_prefix" samtools view -bS "$workdir/input.sam" -o "$workdir/input.bam"
  "${conda_runner[@]}" run -p "$conda_prefix" samtools index "$workdir/input.bam"
fi

# Verify all 3 strand modes, ribosomal intervals, IGNORE_SEQUENCE, multi-level, CHART_OUTPUT
run_case() {
  label="$1"
  strand="$2"
  ribo_arg="$3"
  threads="$4"

  turbo_args=(
    CollectRnaSeqMetrics
    "I=$workdir/input.bam"
    "O=$workdir/turbo-$label.txt"
    "REF_FLAT=$workdir/refFlat.txt"
    "STRAND=$strand"
    "IGNORE_SEQUENCE=chrM"
    "LEVEL=ALL_READS"
    "LEVEL=SAMPLE"
    "LEVEL=LIBRARY"
    "LEVEL=READ_GROUP"
    "CHART=$workdir/turbo-$label.pdf"
    "QUIET=true"
  )
  picard_args=(
    CollectRnaSeqMetrics
    "I=$workdir/input.bam"
    "O=$workdir/picard-$label.txt"
    "REF_FLAT=$workdir/refFlat.txt"
    "STRAND_SPECIFICITY=$strand"
    "IGNORE_SEQUENCE=chrM"
    "METRIC_ACCUMULATION_LEVEL=ALL_READS"
    "METRIC_ACCUMULATION_LEVEL=SAMPLE"
    "METRIC_ACCUMULATION_LEVEL=LIBRARY"
    "METRIC_ACCUMULATION_LEVEL=READ_GROUP"
    "QUIET=true"
  )
  if [[ -n "$ribo_arg" ]]; then
    turbo_args+=("RIBOSOMAL_INTERVALS=$ribo_arg")
    picard_args+=("RIBOSOMAL_INTERVALS=$ribo_arg")
  fi

  TURBO_PICARD_THREADS="$threads" run_turbo "${turbo_args[@]}"
  run_picard "${picard_args[@]}"
}

run_case "none_ribo_t1" "NONE" "$workdir/ribo.interval_list" "1"
run_case "first_noribo_t4" "FIRST_READ_TRANSCRIPTION_STRAND" "" "4"
run_case "second_ribo_t4" "SECOND_READ_TRANSCRIPTION_STRAND" "$workdir/ribo.interval_list" "4"

python3 - "$workdir" <<'PY'
from pathlib import Path
import sys

workdir = Path(sys.argv[1])


def parse_metrics(path: Path):
    lines = [l.rstrip("\n") for l in path.read_text(encoding="utf-8").splitlines()]
    metrics_rows = []
    hist_header = None
    hist_rows = []
    i = 0
    while i < len(lines):
        if lines[i].startswith("## METRICS CLASS\tpicard.analysis.RnaSeqMetrics"):
            header = lines[i + 1].split("\t")
            i += 2
            while i < len(lines) and lines[i].strip():
                vals = lines[i].split("\t")
                metrics_rows.append(dict(zip(header, vals)))
                i += 1
        elif lines[i].startswith("## HISTOGRAM\t"):
            hist_header = lines[i + 1].split("\t")
            i += 2
            while i < len(lines) and lines[i].strip():
                hist_rows.append(lines[i].split("\t"))
                i += 1
        else:
            i += 1
    return metrics_rows, hist_header, hist_rows


for label in ("none_ribo_t1", "first_noribo_t4", "second_ribo_t4"):
    pdf = workdir / f"turbo-{label}.pdf"
    if not pdf.read_bytes().startswith(b"%PDF-1."):
        raise SystemExit(f"missing PDF header in {pdf}")
    t_rows, t_hh, t_hr = parse_metrics(workdir / f"turbo-{label}.txt")
    p_rows, p_hh, p_hr = parse_metrics(workdir / f"picard-{label}.txt")
    if len(t_rows) != len(p_rows):
        raise SystemExit(f"{label}: row count mismatch turbo={len(t_rows)} picard={len(p_rows)}")
    for idx, (tr, pr) in enumerate(zip(t_rows, p_rows)):
        if tr != pr:
            diff = {k: (tr.get(k), pr.get(k)) for k in pr if tr.get(k) != pr.get(k)}
            raise SystemExit(f"{label} row {idx} mismatch: {diff}")
    if t_hh != p_hh or t_hr != p_hr:
        raise SystemExit(f"{label} histogram mismatch:\nturbo_hh={t_hh}\npicard_hh={p_hh}\nfirst_diff={(t_hr[:3], p_hr[:3])}")

print("CollectRnaSeqMetrics metrics, multi-level rows, chart PDF, and histogram match Picard")
PY
