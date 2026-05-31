#!/usr/bin/env python3
"""Tests for real-data CI coverage verifier."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_real_data_ci_coverage.py")
SPEC = importlib.util.spec_from_file_location("verify_real_data_ci_coverage", MODULE_PATH)
assert SPEC is not None
verify_real_data_ci_coverage = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_real_data_ci_coverage"] = verify_real_data_ci_coverage
SPEC.loader.exec_module(verify_real_data_ci_coverage)


class RealDataCiCoverageTests(unittest.TestCase):
    def test_accepts_all_required_snippets(self) -> None:
        ci_text = "\n".join(verify_real_data_ci_coverage.REQUIRED_SNIPPETS)

        self.assertEqual(verify_real_data_ci_coverage.validate_ci_coverage(ci_text), [])

    def test_reports_missing_snippet(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "tools/update_real_data_manifest.py"
        )

        self.assertIn(
            "CI missing real-data evidence coverage: tools/update_real_data_manifest.py",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )


if __name__ == "__main__":
    unittest.main()
