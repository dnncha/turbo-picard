#!/usr/bin/env python3
"""Verify README benchmark claims against rendered benchmark-data.json."""

from __future__ import annotations

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "README.md"
BENCHMARK_DATA = ROOT / "docs" / "site" / "assets" / "benchmark-data.json"


def format_speedup(value: float) -> str:
    return f"{value:.2f}x"


def validate_readme_benchmark_evidence(readme: str, data: dict) -> list[str]:
    errors = []
    summary = data["summary"]
    parity_count = data["parity"].split()[0]

    expected_claims = [
        (f"`{parity_count}`", f"missing README parity claim: `{parity_count}`"),
        (
            f"`{format_speedup(summary['top_speedup'])}` top speedup: `{summary['top_command']}`",
            "missing README top-speedup claim",
        ),
        (
            f"`{format_speedup(summary['floor_speedup'])}` floor speedup: `{summary['floor_command']}`",
            "missing README floor-speedup claim",
        ),
        (
            f"`{format_speedup(summary['median_speedup'])}` median speedup",
            "missing README median-speedup claim",
        ),
        (
            f"`{format_speedup(summary['geometric_mean_speedup'])}` geometric mean speedup",
            "missing README geometric-mean-speedup claim",
        ),
    ]
    for needle, message in expected_claims:
        if needle not in readme:
            errors.append(message)

    if "python3 tools/verify_benchmark_log_evidence.py" not in readme:
        errors.append("missing README benchmark-log evidence verifier command")

    for row in data["benchmarks"]:
        command = row["command"]
        speedup = format_speedup(row["speedup"])
        parity = row["parity"]
        table_row = f"| {command} | {speedup} | {parity} |"
        if table_row not in readme:
            errors.append(f"missing README benchmark table row: {command} {speedup} {parity}")

    return errors


def main() -> int:
    readme = README.read_text(encoding="utf-8")
    data = json.loads(BENCHMARK_DATA.read_text(encoding="utf-8"))
    errors = validate_readme_benchmark_evidence(readme, data)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
