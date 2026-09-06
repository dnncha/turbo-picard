#!/usr/bin/env bash
# Explicit one-time extraction; never merges the research branch into Turbo Picard.
set -euo pipefail
command -v gh >/dev/null || { echo 'GitHub CLI (gh) is required.' >&2; exit 2; }
gh auth status >/dev/null
SOURCE_REF=${HMMFORGE_SOURCE_REF:-research/hmmforge-prototype}
DEST=${1:-hmmforge-standalone}
[[ ! -e "$DEST" ]] || { echo "Destination already exists: $DEST" >&2; exit 2; }
# Refuse existing repositories. Any ambiguous API failure is handled by repo-create,
# which must succeed before a push can happen. Never push into a pre-existing repo.
if gh repo view dnncha/hmmforge --json name >/dev/null 2>&1; then
  echo 'dnncha/hmmforge already exists; refusing to overwrite or modify it.' >&2; exit 2
fi
mkdir "$DEST"
TARBALL=$(mktemp)
trap 'rm -f "$TARBALL"' EXIT
gh api "repos/dnncha/turbo-picard/tarball/$SOURCE_REF" > "$TARBALL"
# Extract only the independent package, never Turbo Picard code or history.
python3 - "$TARBALL" "$DEST" <<'PY'
import pathlib, sys, tarfile
out=pathlib.Path(sys.argv[2]).resolve()
with tarfile.open(sys.argv[1]) as archive:
    for item in archive:
        parts=pathlib.PurePosixPath(item.name).parts
        if len(parts)<4 or parts[1:3] != ('research','hmmforge'):
            continue
        rel=pathlib.Path(*parts[3:]); target=out/rel
        if not target.resolve().is_relative_to(out) or item.issym() or item.islnk():
            raise SystemExit('Unsafe archive entry')
        if item.isfile():
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.extractfile(item).read())
if not (out/'pyproject.toml').is_file():
    raise SystemExit('Independent package not found in source ref')
PY
(
  cd "$DEST"
  git init -b main
  git add .
  git commit -m 'Import standalone HMMForge research package and reproducible evidence'
  gh repo create dnncha/hmmforge --private --description 'Experimental HMMER-backed protein annotation and reproducible performance studies' --source=. --remote=origin --push
)
echo 'Created private dnncha/hmmforge. No PyPI publication or production announcement.'
