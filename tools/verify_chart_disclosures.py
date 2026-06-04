#!/usr/bin/env python3
"""Ensure lightweight chart artifact boundaries are disclosed in user-facing docs."""

from __future__ import annotations

import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
README = ROOT / "README.md"
MATRIX = ROOT / "docs" / "command-matrix.yml"

LIGHTWEIGHT_CHART_COMMANDS = [
    "CollectBaseDistributionByCycle",
    "CollectGcBiasMetrics",
    "CollectInsertSizeMetrics",
    "MeanQualityByCycle",
    "QualityScoreDistribution",
]


def section_for_command(text: str, command: str) -> str:
    matches = list(re.finditer(r"^## .*$", text, re.MULTILINE))
    fallback = ""
    for index, match in enumerate(matches):
        start = match.start()
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        section = text[start:end]
        heading = match.group(0)
        if command in heading:
            return section
        if not fallback and command in section:
            fallback = section
    return fallback


def matrix_native_scope(matrix_text: str, command: str) -> str:
    pattern = re.compile(
        rf"^\s*-\s+name: {re.escape(command)}\n(?P<body>(?:\s+.+\n?)*)",
        re.MULTILINE,
    )
    match = pattern.search(matrix_text)
    if not match:
        return ""
    for line in match.group("body").splitlines():
        scope_match = re.match(r'\s+native_scope:\s+"?(.*?)"?$', line)
        if scope_match:
            return scope_match.group(1)
    return ""


def has_chart_boundary_wording(text: str) -> bool:
    normalized = text.lower()
    return "lightweight" in normalized and "pdf" in normalized


def validate_chart_disclosures(
    *,
    chart_commands: list[str],
    readme_text: str,
    matrix_text: str,
) -> list[str]:
    errors: list[str] = []
    for command in chart_commands:
        readme_section = section_for_command(readme_text, command)
        if not has_chart_boundary_wording(readme_section):
            errors.append(
                f"README chart disclosure missing lightweight PDF wording for {command}"
            )
        native_scope = matrix_native_scope(matrix_text, command)
        if not has_chart_boundary_wording(native_scope):
            errors.append(
                f"command matrix native_scope missing lightweight PDF wording for {command}"
            )
    return errors


def main() -> int:
    errors = validate_chart_disclosures(
        chart_commands=LIGHTWEIGHT_CHART_COMMANDS,
        readme_text=README.read_text(encoding="utf-8"),
        matrix_text=MATRIX.read_text(encoding="utf-8"),
    )
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
