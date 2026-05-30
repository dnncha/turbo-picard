#!/usr/bin/env python3
"""Verify benchmark-data.json meets explicit performance dominance thresholds."""

from __future__ import annotations

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BENCHMARK_DATA = ROOT / "docs" / "site" / "assets" / "benchmark-data.json"

MIN_FLOOR_SPEEDUP = 5.0
MIN_GEOMEAN_SPEEDUP = 20.0
MIN_TOP_SPEEDUP = 50.0


def format_speedup(value: float) -> str:
    return f"{value:.2f}x"


def validate_benchmark_thresholds(data: dict) -> list[str]:
    errors = []
    summary = data["summary"]
    command_count = int(summary["command_count"])
    parity_pass_count = int(summary["parity_pass_count"])
    floor_speedup = float(summary["floor_speedup"])
    top_speedup = float(summary["top_speedup"])
    geomean_speedup = float(summary["geometric_mean_speedup"])

    if parity_pass_count != command_count:
        errors.append(
            f"parity pass count {parity_pass_count} does not match command count {command_count}"
        )
    if floor_speedup < MIN_FLOOR_SPEEDUP:
        errors.append(
            f"floor speedup {format_speedup(floor_speedup)} is below required {format_speedup(MIN_FLOOR_SPEEDUP)}"
        )
    if geomean_speedup < MIN_GEOMEAN_SPEEDUP:
        errors.append(
            f"geometric mean speedup {format_speedup(geomean_speedup)} is below required {format_speedup(MIN_GEOMEAN_SPEEDUP)}"
        )
    if top_speedup < MIN_TOP_SPEEDUP:
        errors.append(
            f"top speedup {format_speedup(top_speedup)} is below required {format_speedup(MIN_TOP_SPEEDUP)}"
        )

    for row in data["benchmarks"]:
        if row["parity"] != "PASS":
            errors.append(f"benchmark {row['command']} parity is {row['parity']}")
        if float(row["speedup"]) < MIN_FLOOR_SPEEDUP:
            errors.append(
                f"benchmark {row['command']} speedup {format_speedup(float(row['speedup']))} is below required {format_speedup(MIN_FLOOR_SPEEDUP)}"
            )

    return errors


def main() -> int:
    data = json.loads(BENCHMARK_DATA.read_text(encoding="utf-8"))
    errors = validate_benchmark_thresholds(data)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
