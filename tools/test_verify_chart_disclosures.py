#!/usr/bin/env python3
"""Tests for chart boundary disclosure checks."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_chart_disclosures.py")
SPEC = importlib.util.spec_from_file_location("verify_chart_disclosures", MODULE_PATH)
assert SPEC is not None
verify_chart_disclosures = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_chart_disclosures"] = verify_chart_disclosures
SPEC.loader.exec_module(verify_chart_disclosures)


class ChartDisclosureTests(unittest.TestCase):
    def test_disclosures_accept_readme_and_matrix_mentions(self) -> None:
        commands = ["QualityScoreDistribution", "MeanQualityByCycle"]
        readme = """
## Supported QualityScoreDistribution and MeanQualityByCycle Surface
The CHART_OUTPUT / CHART files are lightweight PDF artifacts, not Picard-equivalent rendered plots.
"""
        matrix = """
- name: QualityScoreDistribution
  native_scope: "Quality histogram metrics and lightweight PDF chart artifact."
- name: MeanQualityByCycle
  native_scope: "Mean quality metrics and lightweight PDF chart artifact."
"""

        errors = verify_chart_disclosures.validate_chart_disclosures(
            chart_commands=commands,
            readme_text=readme,
            matrix_text=matrix,
        )

        self.assertEqual(errors, [])

    def test_disclosures_report_missing_readme_and_matrix_mentions(self) -> None:
        errors = verify_chart_disclosures.validate_chart_disclosures(
            chart_commands=["CollectInsertSizeMetrics"],
            readme_text="## Supported CollectInsertSizeMetrics Surface\nCHART_OUTPUT / CHART chart artifact",
            matrix_text='- name: CollectInsertSizeMetrics\n  native_scope: "Insert-size metrics and chart artifact."',
        )

        self.assertIn(
            "README chart disclosure missing lightweight PDF wording for CollectInsertSizeMetrics",
            errors,
        )
        self.assertIn(
            "command matrix native_scope missing lightweight PDF wording for CollectInsertSizeMetrics",
            errors,
        )


if __name__ == "__main__":
    unittest.main()
