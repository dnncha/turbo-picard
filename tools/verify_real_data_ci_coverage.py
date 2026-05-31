#!/usr/bin/env python3
"""Verify CI covers real-data evidence helper scripts."""

from __future__ import annotations

import pathlib
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
CI = ROOT / ".github" / "workflows" / "ci.yml"

REQUIRED_SNIPPETS = [
    "python3 -m unittest tools/test_compare_real_data.py",
    "python3 -m unittest tools/test_update_real_data_manifest.py",
    "python3 -m unittest tools/test_verify_real_data_evidence.py",
    "tools/compare_real_data.py",
    "tools/update_real_data_manifest.py",
    "tools/verify_real_data_evidence.py",
    "tools/test_compare_real_data.py",
    "tools/test_update_real_data_manifest.py",
    "tools/test_verify_real_data_evidence.py",
    "python3 tools/verify_real_data_evidence.py",
]


def validate_ci_coverage(ci_text: str) -> list[str]:
    return [
        f"CI missing real-data evidence coverage: {snippet}"
        for snippet in REQUIRED_SNIPPETS
        if snippet not in ci_text
    ]


def main() -> int:
    errors = validate_ci_coverage(CI.read_text(encoding="utf-8"))
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
