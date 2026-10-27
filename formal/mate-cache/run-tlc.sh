#!/usr/bin/env bash
# Run TLC on the mate-cache model. Usage: run-tlc.sh [config] [spec]
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
config=${1:-"$here/MateCache.cfg"}
spec=${2:-"$here/MateCache.tla"}

jar=${TLA2TOOLS_JAR:-}
if [[ -z $jar ]]; then
    for candidate in \
        "$HOME/.local/share/tlaplus/1.7.4/tla2tools.jar" \
        "$HOME/.cache/supertime-formal/tla2tools-1.7.4.jar"; do
        if [[ -f $candidate ]]; then jar=$candidate; break; fi
    done
fi
if [[ -z $jar || ! -f $jar ]]; then
    echo "error: tla2tools.jar not found. Set TLA2TOOLS_JAR or install TLA+ 1.7.4." >&2
    exit 2
fi

java=""
for candidate in \
    ${JAVA_HOME:+"$JAVA_HOME/bin/java"} \
    "$(command -v java || true)" \
    /opt/homebrew/opt/openjdk@25/bin/java \
    /opt/homebrew/opt/openjdk@21/bin/java \
    /opt/homebrew/opt/openjdk@17/bin/java; do
    if [[ -n $candidate && -x $candidate ]] && "$candidate" -version >/dev/null 2>&1; then
        java=$candidate
        break
    fi
done
if [[ -z $java ]]; then
    echo "error: no Java runtime found. Install a JDK 17 or newer." >&2
    exit 2
fi

metadir=$(mktemp -d)
trap 'rm -rf "$metadir"' EXIT
exec "$java" -jar "$jar" -cleanup -metadir "$metadir" -workers auto \
    -config "$config" "$spec"
