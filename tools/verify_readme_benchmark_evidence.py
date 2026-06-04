#!/usr/bin/env python3
"""Verify README benchmark claims against rendered benchmark-data.json."""

from __future__ import annotations

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOOL_DIR = Path(__file__).resolve().parent
if str(TOOL_DIR) not in sys.path:
    sys.path.insert(0, str(TOOL_DIR))

from verify_benchmark_suite_coverage import BENCHMARK_EXEMPTIONS  # noqa: E402

README = ROOT / "README.md"
BENCHMARK_DATA = ROOT / "docs" / "site" / "assets" / "benchmark-data.json"


def format_speedup(value: float) -> str:
    return f"{value:.2f}x"


def validate_readme_benchmark_evidence(readme: str, data: dict) -> list[str]:
    errors = []
    summary = data["summary"]
    parity_count = data["parity"].split()[0]
    source = data.get("source")
    date = data.get("date")
    source_artifact = data.get("source_artifact")

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

    for value, message in [
        (date, "missing README benchmark date"),
        (source, "missing README benchmark source command"),
        (source_artifact, "missing README raw benchmark artifact path"),
    ]:
        if not isinstance(value, str) or not value or f"`{value}`" not in readme:
            errors.append(message)

    if "python3 tools/verify_benchmark_log_evidence.py" not in readme:
        errors.append("missing README benchmark-log evidence verifier command")
    if "python3 tools/verify_benchmark_suite_coverage.py" not in readme:
        errors.append("missing README benchmark-suite coverage verifier command")
    if "python3 tools/verify_benchmark_thresholds.py" not in readme:
        errors.append("missing README benchmark-threshold verifier command")
    if "python3 tools/verify_real_data_evidence.py --release-ready" not in readme:
        errors.append("missing README release-ready real-data verifier command")
    if "benchmark exceptions" not in readme:
        errors.append("missing README benchmark exception disclosure")
    for needle, message in [
        (
            "https://turbo-picard.readthedocs.io/en/latest/adoption.html",
            "missing README adoption guide link",
        ),
        (
            "https://turbo-picard.readthedocs.io/en/latest/benchmarks.html",
            "missing README benchmark documentation link",
        ),
        (
            "https://turbo-picard.readthedocs.io/en/latest/citation.html",
            "missing README citation documentation link",
        ),
        (
            "CITATION.cff",
            "missing README software citation pointer",
        ),
        (
            "SHA-256",
            "missing README pinned input SHA-256 guidance",
        ),
    ]:
        if needle not in readme:
            errors.append(message)
    for command in sorted(BENCHMARK_EXEMPTIONS):
        if command not in readme:
            errors.append(f"missing README benchmark exception: {command}")

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
