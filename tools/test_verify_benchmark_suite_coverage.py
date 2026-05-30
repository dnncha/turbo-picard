#!/usr/bin/env python3
"""Tests for benchmark suite coverage consistency checks."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_benchmark_suite_coverage.py")
SPEC = importlib.util.spec_from_file_location("verify_benchmark_suite_coverage", MODULE_PATH)
assert SPEC is not None
verify_benchmark_suite_coverage = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_benchmark_suite_coverage"] = verify_benchmark_suite_coverage
SPEC.loader.exec_module(verify_benchmark_suite_coverage)


class BenchmarkSuiteCoverageTests(unittest.TestCase):
    def test_suite_coverage_accepts_matrix_backed_benchmarks(self) -> None:
        matrix_commands = {"SortSam", "BuildBamIndex", "CreateSequenceDictionary"}
        suite_commands = {"SortSam", "BuildBamIndex", "CreateSequenceDictionary"}
        manifest_commands = {"SortSam", "BuildBamIndex", "CreateSequenceDictionary"}

        errors = verify_benchmark_suite_coverage.validate_benchmark_suite_coverage(
            matrix_commands=matrix_commands,
            suite_commands=suite_commands,
            manifest_commands=manifest_commands,
            minimum_benchmark_count=3,
        )

        self.assertEqual(errors, [])

    def test_suite_manifest_and_matrix_mismatches_are_reported(self) -> None:
        errors = verify_benchmark_suite_coverage.validate_benchmark_suite_coverage(
            matrix_commands={"SortSam", "BuildBamIndex"},
            suite_commands={"SortSam", "CreateSequenceDictionary"},
            manifest_commands={"BuildBamIndex", "UnknownCommand"},
            minimum_benchmark_count=3,
        )

        self.assertIn("benchmarked command missing from command matrix: UnknownCommand", errors)
        self.assertIn("suite benchmark missing from manifest: SortSam", errors)
        self.assertIn("manifest benchmark missing from suite: BuildBamIndex", errors)
        self.assertIn("manifest benchmark missing from suite: UnknownCommand", errors)
        self.assertIn(
            "benchmark suite covers 2 commands, below required minimum 3",
            errors,
        )


if __name__ == "__main__":
    unittest.main()
