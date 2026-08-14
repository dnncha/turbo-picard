#!/usr/bin/env python3
"""Verify the production-evidence workflow fails closed before expensive work."""

from __future__ import annotations

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "production-evidence.yml"


def validate_production_evidence_workflow(root: Path = ROOT) -> list[str]:
    path = root / ".github" / "workflows" / "production-evidence.yml"
    if not path.is_file():
        return ["Production evidence workflow is missing"]

    text = path.read_text(encoding="utf-8")
    errors: list[str] = []
    required = (
        (
            "workflow_dispatch:",
            "Production evidence workflow must support manual dispatch",
        ),
        (
            "permissions:\n  contents: read",
            "Production evidence workflow must use read-only contents permissions",
        ),
        (
            "Run dispatch input validator tests",
            "Production evidence validation must run dispatch validator tests",
        ),
        (
            "python -m unittest discover tools -p 'test_validate_production_dispatch.py'",
            "Production evidence validation must run the dispatch validator module",
        ),
        (
            "Run workflow contract verifier tests",
            "Production evidence validation must run workflow verifier tests",
        ),
        (
            "python -m unittest discover tools -p 'test_verify_production_evidence_workflow.py'",
            "Production evidence validation must run the workflow verifier module",
        ),
        (
            "if: github.event_name == 'workflow_dispatch'",
            "Production evidence measurement must be limited to manual dispatch",
        ),
        (
            "needs: validate",
            "Production evidence measurement must depend on validation",
        ),
        (
            "python3 tools/validate_production_dispatch.py",
            "Production evidence measurement must reuse the dispatch validator",
        ),
        ('--dataset-id "$DATASET_ID"', "Dispatch validation must validate the dataset identifier"),
        ('--input-url "$INPUT_URL"', "Dispatch validation must validate the input URL"),
        ('--input-sha256 "$INPUT_SHA256"', "Dispatch validation must validate the input hash"),
        ('--tools "$TOOLS"', "Dispatch validation must validate the selected tools"),
        ('--require-tools "$REQUIRE_TOOLS"', "Dispatch validation must validate the required tools"),
        ('--repeats "$REPEATS"', "Dispatch validation must validate the repeat count"),
        ('--profile "$PROFILE"', "Dispatch validation must validate the workflow profile"),
    )
    for needle, message in required:
        if needle not in text:
            errors.append(message)

    validation_marker = "Validate dispatch inputs"
    download_marker = "Download and hash input"
    build_marker = "Build Turbo-Picard"
    if validation_marker in text and download_marker in text:
        if text.index(validation_marker) > text.index(download_marker):
            errors.append("Dispatch validation must run before input download")
    if validation_marker in text and build_marker in text:
        if text.index(validation_marker) > text.index(build_marker):
            errors.append("Dispatch validation must run before Turbo-Picard build")

    for path_trigger in (
        "tools/verify_production_evidence_workflow.py",
        "tools/test_verify_production_evidence_workflow.py",
    ):
        if text.count(path_trigger) < 2:
            errors.append(
                "Production evidence workflow path triggers must include "
                f"{path_trigger} for pull requests and main pushes"
            )
    return errors


def main() -> int:
    errors = validate_production_evidence_workflow()
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("Production evidence workflow is guarded by the tested dispatch contract")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
