#!/usr/bin/env bash
set -uo pipefail
REF="${REF:?git ref to build}"
THREADS="${THREADS:-$(nproc)}"
DATASETS="${DATASETS:-4x,15x,30x}"
PICARD_DATASETS="${PICARD_DATASETS:-4x}"
TASKS="${TASKS:-wgs,multi,markdup-coord,markdup-qgroup}"
ONLY_TOOLS="${ONLY_TOOLS:-}"
ROOT=/content/duel
DATA=$ROOT/data
OUT=$ROOT/out
mkdir -p "$DATA" "$OUT"
exec > >(tee -a "$OUT/vm.log") 2>&1
stamp() { echo "[$(date -u +%FT%TZ)] $*"; }

S3=https://1000genomes.s3.amazonaws.com
CRAM_URL=$S3/1000G_2504_high_coverage/data/ERR3242389/HG02675.final.cram
REF_URL=$S3/technical/reference/GRCh38_reference_genome/GRCh38_full_analysis_set_plus_decoy_hla.fa
MM=$ROOT/mm
BIN=$MM/envs/comp/bin

stamp "tools"
if [ ! -x "$BIN/riker" ]; then
  apt-get update -qq >/dev/null
  apt-get install -y -qq clang libclang-dev cmake libbz2-dev liblzma-dev libcurl4-openssl-dev libssl-dev zlib1g-dev pkg-config >/dev/null
  pkg=$(apt-cache search '^linux-tools-[0-9.-]*-generic$' | sort -V | tail -1 | cut -d' ' -f1)
  apt-get install -y -qq "$pkg" >/dev/null && ln -sf "$(ls /usr/lib/linux-tools/*/perf | tail -1)" /usr/local/bin/perf-real
  (cd $ROOT && curl -Ls https://micro.mamba.pm/api/micromamba/linux-64/latest | tar -xj bin/micromamba)
  $ROOT/bin/micromamba create -y -q -r "$MM" -n comp -c conda-forge -c bioconda riker dupblaster fastdup samtools openjdk=21 >/dev/null
fi
[ -f $ROOT/picard.jar ] || curl -sSfL -o $ROOT/picard.jar https://github.com/broadinstitute/picard/releases/download/3.5.0/picard.jar
printf '#!/bin/sh\nexec %s/java -Xmx24g -jar %s/picard.jar "$@"\n' "$BIN" "$ROOT" > /usr/local/bin/picard-jar && chmod +x /usr/local/bin/picard-jar

stamp "build $REF"
[ -x "$HOME/.cargo/bin/cargo" ] || curl -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal
. "$HOME/.cargo/env"
[ -d $ROOT/src ] || git clone -q https://github.com/dnncha/turbo-picard $ROOT/src
git -C $ROOT/src fetch -q origin "$REF" && git -C $ROOT/src checkout -q --detach FETCH_HEAD
SHA=$(git -C $ROOT/src rev-parse HEAD)
(cd $ROOT/src && cargo build --release --locked -q -p turbo-picard-cli --bin turbo-picard) || { stamp "build failed"; exit 2; }
TP=$ROOT/src/target/release/turbo-picard

stamp "data"
[ -s $DATA/ref.fa ] || { curl -sSfL -o $DATA/ref.fa "$REF_URL" && curl -sSfL -o $DATA/ref.fa.fai "$REF_URL.fai"; }
[ -s $DATA/ref.dict ] || $BIN/samtools dict $DATA/ref.fa -o $DATA/ref.dict
if [ ! -s $DATA/30x.bam ]; then
  curl -sSfL -o $DATA/in.cram "$CRAM_URL" && curl -sSfL -o $DATA/in.cram.crai "$CRAM_URL.crai"
  $BIN/samtools view -@ "$THREADS" -b -T $DATA/ref.fa -o $DATA/30x.bam $DATA/in.cram && $BIN/samtools index -@ "$THREADS" $DATA/30x.bam && rm -f $DATA/in.cram $DATA/in.cram.crai
fi
depth=$($BIN/samtools idxstats $DATA/30x.bam | awk '{m+=$3} END {printf "%.2f", m*151/3.1e9}')
echo "full depth ~${depth}x" > $OUT/depth.txt
for x in 4 15; do
  if [ ! -s $DATA/${x}x.bam ]; then
    frac=$(awk -v x=$x -v d="$depth" 'BEGIN {printf "%.4f", x/d}')
    $BIN/samtools view -@ "$THREADS" -b -s "42${frac#0}" -o $DATA/${x}x.bam $DATA/30x.bam && $BIN/samtools index $DATA/${x}x.bam
  fi
  if [ ! -s $DATA/${x}x.qg.bam ]; then
    TURBO_PICARD_REQUIRE_NATIVE=1 $TP SortSam I=$DATA/${x}x.bam O=$DATA/${x}x.qg.bam SORT_ORDER=queryname TMP_DIR=$DATA VALIDATION_STRINGENCY=SILENT
  fi
done
df -h /content > $OUT/disk.txt

specs=()
IFS=, read -r -a names <<< "$DATASETS"
for n in "${names[@]}"; do
  picard=false; [[ ",$PICARD_DATASETS," == *",$n,"* ]] && picard=true
  spec="name=$n,coord=$DATA/$n.bam,picard=$picard"
  [ -s $DATA/$n.qg.bam ] && spec="$spec,qgroup=$DATA/$n.qg.bam"
  if [ "$n" = 30x ]; then spec="$spec,repeats=2,warmup=false"; fi
  specs+=(--dataset "$spec")
done
extra=()
[ -n "$ONLY_TOOLS" ] && extra+=(--only-tools "$ONLY_TOOLS")
stamp "duel sha=$SHA threads=$THREADS"
PATH=$BIN:$PATH python3 $ROOT/src/tools/duel.py run "${specs[@]}" --reference $DATA/ref.fa --threads "$THREADS" \
  --workdir $ROOT/work --results $OUT/results.jsonl --tasks "$TASKS" --git-sha "$SHA" --repo $ROOT/src \
  --turbo "$TP" --picard /usr/local/bin/picard-jar --riker $BIN/riker --dupblaster $BIN/dupblaster \
  --fastdup $BIN/fastdup --samtools $BIN/samtools "${extra[@]}"
stamp "DONE"
touch $OUT/DONE
