#!/usr/bin/env bash
set -euo pipefail
usage() { echo "usage: $0 launch|sync|report|stop <session> [KEY=VALUE env for tools/duel_vm.sh...]" >&2; exit 64; }
[ $# -ge 2 ] || usage
cmd=$1 session=$2; shift 2
repo="$(cd "$(dirname "$0")/.." && pwd)"
cfg="${COLAB_STATE:-${TMPDIR:-/tmp}/turbo-picard-colab-state.json}"
local_dir="$repo/benchmarks/duel/runs/$session"
case "$cmd" in
  launch)
    gpu=(); [ -n "${COLAB_GPU:-}" ] && gpu=(--gpu "$COLAB_GPU")
    colab --config "$cfg" sessions | grep -q "\[$session\]" || colab --config "$cfg" new -s "$session" --high-mem "${gpu[@]}"
    colab --config "$cfg" upload -s "$session" "$repo/tools/duel_vm.sh" /content/duel_vm.sh
    envs=""; for kv in "$@"; do envs+=" $(printf %q "$kv")"; done
    colab --config "$cfg" ssh -s "$session" -- "rm -f /content/duel/out/DONE; env$envs setsid nohup bash /content/duel_vm.sh > /content/duel_vm.out 2>&1 < /dev/null & disown; echo started"
    ;;
  sync)
    mkdir -p "$local_dir"
    colab --config "$cfg" ssh -s "$session" -- 'tar cz -C /content/duel out' > "$local_dir/out.tgz.part"
    mv "$local_dir/out.tgz.part" "$local_dir/out.tgz" && tar xzf "$local_dir/out.tgz" -C "$local_dir" && rm "$local_dir/out.tgz"
    tail -5 "$local_dir/out/vm.log"
    ;;
  report)
    python3 "$repo/tools/duel.py" report --ledger "$repo/benchmarks/duel/ledger.jsonl" --add "$local_dir/out/results.jsonl" \
      --scoreboard "$repo/benchmarks/duel/SCOREBOARD.md" --losses "$repo/benchmarks/duel/losses.json"
    ;;
  stop) colab --config "$cfg" stop -s "$session" ;;
  *) usage ;;
esac
