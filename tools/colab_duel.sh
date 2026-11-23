#!/usr/bin/env bash
set -euo pipefail
usage() { echo "usage: $0 launch|sync|watch|report|stop <session> [KEY=VALUE env for tools/duel_vm.sh...]" >&2; exit 64; }
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
    env_json=$(python3 -c 'import json, sys; print(json.dumps(dict(a.split("=", 1) for a in sys.argv[1:])))' "$@")
    colab --config "$cfg" exec -s "$session" <<PY
import json, os, pathlib, subprocess
pathlib.Path("/content/duel/out/DONE").unlink(missing_ok=True)
subprocess.run(["pkill", "-f", "duel_vm.sh"], check=False)
env = {**os.environ, **json.loads('$env_json')}
with open("/content/duel_vm.out", "ab") as log:
    proc = subprocess.Popen(["bash", "/content/duel_vm.sh"], env=env, stdout=log, stderr=log, stdin=subprocess.DEVNULL, start_new_session=True)
print("started", proc.pid)
PY
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
  watch)
    while :; do
      "$0" sync "$session" > /dev/null 2>&1 || true
      [ -e "$local_dir/out/DONE" ] && exit 0
      sleep "${WATCH_SECONDS:-900}"
    done
    ;;
  stop) colab --config "$cfg" stop -s "$session" ;;
  *) usage ;;
esac
