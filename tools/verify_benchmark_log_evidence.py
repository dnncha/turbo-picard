#!/usr/bin/env python3
"""Verify raw benchmark suite output matches rendered benchmark-data.json."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
TOOL_DIR = Path(__file__).resolve().parent
if str(TOOL_DIR) not in sys.path:
    sys.path.insert(0, str(TOOL_DIR))

from render_benchmark_assets import (  # noqa: E402
    build_benchmark_data_from_suite_output,
    parse_suite_metadata,
)


SUITE_OUTPUT = ROOT / "docs" / "site" / "assets" / "bench-suite-output.txt"
BENCHMARK_DATA = ROOT / "docs" / "site" / "assets" / "benchmark-data.json"
SOURCE_ARTIFACT = "docs/site/assets/bench-suite-output.txt"
EXPECTED_SOURCE_PREFIX = "python3 tools/bench_suite.py"


def format_value(value: Any) -> str:
    return repr(value)


def compare_field(
    errors: list[str],
    *,
    label: str,
    actual: Any,
    expected: Any,
) -> None:
    if actual != expected:
        errors.append(
            f"{label} is {format_value(actual)}, expected {format_value(expected)}"
        )


def validate_benchmark_log_evidence(
    suite_output: str,
    manifest: dict,
    *,
    source_artifact: str = SOURCE_ARTIFACT,
) -> list[str]:
    errors = []
    metadata = parse_suite_metadata(suite_output)
    if "benchmark_date" not in metadata:
        errors.append("raw benchmark log is missing benchmark_date metadata")
    elif not re.fullmatch(r"\d{4}-\d{2}-\d{2}", metadata["benchmark_date"]):
        errors.append(
            f"raw benchmark log has non-ISO benchmark_date: {metadata['benchmark_date']}"
        )
    if "source" not in metadata:
        errors.append("raw benchmark log is missing source metadata")
    elif not metadata["source"].startswith(EXPECTED_SOURCE_PREFIX):
        errors.append(
            "raw benchmark log source must start with "
            f"{EXPECTED_SOURCE_PREFIX}: {metadata['source']}"
        )

    artifact_path = Path(source_artifact)
    if artifact_path.is_absolute() or ".." in artifact_path.parts:
        errors.append(
            f"benchmark source_artifact must be repository-relative: {source_artifact}"
        )
    else:
        try:
            artifact_path.relative_to("docs/site/assets")
        except ValueError:
            errors.append(
                "benchmark source_artifact must stay under docs/site/assets: "
                f"{source_artifact}"
            )
        if not (ROOT / artifact_path).exists():
            errors.append(f"benchmark source_artifact is missing: {source_artifact}")

    expected = build_benchmark_data_from_suite_output(
        suite_output,
        source_artifact=source_artifact,
    )

    for field in ("source", "date", "parity", "source_artifact"):
        compare_field(
            errors,
            label=f"manifest field {field}",
            actual=manifest.get(field),
            expected=expected.get(field),
        )

    actual_summary = manifest.get("summary", {})
    expected_summary = expected["summary"]
    for key in (
        "command_count",
        "parity_pass_count",
        "top_speedup",
        "top_command",
        "floor_speedup",
        "floor_command",
        "median_speedup",
        "geometric_mean_speedup",
    ):
        compare_field(
            errors,
            label=f"manifest summary {key}",
            actual=actual_summary.get(key),
            expected=expected_summary[key],
        )

    actual_by_command: dict[str, dict] = {}
    benchmark_rows = manifest.get("benchmarks", [])
    if not isinstance(benchmark_rows, list):
        errors.append("manifest benchmarks must be a list")
        benchmark_rows = []
    for index, row in enumerate(benchmark_rows):
        if not isinstance(row, dict):
            errors.append(f"manifest benchmark row {index} must be an object")
            continue
        command = row.get("command")
        if not isinstance(command, str) or not command:
            errors.append(f"manifest benchmark row {index} missing command")
            continue
        if command in actual_by_command:
            errors.append(f"manifest has duplicate benchmark command: {command}")
            continue
        actual_by_command[command] = row
    expected_by_command = {row["command"]: row for row in expected["benchmarks"]}
    for command in sorted(set(actual_by_command) - set(expected_by_command)):
        errors.append(f"manifest has benchmark not present in raw log: {command}")
    for command in sorted(set(expected_by_command) - set(actual_by_command)):
        errors.append(f"manifest is missing benchmark from raw log: {command}")

    for command in sorted(set(actual_by_command) & set(expected_by_command)):
        actual_row = actual_by_command[command]
        expected_row = expected_by_command[command]
        # Legacy manifests contain ratios only. If absolute evidence is published,
        # it must match the raw log too; never verify just its headline speedup.
        keys = ["rank", "speedup", "parity"] + [key for key in (
            "median_turbo_seconds", "median_picard_seconds", "runs", "workload_parameter"
        ) if key in actual_row]
        for key in keys:
            compare_field(
                errors,
                label=f"manifest benchmark {command} {key}",
                actual=actual_row.get(key),
                expected=expected_row.get(key),
            )

    return errors


def main() -> int:
    suite_output = SUITE_OUTPUT.read_text(encoding="utf-8")
    manifest = json.loads(BENCHMARK_DATA.read_text(encoding="utf-8"))
    errors = validate_benchmark_log_evidence(
        suite_output,
        manifest,
        source_artifact=SOURCE_ARTIFACT,
    )
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
