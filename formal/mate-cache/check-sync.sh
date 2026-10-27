#!/usr/bin/env bash
# Report whether the covered file changed since the skill's sync commit.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
skill="$root/.opencode/skills/verify-mate-cache/SKILL.md"
covered="crates/turbo-picard-markdup/src/external_mates.rs"

sync=$(grep -oE 'Sync commit: `[0-9a-f]+`' "$skill" | grep -oE '[0-9a-f]{7,40}' | head -1)
if [[ -z $sync ]]; then
    echo "error: no sync commit in $skill" >&2
    exit 2
fi
if ! git -C "$root" rev-parse --verify "$sync^{commit}" >/dev/null 2>&1; then
    echo "error: sync commit $sync is not in this repository" >&2
    exit 2
fi

if git -C "$root" diff --quiet "$sync" -- "$covered"; then
    echo "in sync: $covered unchanged since $sync"
    exit 0
fi

echo "review needed: $covered changed since $sync"
git -C "$root" log --oneline "$sync"..HEAD -- "$covered"
echo "Update the model if the change affects pairing, eviction, or the filter."
echo "Then move the sync commit in $skill forward."
exit 1
