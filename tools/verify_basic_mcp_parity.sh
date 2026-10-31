#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

requests='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"parity","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"explain","arguments":{"command":"MarkDuplicates","args":["I=in.bam","O=out.bam","M=metrics.txt"]}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"run","arguments":{"command":"explain","args":[]}}}'

output="$(printf '%s\n' "$requests" | cargo run -q -p turbo-picard-cli --bin turbo-picard -- mcp)"

test "$(printf '%s\n' "$output" | wc -l | tr -d ' ')" = 4
printf '%s\n' "$output" | grep -q '"protocolVersion":"2025-06-18"'
for tool in capabilities explain trial doctor run; do
  printf '%s\n' "$output" | grep -q "\"name\":\"$tool\""
done
printf '%s\n' "$output" | grep -q '"command":"MarkDuplicates"'
printf '%s\n' "$output" | grep -q '"status":"partial-native"'
printf '%s\n' "$output" | grep -q '"fallback_ready":'
printf '%s\n' "$output" | grep -q 'is a report; call its own tool'

echo "mcp parity check passed"
