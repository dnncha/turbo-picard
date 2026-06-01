#!/usr/bin/env python3
"""Verify marketing-site benchmark claims against rendered benchmark-data.json."""

from __future__ import annotations

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOOL_DIR = Path(__file__).resolve().parent
if str(TOOL_DIR) not in sys.path:
    sys.path.insert(0, str(TOOL_DIR))

from verify_benchmark_suite_coverage import BENCHMARK_EXEMPTIONS  # noqa: E402

SITE = ROOT / "docs" / "site" / "index.html"
BENCHMARK_DATA = ROOT / "docs" / "site" / "assets" / "benchmark-data.json"


def format_speedup(value: float) -> str:
    return f"{value:.2f}x"


def validate_site_benchmark_evidence(site: str, data: dict) -> list[str]:
    errors = []
    summary = data["summary"]
    parity_count = data["parity"].split()[0]
    command_count = summary["command_count"]
    top_speedup = format_speedup(summary["top_speedup"])
    floor_speedup = format_speedup(summary["floor_speedup"])
    geometric_mean = format_speedup(summary["geometric_mean_speedup"])
    top_command = summary["top_command"]
    floor_command = summary["floor_command"]
    date = data.get("date")
    source = data.get("source")
    source_artifact = data.get("source_artifact")

    checks = [
        (parity_count, f"missing site parity claim: {parity_count}"),
        (f"{command_count} commands", "missing site command-count claim"),
        (top_speedup, f"missing site top-speedup claim: {top_speedup}"),
        (floor_speedup, f"missing site floor-speedup claim: {floor_speedup}"),
        (geometric_mean, f"missing site geometric-mean claim: {geometric_mean}"),
        (top_command, f"missing site top-command claim: {top_command}"),
        (floor_command, f"missing site floor-command claim: {floor_command}"),
        ("assets/benchmark-data.json", "missing site benchmark JSON link: assets/benchmark-data.json"),
        ("assets/bench-suite-output.txt", "missing site raw suite log link: assets/bench-suite-output.txt"),
        (
            "python3 tools/verify_benchmark_log_evidence.py",
            "missing site benchmark-log evidence verifier command",
        ),
        (
            "python3 tools/verify_benchmark_suite_coverage.py",
            "missing site benchmark-suite coverage verifier command",
        ),
        (
            "python3 tools/verify_benchmark_thresholds.py",
            "missing site benchmark-threshold verifier command",
        ),
        (
            "python3 tools/verify_real_data_evidence.py --release-ready",
            "missing site release-ready real-data verifier command",
        ),
        ("benchmark exceptions", "missing site benchmark exception disclosure"),
        ("#adopt", "missing site adoption section link"),
        ("../../CITATION.cff", "missing site CITATION.cff link"),
        ("input SHA-256", "missing site pinned input SHA-256 guidance"),
        ("archived-release citation", "missing site archived-release citation guidance"),
        ("12-command release portfolio", "missing site release-candidate portfolio guidance"),
    ]
    for needle, message in checks:
        if needle not in site:
            errors.append(message)
    for value, message in [
        (date, f"missing site benchmark date: {date}"),
        (source, f"missing site benchmark source command: {source}"),
        (source_artifact, f"missing site benchmark source artifact: {source_artifact}"),
    ]:
        if not isinstance(value, str) or not value or value not in site:
            errors.append(message)
    for command in sorted(BENCHMARK_EXEMPTIONS):
        if command not in site:
            errors.append(f"missing site benchmark exception: {command}")

    return errors


def main() -> int:
    site = SITE.read_text(encoding="utf-8")
    data = json.loads(BENCHMARK_DATA.read_text(encoding="utf-8"))
    errors = validate_site_benchmark_evidence(site, data)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
