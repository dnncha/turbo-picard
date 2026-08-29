from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("verify_production_evidence_workflow.py")
SPEC = importlib.util.spec_from_file_location(
    "verify_production_evidence_workflow", MODULE_PATH
)
assert SPEC is not None and SPEC.loader is not None
verify_production_evidence_workflow = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = verify_production_evidence_workflow
SPEC.loader.exec_module(verify_production_evidence_workflow)


VALID_WORKFLOW = """
on:
  pull_request:
    paths:
      - tools/verify_production_evidence_workflow.py
      - tools/test_verify_production_evidence_workflow.py
  push:
    branches: [main]
    paths:
      - tools/verify_production_evidence_workflow.py
      - tools/test_verify_production_evidence_workflow.py
  workflow_dispatch:
permissions:
  contents: read
jobs:
  validate:
    steps:
      - name: Run dispatch input validator tests
        run: python -m unittest discover tools -p 'test_validate_production_dispatch.py'
      - name: Run workflow verifier
        run: python3 tools/verify_production_evidence_workflow.py
      - name: Run workflow contract verifier tests
        run: python -m unittest discover tools -p 'test_verify_production_evidence_workflow.py'
  measure:
    if: github.event_name == 'workflow_dispatch'
    needs: validate
    steps:
      - name: Validate dispatch inputs
        run: |
          python3 tools/validate_production_dispatch.py \
            --dataset-id "$DATASET_ID" \
            --input-url "$INPUT_URL" \
            --input-sha256 "$INPUT_SHA256" \
            --tools "$TOOLS" \
            --require-tools "$REQUIRE_TOOLS" \
            --repeats "$REPEATS" \
            --profile "$PROFILE"
      - name: Download and hash input
      - name: Build Turbo-Picard
"""


class ProductionEvidenceWorkflowTests(unittest.TestCase):
    def write_workflow(self, text: str) -> Path:
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        path = root / ".github" / "workflows" / "production-evidence.yml"
        path.parent.mkdir(parents=True)
        path.write_text(text, encoding="utf-8")
        return root

    def test_accepts_guarded_workflow(self) -> None:
        root = self.write_workflow(VALID_WORKFLOW)
        self.assertEqual(
            [],
            verify_production_evidence_workflow.validate_production_evidence_workflow(
                root
            ),
        )

    def test_accepts_current_workflow(self) -> None:
        self.assertEqual(
            [], verify_production_evidence_workflow.validate_production_evidence_workflow()
        )

    def test_rejects_validator_after_download(self) -> None:
        root = self.write_workflow(
            VALID_WORKFLOW.replace(
                "      - name: Validate dispatch inputs\n",
                "      - name: Download and hash input\n"
                "      - name: Validate dispatch inputs\n",
            )
        )
        errors = verify_production_evidence_workflow.validate_production_evidence_workflow(
            root
        )
        self.assertIn("Dispatch validation must run before input download", errors)

    def test_rejects_missing_measurement_dependency(self) -> None:
        root = self.write_workflow(VALID_WORKFLOW.replace("    needs: validate\n", ""))
        errors = verify_production_evidence_workflow.validate_production_evidence_workflow(
            root
        )
        self.assertIn("Production evidence measurement must depend on validation", errors)

    def test_rejects_missing_path_trigger(self) -> None:
        root = self.write_workflow(
            VALID_WORKFLOW.replace(
                "      - tools/test_verify_production_evidence_workflow.py\n",
                "",
            )
        )
        errors = verify_production_evidence_workflow.validate_production_evidence_workflow(
            root
        )
        self.assertTrue(
            any(
                "path triggers must include tools/test_verify_production_evidence_workflow.py"
                in error
                for error in errors
            )
        )


if __name__ == "__main__":
    unittest.main()
