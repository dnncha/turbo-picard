#!/usr/bin/env python3
"""Verify benchmark-data.json meets explicit performance dominance thresholds."""

from __future__ import annotations

import json
import math
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
    summary = data.get("summary")
    if not isinstance(summary, dict):
        return ["benchmark-data.json missing summary object"]
    benchmarks = data.get("benchmarks")
    if not isinstance(benchmarks, list):
        return ["benchmark-data.json missing benchmarks list"]

    numeric_summary: dict[str, float] = {}
    for key in (
        "command_count",
        "parity_pass_count",
        "floor_speedup",
        "top_speedup",
        "geometric_mean_speedup",
    ):
        value = summary.get(key)
        try:
            numeric_summary[key] = float(value)
        except (TypeError, ValueError):
            errors.append(f"benchmark summary missing numeric {key}")
    if errors:
        return errors

    command_count = int(numeric_summary["command_count"])
    parity_pass_count = int(numeric_summary["parity_pass_count"])
    floor_speedup = numeric_summary["floor_speedup"]
    top_speedup = numeric_summary["top_speedup"]
    geomean_speedup = numeric_summary["geometric_mean_speedup"]

    row_speedups: list[float] = []
    row_pass_count = 0
    row_commands: set[str] = set()
    for index, row in enumerate(benchmarks):
        if not isinstance(row, dict):
            continue
        command = row.get("command")
        if isinstance(command, str) and command:
            if command in row_commands:
                errors.append(f"benchmark-data has duplicate command row: {command}")
            row_commands.add(command)
        parity = row.get("parity")
        if parity == "PASS":
            row_pass_count += 1
        try:
            speedup = float(row.get("speedup"))
        except (TypeError, ValueError):
            continue
        row_speedups.append(speedup)

    if command_count != len(benchmarks):
        errors.append(
            f"summary command_count {command_count} does not match benchmark row count {len(benchmarks)}"
        )
    if parity_pass_count != row_pass_count:
        errors.append(
            f"summary parity_pass_count {parity_pass_count} does not match PASS rows {row_pass_count}"
        )
    top_level_parity = data.get("parity")
    expected_parity = f"{row_pass_count}/{len(benchmarks)} PASS"
    if top_level_parity != expected_parity:
        errors.append(
            f"top-level parity {top_level_parity!r} does not match benchmark rows {expected_parity!r}"
        )
    if row_speedups:
        calculated_floor = min(row_speedups)
        calculated_top = max(row_speedups)
        calculated_geomean = round(math.prod(row_speedups) ** (1 / len(row_speedups)), 2)
        if round(floor_speedup, 2) != round(calculated_floor, 2):
            errors.append(
                f"summary floor_speedup {format_speedup(floor_speedup)} does not match benchmark rows {format_speedup(calculated_floor)}"
            )
        if round(top_speedup, 2) != round(calculated_top, 2):
            errors.append(
                f"summary top_speedup {format_speedup(top_speedup)} does not match benchmark rows {format_speedup(calculated_top)}"
            )
        if round(geomean_speedup, 2) != calculated_geomean:
            errors.append(
                f"summary geometric_mean_speedup {format_speedup(geomean_speedup)} does not match benchmark rows {format_speedup(calculated_geomean)}"
            )

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

    for index, row in enumerate(benchmarks):
        if not isinstance(row, dict):
            errors.append(f"benchmark row {index} must be an object")
            continue
        command = row.get("command", f"<row {index}>")
        parity = row.get("parity")
        if parity != "PASS":
            errors.append(f"benchmark {command} parity is {parity}")
        try:
            speedup = float(row.get("speedup"))
        except (TypeError, ValueError):
            errors.append(f"benchmark {command} missing numeric speedup")
            continue
        if speedup < MIN_FLOOR_SPEEDUP:
            errors.append(
                f"benchmark {command} speedup {format_speedup(speedup)} is below required {format_speedup(MIN_FLOOR_SPEEDUP)}"
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
