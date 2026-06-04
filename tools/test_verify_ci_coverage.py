#!/usr/bin/env python3
"""Tests for CI coverage checks."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_ci_coverage.py")
SPEC = importlib.util.spec_from_file_location("verify_ci_coverage", MODULE_PATH)
assert SPEC is not None
verify_ci_coverage = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_ci_coverage"] = verify_ci_coverage
SPEC.loader.exec_module(verify_ci_coverage)


class CiCoverageTests(unittest.TestCase):
    def test_accepts_tests_and_verifiers_present_in_ci(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tools = Path(tmp)
            (tools / "test_example.py").write_text("# test\n", encoding="utf-8")
            (tools / "verify_example.py").write_text("# verifier\n", encoding="utf-8")
            ci = """
python3 -m unittest tools/test_example.py
python3 tools/verify_example.py
python3 -m py_compile \
  tools/test_example.py \
  tools/verify_example.py
"""

            self.assertEqual(
                verify_ci_coverage.validate_ci_coverage(ci, tools),
                [],
            )

    def test_reports_missing_test_and_verifier_coverage(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tools = Path(tmp)
            (tools / "test_example.py").write_text("# test\n", encoding="utf-8")
            (tools / "verify_example.py").write_text("# verifier\n", encoding="utf-8")

            self.assertEqual(
                verify_ci_coverage.validate_ci_coverage("", tools),
                [
                    "CI does not run unittest module: tools/test_example.py",
                    "CI does not py_compile test module: tools/test_example.py",
                    "CI does not run verifier: tools/verify_example.py",
                    "CI does not py_compile verifier: tools/verify_example.py",
                ],
            )


if __name__ == "__main__":
    unittest.main()
