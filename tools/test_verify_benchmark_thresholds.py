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
            "summary": {
                "command_count": 2,
                "parity_pass_count": 2,
                "floor_speedup": 7.5,
                "top_speedup": 55.0,
                "geometric_mean_speedup": 22.0,
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


if __name__ == "__main__":
    unittest.main()
