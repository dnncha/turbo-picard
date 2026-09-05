#!/usr/bin/env python3
"""Verify CI runs the repository's Python verifier and test entrypoints."""

from __future__ import annotations

from pathlib import Path
import sys
import re


ROOT = Path(__file__).resolve().parents[1]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
TOOLS = ROOT / "tools"


def tool_paths(pattern: str) -> list[Path]:
    return sorted(
        path
        for path in TOOLS.glob(pattern)
        if path.is_file() and path.name != "__init__.py"
    )


def validate_ci_coverage(ci_text: str, tools_dir: Path = TOOLS) -> list[str]:
    errors: list[str] = []
    # Only count executable standalone commands, not documentation/comments.
    discovery = bool(re.search(r"(?m)^\s*python3 -m unittest discover -s tools\s*$", ci_text))
    compile_all = bool(re.search(r"(?m)^\s*python3 -m compileall -q tools\s*$", ci_text))
    for test_path in tool_paths_in(tools_dir, "test_*.py"):
        rel = f"tools/{test_path.name}"
        if not discovery and f"python3 -m unittest {rel}" not in ci_text:
            errors.append(f"CI does not run unittest module: {rel}")
        if not compile_all and rel not in ci_text:
            errors.append(f"CI does not py_compile test module: {rel}")

    for verifier_path in tool_paths_in(tools_dir, "verify_*.py"):
        rel = f"tools/{verifier_path.name}"
        if f"python3 {rel}" not in ci_text:
            errors.append(f"CI does not run verifier: {rel}")
        if not compile_all and rel not in ci_text:
            errors.append(f"CI does not py_compile verifier: {rel}")
    return errors


def tool_paths_in(tools_dir: Path, pattern: str) -> list[Path]:
    return sorted(
        path
        for path in tools_dir.glob(pattern)
        if path.is_file() and path.name != "__init__.py"
    )


def main() -> int:
    errors = validate_ci_coverage(CI_WORKFLOW.read_text(encoding="utf-8"))
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
