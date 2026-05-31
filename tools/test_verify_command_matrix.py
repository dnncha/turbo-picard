#!/usr/bin/env python3
"""Tests for command matrix consistency checks."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_command_matrix.py")
SPEC = importlib.util.spec_from_file_location("verify_command_matrix", MODULE_PATH)
assert SPEC is not None
verify_command_matrix = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_command_matrix"] = verify_command_matrix
SPEC.loader.exec_module(verify_command_matrix)


class CommandMatrixTests(unittest.TestCase):
    def test_scope_validation_accepts_complete_entries(self) -> None:
        errors = verify_command_matrix.validate_scope_notes(
            [
                {
                    "name": "SortSam",
                    "native_scope": "Coordinate and queryname sorting for SAM/BAM.",
                    "fallback_scope": "Unsupported sort orders should use upstream Picard.",
                }
            ]
        )

        self.assertEqual(errors, [])

    def test_scope_validation_reports_missing_and_vague_entries(self) -> None:
        errors = verify_command_matrix.validate_scope_notes(
            [
                {
                    "name": "MarkDuplicates",
                    "native_scope": "",
                    "fallback_scope": "TBD",
                }
            ]
        )

        self.assertIn("MarkDuplicates missing native_scope", errors)
        self.assertIn("MarkDuplicates has vague fallback_scope: TBD", errors)

    def test_scope_validation_allows_explicit_placeholder_chart_disclosure(self) -> None:
        errors = verify_command_matrix.validate_scope_notes(
            [
                {
                    "name": "QualityScoreDistribution",
                    "native_scope": "Quality histogram metrics and placeholder PDF chart artifact.",
                    "fallback_scope": "Rendered plots should use upstream Picard.",
                }
            ]
        )

        self.assertEqual(errors, [])


if __name__ == "__main__":
    unittest.main()
