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
[ -s $DATA/in.cram.crai ] || { curl -sSfL -o $DATA/in.cram "$CRAM_URL" && curl -sSfL -o $DATA/in.cram.crai "$CRAM_URL.crai"; }
depth=$($BIN/samtools idxstats $DATA/in.cram | awk '{m+=$3} END {printf "%.2f", m*151/3.1e9}')
echo "full depth ~${depth}x" > $OUT/depth.txt

prepare() {
  local n=$1 x=${1%x}
  if [ ! -s $DATA/$n.bam ]; then
    make_bam "$n" "$x" || return 1
  fi
  if [ "$n" != 30x ] && [ ! -s $DATA/$n.qg.bam ]; then
    TURBO_PICARD_REQUIRE_NATIVE=1 $TP SortSam I=$DATA/$n.bam O=$DATA/$n.qg.bam SORT_ORDER=queryname TMP_DIR=$DATA VALIDATION_STRINGENCY=SILENT
  fi
}

make_bam() {
  local n=$1 x=$2
  [ "$n" = 4x ] && [ ! -s $DATA/15x.bam ] && make_bam 15x 15 && $BIN/samtools index -@ "$THREADS" $DATA/15x.bam
  if [ "$n" = 30x ]; then
    $BIN/samtools view -@ "$THREADS" -b -T $DATA/ref.fa -o $DATA/$n.bam $DATA/in.cram
  elif [ "$n" = 4x ] && [ -s $DATA/15x.bam ]; then
    $BIN/samtools view -@ "$THREADS" -b -s "7$(awk 'BEGIN {printf "%.4f", 4/15}' | sed 's/^0//')" -o $DATA/$n.bam $DATA/15x.bam
  else
    local frac; frac=$(awk -v x="$x" -v d="$depth" 'BEGIN {printf "%.4f", x/d}')
    $BIN/samtools view -@ "$THREADS" -b -T $DATA/ref.fa -s "42${frac#0}" -o $DATA/$n.bam $DATA/in.cram
  fi
  $BIN/samtools index -@ "$THREADS" $DATA/$n.bam
  [ "$n" = 30x ] || check_depth "$n" "$x"
}

check_depth() {
  local n=$1 x=$2 got
  got=$($BIN/samtools idxstats $DATA/$n.bam | awk '{m+=$3} END {printf "%.2f", m*151/3.1e9}')
  if ! awk -v g="$got" -v x="$x" 'BEGIN {exit !(g >= 0.75 * x && g <= 1.25 * x)}'; then
    echo "prepare $n: expected about ${x}x but the subsample is ${got}x; a dataset that is not the depth it is named for makes every row of its duel wrong" >&2
    rm -f $DATA/$n.bam $DATA/$n.bam.bai
    return 1
  fi
  echo "prepare $n: ${got}x (target ${x}x)"
}

RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)-$(head -c3 /dev/urandom | od -An -tx1 | tr -d ' \n')}"
echo "$RUN_ID" > $OUT/run_id
extra=()
[ -n "$ONLY_TOOLS" ] && extra+=(--only-tools "$ONLY_TOOLS")
IFS=, read -r -a names <<< "$DATASETS"
for n in "${names[@]}"; do
  stamp "prepare $n"
  prepare "$n" || { stamp "prepare $n failed"; continue; }
  df -h /content | tail -1
  picard=false; [[ ",$PICARD_DATASETS," == *",$n,"* ]] && picard=true
  spec="name=$n,coord=$DATA/$n.bam,picard=$picard"
  [ -s $DATA/$n.qg.bam ] && spec="$spec,qgroup=$DATA/$n.qg.bam"
  [ "$n" = 30x ] && spec="$spec,repeats=2,warmup=false"
  stamp "duel $n sha=$SHA threads=$THREADS run=$RUN_ID"
  PATH=$BIN:$PATH python3 $ROOT/src/tools/duel.py run --dataset "$spec" --reference $DATA/ref.fa --threads "$THREADS" \
    --workdir $ROOT/work --results $OUT/results.jsonl --tasks "$TASKS" --git-sha "$SHA" --repo $ROOT/src --run-id "$RUN_ID" \
    --turbo "$TP" --picard /usr/local/bin/picard-jar --riker $BIN/riker --dupblaster $BIN/dupblaster \
    --fastdup $BIN/fastdup --samtools $BIN/samtools "${extra[@]}"
done
stamp "DONE"
touch $OUT/DONE
