#!/usr/bin/env python3
"""Verify benchmark suite coverage is explicit and aligned with public evidence."""

from __future__ import annotations

import ast
import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOOL_DIR = Path(__file__).resolve().parent
if str(TOOL_DIR) not in sys.path:
    sys.path.insert(0, str(TOOL_DIR))

from verify_command_matrix import matrix_native_commands  # noqa: E402


BENCH_SUITE = ROOT / "tools" / "bench_suite.py"
BENCHMARK_DATA = ROOT / "docs" / "site" / "assets" / "benchmark-data.json"
MINIMUM_BENCHMARK_COUNT = 28


def suite_benchmark_commands(text: str) -> set[str]:
    tree = ast.parse(text)
    commands = set()
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        if not isinstance(node.func, ast.Name) or node.func.id != "run_benchmark":
            continue
        if len(node.args) < 2:
            continue
        script_arg = node.args[1]
        if not isinstance(script_arg, ast.Constant) or not isinstance(script_arg.value, str):
            continue
        script_path = ROOT / "tools" / script_arg.value
        if not script_path.is_file():
            commands.add(f"<missing script {script_arg.value}>")
            continue
        commands.update(benchmark_script_commands(script_path.read_text(encoding="utf-8")))
    return commands


def benchmark_script_commands(text: str) -> set[str]:
    tree = ast.parse(text)
    commands = set()
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        if not isinstance(node.func, ast.Name) or node.func.id != "print":
            continue
        if len(node.args) != 1:
            continue
        value = string_literal_value(node.args[0])
        if value is None:
            continue
        if not value.startswith("command="):
            continue
        commands.add(value.split("=", 1)[1])
    return commands


def string_literal_value(node: ast.AST) -> str | None:
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value
    if isinstance(node, ast.JoinedStr):
        parts = []
        for value in node.values:
            if not isinstance(value, ast.Constant) or not isinstance(value.value, str):
                return None
            parts.append(value.value)
        return "".join(parts)
    return None


def manifest_benchmark_commands(data: dict) -> set[str]:
    return {row["command"] for row in data.get("benchmarks", [])}


def validate_benchmark_suite_coverage(
    *,
    matrix_commands: set[str],
    suite_commands: set[str],
    manifest_commands: set[str],
    minimum_benchmark_count: int = MINIMUM_BENCHMARK_COUNT,
) -> list[str]:
    errors = []

    for command in sorted((suite_commands | manifest_commands) - matrix_commands):
        errors.append(f"benchmarked command missing from command matrix: {command}")
    for command in sorted(suite_commands - manifest_commands):
        errors.append(f"suite benchmark missing from manifest: {command}")
    for command in sorted(manifest_commands - suite_commands):
        errors.append(f"manifest benchmark missing from suite: {command}")
    if len(suite_commands) < minimum_benchmark_count:
        errors.append(
            f"benchmark suite covers {len(suite_commands)} commands, below required minimum {minimum_benchmark_count}"
        )

    return errors


def main() -> int:
    matrix_commands = matrix_native_commands()
    suite_commands = suite_benchmark_commands(BENCH_SUITE.read_text(encoding="utf-8"))
    manifest_commands = manifest_benchmark_commands(
        json.loads(BENCHMARK_DATA.read_text(encoding="utf-8"))
    )
    errors = validate_benchmark_suite_coverage(
        matrix_commands=matrix_commands,
        suite_commands=suite_commands,
        manifest_commands=manifest_commands,
    )
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
