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

# Build 2-contig reference with short + long homopolymers and varied GC
chr1 = (
    "ACGTACGTAC" * 10
    + "TTTTTTTGGGGGAAAAAAACCCCC"
    + "ACGTCGATCGATCGATCGAT" * 10
    + "GGGGGGGGACTAGCTAGCTA"
    + "ACGTACGTAC" * 170
)[:2000]
chr2 = ("AACCGGTTACACGTACGTGA" * 50)[:1000]

fa_path = workdir / "reference.fa"
fa_path.write_text(f">chr1\n{chr1}\n>chr2\n{chr2}\n", encoding="utf-8")

# Write .fai
fai_lines = [
    f"chr1\t{len(chr1)}\t6\t{len(chr1)}\t{len(chr1)+1}\n",
    f"chr2\t{len(chr2)}\t{6 + len(chr1) + 1 + 6}\t{len(chr2)}\t{len(chr2)+1}\n",
]
(workdir / "reference.fai").write_text("".join(fai_lines), encoding="utf-8")
(workdir / "reference.fa.fai").write_text("".join(fai_lines), encoding="utf-8")

(workdir / "intervals.interval_list").write_text(
    "@HD\tVN:1.6\tSO:coordinate\n"
    "@SQ\tSN:chr1\tLN:2000\n"
    "@SQ\tSN:chr2\tLN:1000\n"
    "chr1\t100\t220\t+\tiv1\n"
    "chr2\t10\t120\t+\tiv2\n",
    encoding="utf-8",
)

def mutate(s: str, mods: dict[int, str]) -> str:
    arr = list(s)
    for k, v in mods.items():
        arr[k] = v
    return "".join(arr)

lines = [
    "@HD\tVN:1.6\tSO:coordinate",
    "@SQ\tSN:chr1\tLN:2000",
    "@SQ\tSN:chr2\tLN:1000",
    "@RG\tID:RG1\tSM:SM1\tLB:LB1\tPU:PU1",
    "@RG\tID:RG2\tSM:SM1\tLB:LB1\tPU:PU2",
]

recs = []
# Pair 1 (F1R2): overlaps across chr1[100..160], tests homopolymers, 1I, 2D, SNPs (mate disagrees, both disagree, 3-way)
# chr1 1-based pos 101..115 (15M), 1I ('G'), 116..125 (10M), 2D (126..127), 128..147 (20M) -> qlen = 46
r1_seq = mutate(chr1[100:115], {4: "A", 10: "G"}) + "G" + chr1[115:125] + mutate(chr1[127:147], {3: "T"})
r1_qual = "I" * 10 + "!" + "I" * (len(r1_seq) - 11)  # pos 111 (off 10) has BQ=0 (< 20)
recs.append((0, 101, f"p1\t99\tchr1\t101\t60\t15M1I10M2D20M\t=\t108\t46\t{r1_seq}\t{r1_qual}\tRG:Z:RG1\tNM:i:6"))

# Mate R2 covers chr1[108..148] (1-based 109..148): shares 105? No, shares 111(BQ!), 131 (both disagree vs 3-way: r1 has T at chr1[130], put A in r2 for 3-way!), and mate agrees ref elsewhere; plus put shared error at chr1[120] (both have C)
r2_seq = mutate(chr1[108:148], {12: "C", 22: "A"})  # chr1[108+12]=chr1[120], chr1[108+22]=chr1[130]
# Also mutate r1 at chr1[120] -> offset 15+1+5 = 21 in r1? pos 101..115 is 15M (101..115 => idx 100..114), 1I, then 10M is pos 116..125 (idx 115..124), so idx 120 is offset 16 + (120-115) = 21. Let's make idx 120 mismatch in r1 too!
r1_seq_l = list(r1_seq)
r1_seq_l[21] = "C" if chr1[120] != "C" else "G"
if r1_seq_l[21] == chr1[120]:
    r1_seq_l[21] = "A"
r2_seq_l = list(r2_seq)
r2_seq_l[12] = r1_seq_l[21]  # both disagree with ref & agree with each other!
r1_seq = "".join(r1_seq_l)
r2_seq = "".join(r2_seq_l)
recs[0] = (0, 101, f"p1\t99\tchr1\t101\t60\t15M1I10M2D20M\t=\t109\t47\t{r1_seq}\t{r1_qual}\tRG:Z:RG1\tNM:i:7")
recs.append((0, 109, f"p1\t147\tchr1\t109\t60\t40M\t=\t101\t-47\t{r2_seq}\t{'I'*40}\tRG:Z:RG2\tNM:i:2"))

# Pair 2 (F2R1): 83 / 163
recs.append((0, 150, f"p2\t83\tchr1\t150\t50\t35M\t=\t170\t-55\t{chr1[149:184]}\t{'I'*35}\tRG:Z:RG2\tNM:i:0"))
recs.append((0, 170, f"p2\t163\tchr1\t170\t50\t35M\t=\t150\t55\t{chr1[169:204]}\t{'I'*35}\tRG:Z:RG1\tNM:i:0"))

# Pair 3 (both + -> Picard R1R2): 67 / 131
recs.append((0, 210, f"p3\t67\tchr1\t210\t45\t30M\t=\t220\t40\t{chr1[209:239]}\t{'I'*30}\tRG:Z:RG1\tNM:i:0"))
recs.append((0, 220, f"p3\t131\tchr1\t220\t45\t30M\t=\t210\t-40\t{chr1[219:249]}\t{'I'*30}\tRG:Z:RG1\tNM:i:0"))

# Pair 4 (both - -> Picard F1F2): 115 / 179
recs.append((0, 260, f"p4\t115\tchr1\t260\t45\t30M\t=\t270\t40\t{chr1[259:289]}\t{'I'*30}\tRG:Z:RG1\tNM:i:0"))
recs.append((0, 270, f"p4\t179\tchr1\t270\t45\t30M\t=\t260\t-40\t{chr1[269:299]}\t{'I'*30}\tRG:Z:RG1\tNM:i:0"))

# Unpaired, low MAPQ (skipped), duplicate (skipped), secondary (skipped), supplementary (skipped)
recs.append((0, 310, f"unp\t0\tchr1\t310\t60\t30M\t*\t0\t0\t{chr1[309:339]}\t{'I'*30}\tRG:Z:RG1\tNM:i:0"))
recs.append((0, 310, f"lowmq\t0\tchr1\t310\t5\t30M\t*\t0\t0\t{chr1[309:339]}\t{'I'*30}\tRG:Z:RG1\tNM:i:0"))
recs.append((0, 310, f"dup\t1024\tchr1\t310\t60\t30M\t*\t0\t0\t{chr1[309:339]}\t{'I'*30}\tRG:Z:RG1\tNM:i:0"))
recs.append((0, 310, f"sec\t256\tchr1\t310\t60\t30M\t*\t0\t0\t{chr1[309:339]}\t{'I'*30}\tRG:Z:RG1\tNM:i:0"))
recs.append((0, 310, f"sup\t2048\tchr1\t310\t60\t30M\t*\t0\t0\t{chr1[309:339]}\t{'I'*30}\tRG:Z:RG1\tNM:i:0"))

# Contig chr2 read with 2I insertion + 1D deletion + 1 mismatch
c2_seq = chr2[9:19] + "AT" + chr2[19:29] + mutate(chr2[30:40], {2: "T"})
recs.append((1, 10, f"c2_1\t0\tchr2\t10\t60\t10M2I10M1D10M\t*\t0\t0\t{c2_seq}\t{'I'*len(c2_seq)}\tRG:Z:RG2\tNM:i:4"))

recs.sort(key=lambda item: (item[0], item[1]))
(workdir / "input.sam").write_text("\n".join(lines + [r[2] for r in recs]) + "\n", encoding="utf-8")
PY

if command -v samtools >/dev/null 2>&1; then
  samtools view -bS "$workdir/input.sam" -o "$workdir/input.bam"
  samtools index "$workdir/input.bam"
  samtools dict "$workdir/reference.fa" -o "$workdir/reference.dict"
else
  "${conda_runner[@]}" run -p "$conda_prefix" samtools view -bS "$workdir/input.sam" -o "$workdir/input.bam"
  "${conda_runner[@]}" run -p "$conda_prefix" samtools index "$workdir/input.bam"
  "${conda_runner[@]}" run -p "$conda_prefix" samtools dict "$workdir/reference.fa" -o "$workdir/reference.dict"
fi

# Case 1 & 2: All 26 default files (t=1 sequential and t=4 indexed parallel)
TURBO_PICARD_THREADS=1 run_turbo \
  CollectSamErrorMetrics \
  "I=$workdir/input.bam" \
  "O=$workdir/turbo_t1" \
  "R=$workdir/reference.fa" \
  QUIET=true

TURBO_PICARD_THREADS=4 run_turbo \
  CollectSamErrorMetrics \
  "I=$workdir/input.bam" \
  "O=$workdir/turbo_t4" \
  "R=$workdir/reference.fa" \
  QUIET=true

run_picard \
  CollectSamErrorMetrics \
  "I=$workdir/input.bam" \
  "O=$workdir/picard_def" \
  "R=$workdir/reference.fa" \
  QUIET=true

# Case 3: INTERVALS + MAX_LOCI + custom ERROR_METRICS=null + FILE_EXTENSION=.txt
TURBO_PICARD_THREADS=4 run_turbo \
  CollectSamErrorMetrics \
  "I=$workdir/input.bam" \
  "O=$workdir/turbo_iv" \
  "R=$workdir/reference.fa" \
  "INTERVALS=$workdir/intervals.interval_list" \
  MAX_LOCI=25 \
  PRIOR_Q=25 \
  ERROR_METRICS=null \
  ERROR_METRICS=ERROR \
  ERROR_METRICS=ERROR:BINNED_HOMOPOLYMER \
  ERROR_METRICS=INDEL_ERROR \
  FILE_EXTENSION=.txt \
  QUIET=true

run_picard \
  CollectSamErrorMetrics \
  "I=$workdir/input.bam" \
  "O=$workdir/picard_iv" \
  "R=$workdir/reference.fa" \
  "INTERVALS=$workdir/intervals.interval_list" \
  MAX_LOCI=25 \
  PRIOR_Q=25 \
  ERROR_METRICS=null \
  ERROR_METRICS=ERROR \
  ERROR_METRICS=ERROR:BINNED_HOMOPOLYMER \
  ERROR_METRICS=INDEL_ERROR \
  FILE_EXTENSION=.txt \
  QUIET=true

python3 - "$workdir" <<'PY'
from pathlib import Path
import sys

workdir = Path(sys.argv[1])


def metric_body(path: Path) -> list[str]:
    lines = [l.rstrip("\n") for l in path.read_text(encoding="utf-8").splitlines()]
    out = []
    started = False
    for line in lines:
        if line.startswith("## METRICS CLASS\t"):
            started = True
        if started and line:
            out.append(line)
    return out


picard_files = sorted(p.name[len("picard_def."):] for p in workdir.glob("picard_def.*"))
if len(picard_files) != 26:
    raise SystemExit(f"expected 26 default Picard files, found {len(picard_files)}: {picard_files}")

for prefix in ("turbo_t1", "turbo_t4"):
    turbo_files = sorted(p.name[len(prefix) + 1:] for p in workdir.glob(f"{prefix}.*"))
    if turbo_files != picard_files:
        raise SystemExit(f"{prefix} file set mismatch:\nturbo={turbo_files}\npicard={picard_files}")
    mismatches = []
    for suffix in picard_files:
        tbody = metric_body(workdir / f"{prefix}.{suffix}")
        pbody = metric_body(workdir / f"picard_def.{suffix}")
        if tbody != pbody:
            import difflib
            diff = "\n".join(difflib.unified_diff(pbody, tbody, fromfile="picard", tofile="turbo", lineterm=""))
            mismatches.append(f"=== {prefix}.{suffix} ===\n{diff}")
    if mismatches:
        raise SystemExit("\n".join(mismatches[:5]))

iv_picard_files = sorted(p.name[len("picard_iv."):] for p in workdir.glob("picard_iv.*"))
iv_turbo_files = sorted(p.name[len("turbo_iv."):] for p in workdir.glob("turbo_iv.*"))
if iv_turbo_files != iv_picard_files or len(iv_picard_files) != 3:
    raise SystemExit(f"intervals/custom file mismatch: {iv_turbo_files} vs {iv_picard_files}")
for suffix in iv_picard_files:
    tbody = metric_body(workdir / f"turbo_iv.{suffix}")
    pbody = metric_body(workdir / f"picard_iv.{suffix}")
    if tbody != pbody:
        raise SystemExit(
            f"turbo_iv.{suffix} mismatch:\n--- TURBO ---\n"
            + "\n".join(tbody)
            + "\n--- PICARD ---\n"
            + "\n".join(pbody)
        )

print("CollectSamErrorMetrics matches Picard across all 26 default metrics, intervals, max_loci, and extensions")
PY
