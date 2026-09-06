#!/usr/bin/env bash
# Publish only the three independently tested BEDTools/HTSJDK contributions.
# Uses the user's GitHub CLI login; no credentials are accepted by this script.
set -euo pipefail
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"

SOURCE_COMMIT=f2df5d201041122f08a86d14d621555c5e580b22
SOURCE_SHA256=50eb1dd969bdedaa3998c815e3080af56fb30a403172592dcb3d92e51d215a6a
WORK=$(mktemp -d "${HOME}/upstream-prs.XXXXXX")
trap 'code=$?; if [ "$code" -ne 0 ]; then printf "\nPublishing stopped (exit %s). Files/results retained in:\n%s\n" "$code" "$WORK" >&2; fi' EXIT
printf 'Publishing the tested BEDTools fix and two HTSJDK fixes.\nResults directory: %s\n\n' "$WORK"

install_with_brew() {
    if ! command -v brew >/dev/null 2>&1; then
        printf 'Missing %s and Homebrew is not available. No GitHub changes have been made.\n' "$1" >&2
        exit 2
    fi
    printf 'Installing missing dependency with Homebrew: %s\n' "$1"
    brew install "$1"
}
command -v curl >/dev/null 2>&1 || { echo 'curl is required.' >&2; exit 2; }
git --version >/dev/null 2>&1 || { echo 'Git is required; finish installing the Apple command-line tools and rerun.' >&2; exit 2; }
command -v gh >/dev/null 2>&1 || install_with_brew gh

PYTHON=''
find_python() {
    local candidate
    for candidate in python3 python3.14 python3.13 python3.12 python3.11 python3.10; do
        if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c 'import sys; sys.exit(sys.version_info < (3,10))' >/dev/null 2>&1; then
            PYTHON=$(command -v "$candidate")
            return 0
        fi
    done
    return 1
}
if ! find_python; then
    install_with_brew python
    find_python || { echo 'Python 3.10 or newer was not found after installation.' >&2; exit 2; }
fi

if ! gh auth status --hostname github.com >/dev/null 2>&1; then
    printf '\nApprove the GitHub CLI sign-in in your browser as dnncha.\n'
    gh auth login --hostname github.com --git-protocol https --web
fi
LOGIN=$(gh api --hostname github.com user --jq .login)
if [ "$LOGIN" != dnncha ]; then
    printf 'Expected GitHub user dnncha, but the active login is %s. No GitHub changes made.\n' "$LOGIN" >&2
    exit 2
fi

curl --fail --location --proto '=https' --tlsv1.2 --silent --show-error \
    "https://raw.githubusercontent.com/dnncha/turbo-picard/${SOURCE_COMMIT}/research/upstream-correctness/submit_prs.py" \
    -o "$WORK/submit_prs.py"
"$PYTHON" - "$WORK/submit_prs.py" "$SOURCE_SHA256" <<'PY'
import hashlib, pathlib, sys
actual = hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest()
if actual != sys.argv[2]:
    raise SystemExit('Downloaded publisher checksum mismatch; refusing to execute.')
print('Verified the pinned publisher checksum.')
PY
"$PYTHON" "$WORK/submit_prs.py" --submit --results "$WORK/submission-results.json"
"$PYTHON" - "$WORK/submission-results.json" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
results = json.loads(path.read_text())
expected = {'bedtools-union', 'htsjdk-cigar', 'htsjdk-reference'}
if set(results) != expected:
    raise SystemExit('Not all three PR results were returned. Inspect ' + str(path))
print('\nGitHub-confirmed pull requests:')
for case, result in results.items():
    print(f"{case}: {result['url']} ({result['state']})")
print('\nSaved: ' + str(path))
PY
