#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo deny --locked check
uvx ruff@0.17.3 check .
uvx ruff@0.17.3 format --check .
uvx --with pytest --with types-PyYAML mypy@2.4.1
