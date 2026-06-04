#!/usr/bin/env python3
"""Tests for benchmark performance threshold verification."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_benchmark_thresholds.py")
SPEC = importlib.util.spec_from_file_location("verify_benchmark_thresholds", MODULE_PATH)
assert SPEC is not None
verify_benchmark_thresholds = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_benchmark_thresholds"] = verify_benchmark_thresholds
SPEC.loader.exec_module(verify_benchmark_thresholds)


class BenchmarkThresholdTests(unittest.TestCase):
    def test_dominating_suite_passes_thresholds(self) -> None:
        data = {
            "parity": "2/2 PASS",
            "summary": {
                "command_count": 2,
                "parity_pass_count": 2,
                "floor_speedup": 7.5,
                "top_speedup": 55.0,
                "geometric_mean_speedup": 20.31,
            },
            "benchmarks": [
                {"command": "A", "speedup": 55.0, "parity": "PASS"},
                {"command": "B", "speedup": 7.5, "parity": "PASS"},
            ],
        }

        errors = verify_benchmark_thresholds.validate_benchmark_thresholds(data)

        self.assertEqual(errors, [])

    def test_threshold_failures_are_reported(self) -> None:
        data = {
            "parity": "1/2 PASS",
            "summary": {
                "command_count": 2,
                "parity_pass_count": 1,
                "floor_speedup": 4.9,
                "top_speedup": 40.0,
                "geometric_mean_speedup": 19.0,
            },
            "benchmarks": [
                {"command": "A", "speedup": 40.0, "parity": "PASS"},
                {"command": "B", "speedup": 4.9, "parity": "FAIL"},
            ],
        }

        errors = verify_benchmark_thresholds.validate_benchmark_thresholds(data)

        self.assertIn("parity pass count 1 does not match command count 2", errors)
        self.assertIn("floor speedup 4.90x is below required 5.00x", errors)
        self.assertIn("geometric mean speedup 19.00x is below required 20.00x", errors)
        self.assertIn("top speedup 40.00x is below required 50.00x", errors)
        self.assertIn("benchmark B parity is FAIL", errors)

    def test_malformed_benchmark_data_reports_errors_instead_of_crashing(self) -> None:
        self.assertEqual(
            verify_benchmark_thresholds.validate_benchmark_thresholds({}),
            ["benchmark-data.json missing summary object"],
        )
        self.assertEqual(
            verify_benchmark_thresholds.validate_benchmark_thresholds({"summary": {}}),
            ["benchmark-data.json missing benchmarks list"],
        )

        errors = verify_benchmark_thresholds.validate_benchmark_thresholds(
            {
                "parity": "2/2 PASS",
                "summary": {
                    "command_count": "two",
                    "parity_pass_count": 1,
                    "floor_speedup": 7.5,
                    "top_speedup": 55.0,
                    "geometric_mean_speedup": 22.0,
                },
                "benchmarks": [],
            }
        )
        self.assertEqual(errors, ["benchmark summary missing numeric command_count"])

        errors = verify_benchmark_thresholds.validate_benchmark_thresholds(
            {
                "parity": "2/2 PASS",
                "summary": {
                    "command_count": 2,
                    "parity_pass_count": 2,
                    "floor_speedup": 7.5,
                    "top_speedup": 55.0,
                    "geometric_mean_speedup": 22.0,
                },
                "benchmarks": [
                    "not-a-row",
                    {"command": "B", "parity": "PASS"},
                ],
            }
        )
        self.assertIn("benchmark row 0 must be an object", errors)
        self.assertIn("benchmark B missing numeric speedup", errors)

    def test_summary_must_match_benchmark_rows(self) -> None:
        data = {
            "parity": "3/3 PASS",
            "summary": {
                "command_count": 3,
                "parity_pass_count": 3,
                "floor_speedup": 7.5,
                "top_speedup": 60.0,
                "geometric_mean_speedup": 30.0,
            },
            "benchmarks": [
                {"command": "A", "speedup": 55.0, "parity": "PASS"},
                {"command": "B", "speedup": 8.0, "parity": "PASS"},
            ],
        }

        errors = verify_benchmark_thresholds.validate_benchmark_thresholds(data)

        self.assertIn(
            "summary command_count 3 does not match benchmark row count 2",
            errors,
        )
        self.assertIn(
            "summary parity_pass_count 3 does not match PASS rows 2",
            errors,
        )
        self.assertIn(
            "top-level parity '3/3 PASS' does not match benchmark rows '2/2 PASS'",
            errors,
        )
        self.assertIn(
            "summary floor_speedup 7.50x does not match benchmark rows 8.00x",
            errors,
        )
        self.assertIn(
            "summary top_speedup 60.00x does not match benchmark rows 55.00x",
            errors,
        )
        self.assertIn(
            "summary geometric_mean_speedup 30.00x does not match benchmark rows 20.98x",
            errors,
        )

    def test_duplicate_benchmark_rows_are_reported(self) -> None:
        data = {
            "parity": "2/2 PASS",
            "summary": {
                "command_count": 2,
                "parity_pass_count": 2,
                "floor_speedup": 8.0,
                "top_speedup": 55.0,
                "geometric_mean_speedup": 20.98,
            },
            "benchmarks": [
                {"command": "A", "speedup": 55.0, "parity": "PASS"},
                {"command": "A", "speedup": 8.0, "parity": "PASS"},
            ],
        }

        errors = verify_benchmark_thresholds.validate_benchmark_thresholds(data)

        self.assertIn("benchmark-data has duplicate command row: A", errors)


if __name__ == "__main__":
    unittest.main()
