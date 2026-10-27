#!/usr/bin/env bash
# Retroactively trace the last few milliseconds of a turbo-picard process.
#
# This is the tail-latency companion to the aggregate suite profiler: attach to
# a running process, trigger a slow record or batch, and read the call stack
# that led there. See docs/performance.rst ("Retroactive tail traces").
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: tools/retro-trace.sh <pid> [output.ftf] [magic-trace arguments...]

Capture a ring buffer of control flow for the process named by <pid>, then open
the trace in https://ui.perfetto.dev.

Examples:
  tools/retro-trace.sh 12345
  tools/retro-trace.sh 12345 /tmp/markdup-tail.ftf -symbol 'camlMark_duplicates__flush'
  tools/retro-trace.sh 12345 tail.ftf -multi-thread

The process must run on x86-64 Linux with Intel Processor Trace. magic-trace
must be on PATH: https://github.com/janestreet/magic-trace
EOF
}

if [[ $# -lt 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    [[ $# -lt 1 ]] && exit 2
    exit 0
fi

if ! command -v magic-trace >/dev/null 2>&1; then
    echo "error: magic-trace is not installed or not on PATH" >&2
    echo "       install: opam install magic-trace   (see docs/performance.rst)" >&2
    exit 2
fi

pid=$1
shift
output=${1:-turbo-picard-tail.ftf}
if [[ $# -gt 0 ]]; then
    shift
fi

if [[ ! -d /proc/$pid ]]; then
    echo "error: process $pid is not visible from this host" >&2
    exit 2
fi

if [[ -r /sys/bus/event_source/devices/intel_pt/caps/psb_cyc ]]; then
    if [[ "$(cat /sys/bus/event_source/devices/intel_pt/caps/psb_cyc)" != "1" ]]; then
        echo "warning: Intel Processor Trace does not report precise timing on this host" >&2
    fi
else
    echo "warning: Intel Processor Trace is not exposed on this host" >&2
fi

exec magic-trace attach -pid "$pid" -output "$output" "$@"
