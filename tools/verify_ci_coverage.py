#!/usr/bin/env python3

from __future__ import annotations

import importlib
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
TOOLS = ROOT / "tools"


def tool_paths(pattern: str) -> list[Path]:
    return sorted(path for path in TOOLS.glob(pattern) if path.is_file() and path.name != "__init__.py")


def validate_ci_coverage(ci_text: str, tools_dir: Path = TOOLS) -> list[str]:
    errors: list[str] = []
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
    return sorted(path for path in tools_dir.glob(pattern) if path.is_file() and path.name != "__init__.py")


def validate_scientific_parity_ci(ci_text: str) -> list[str]:
    yaml = importlib.import_module("yaml")
    workflow = yaml.safe_load(ci_text)
    job = workflow.get("jobs", {}).get("picard-parity", {})
    errors: list[str] = []
    versions = job.get("strategy", {}).get("matrix", {}).get("picard", [])
    if not {"3.4.0", "3.5.0"}.issubset(set(versions)):
        errors.append("Picard parity matrix must include 3.4.0 and 3.5.0")
    steps = job.get("steps", [])
    gates = [
        step
        for step in steps
        if re.search(r"(?m)^\s*python3 tools/verify_adversarial_parity.py(?:\s|$)", step.get("run", ""))
    ]
    if not gates or any(step.get("continue-on-error") or step.get("if") for step in gates):
        errors.append("both reference jobs must unconditionally run the adversarial parity gate")
    if not gates or not all(
        '--expected-picard-version "${{ matrix.picard }}"' in step.get("run", "") for step in gates
    ):
        errors.append("adversarial parity must verify the actual selected Picard version")
    if not gates or not all(
        'export PATH="${MAMBA_ROOT_PREFIX}/envs/turbo-picard-ci/bin:$PATH"' in step.get("run", "") for step in gates
    ):
        errors.append("adversarial parity must expose the selected environment's samtools and Rscript")
    workloads = [step.get("run", "") for step in steps if "./tools/verify_basic_cram_parity.sh" in step.get("run", "")]
    if not workloads or not all(
        re.search(
            r"(?m)^\s*export TURBO_PICARD_REQUIRE_NATIVE=1\s*$", run[: run.index("./tools/verify_basic_cram_parity.sh")]
        )
        for run in workloads
    ):
        errors.append("scientific workload parity must require native execution before running commands")
    retained = [
        step
        for step in steps
        if step.get("uses", "").startswith("actions/upload-artifact@")
        and "adversarial-parity" in step.get("with", {}).get("path", "")
    ]
    if not retained or any(step.get("if") != "always()" for step in retained):
        errors.append("adversarial parity evidence must be retained after failure")
    return errors


def main() -> int:
    ci_text = CI_WORKFLOW.read_text(encoding="utf-8")
    errors = validate_ci_coverage(ci_text) + validate_scientific_parity_ci(ci_text)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
