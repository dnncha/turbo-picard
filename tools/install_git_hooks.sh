#!/usr/bin/env bash
set -eu

root=$(git rev-parse --show-toplevel)
hooks="$root/.githooks"

if [ ! -d "$hooks" ]; then
  echo "install_git_hooks: $hooks is missing" >&2
  exit 1
fi

chmod +x "$hooks"/*
git -C "$root" config core.hooksPath .githooks
echo "install_git_hooks: core.hooksPath=.githooks"
for hook in "$hooks"/*; do
  echo "install_git_hooks: $(basename "$hook")"
done
