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
        matrix_commands = {
            "SortSam",
            "BuildBamIndex",
            "CreateSequenceDictionary",
            "LiftoverVcf",
        }
        suite_commands = {"SortSam", "BuildBamIndex", "CreateSequenceDictionary"}
        manifest_commands = {"SortSam", "BuildBamIndex", "CreateSequenceDictionary"}
        benchmark_exemptions = {
            "LiftoverVcf": "liftover benchmark needs a larger chain-backed fixture before public speedup claims",
        }

        errors = verify_benchmark_suite_coverage.validate_benchmark_suite_coverage(
            matrix_commands=matrix_commands,
            suite_commands=suite_commands,
            manifest_commands=manifest_commands,
            benchmark_exemptions=benchmark_exemptions,
            benchmark_docs=(
                "LiftoverVcf liftover benchmark needs a larger chain-backed fixture "
                "before public speedup claims"
            ),
            minimum_benchmark_count=3,
        )

        self.assertEqual(errors, [])

    def test_suite_manifest_and_matrix_mismatches_are_reported(self) -> None:
        errors = verify_benchmark_suite_coverage.validate_benchmark_suite_coverage(
            matrix_commands={"SortSam", "BuildBamIndex"},
            suite_commands={"SortSam", "CreateSequenceDictionary"},
            manifest_commands={"BuildBamIndex", "UnknownCommand"},
            benchmark_exemptions={"MissingExemption": "has a reason"},
            benchmark_docs="",
            minimum_benchmark_count=3,
        )

        self.assertIn("benchmarked command missing from command matrix: UnknownCommand", errors)
        self.assertIn(
            "matrix native command missing benchmark or exemption: BuildBamIndex",
            errors,
        )
        self.assertIn(
            "benchmark exemption is not a matrix native command: MissingExemption",
            errors,
        )
        self.assertIn("suite benchmark missing from manifest: SortSam", errors)
        self.assertIn("manifest benchmark missing from suite: BuildBamIndex", errors)
        self.assertIn("manifest benchmark missing from suite: UnknownCommand", errors)
        self.assertIn(
            "benchmark suite covers 2 commands, below required minimum 3",
            errors,
        )

    def test_manifest_benchmark_command_parser_reports_malformed_rows(self) -> None:
        commands, errors = verify_benchmark_suite_coverage.manifest_benchmark_commands_with_errors(
            {
                "benchmarks": [
                    {"command": "SortSam"},
                    {"command": "SortSam"},
                    {"speedup": 12.0},
                    "not-a-row",
                ]
            }
        )

        self.assertEqual(commands, {"SortSam"})
        self.assertIn("benchmark-data has duplicate command row: SortSam", errors)
        self.assertIn("benchmark-data row 2 missing command", errors)
        self.assertIn("benchmark-data row 3 must be an object", errors)

        commands, errors = verify_benchmark_suite_coverage.manifest_benchmark_commands_with_errors(
            {"benchmarks": "not-a-list"}
        )
        self.assertEqual(commands, set())
        self.assertEqual(errors, ["benchmark-data benchmarks must be a list"])

    def test_exemption_requires_reason_and_docs(self) -> None:
        errors = verify_benchmark_suite_coverage.validate_benchmark_suite_coverage(
            matrix_commands={"LiftoverVcf"},
            suite_commands=set(),
            manifest_commands=set(),
            benchmark_exemptions={"LiftoverVcf": "todo"},
            benchmark_docs="LiftoverVcf",
            minimum_benchmark_count=0,
        )

        self.assertIn(
            "benchmark exemption for LiftoverVcf has no useful reason",
            errors,
        )
        self.assertIn(
            "benchmark docs missing exemption reason for LiftoverVcf",
            errors,
        )

    def test_promoted_benchmark_cannot_remain_exempt(self) -> None:
        errors = verify_benchmark_suite_coverage.validate_benchmark_suite_coverage(
            matrix_commands={"IntervalListTools"},
            suite_commands={"IntervalListTools"},
            manifest_commands={"IntervalListTools"},
            benchmark_exemptions={
                "IntervalListTools": "standalone benchmark passes parity",
            },
            benchmark_docs="IntervalListTools standalone benchmark passes parity",
            minimum_benchmark_count=1,
        )

        self.assertIn(
            "benchmark exemption also appears in suite: IntervalListTools",
            errors,
        )

    def test_default_minimum_tracks_matrix_minus_exemptions(self) -> None:
        errors = verify_benchmark_suite_coverage.validate_benchmark_suite_coverage(
            matrix_commands={"SortSam", "BuildBamIndex", "LiftoverVcf"},
            suite_commands={"SortSam"},
            manifest_commands={"SortSam"},
            benchmark_exemptions={
                "LiftoverVcf": "chain-backed benchmark is tracked separately",
            },
            benchmark_docs="LiftoverVcf chain-backed benchmark is tracked separately",
        )

        self.assertIn(
            "matrix native command missing benchmark or exemption: BuildBamIndex",
            errors,
        )
        self.assertIn(
            "benchmark suite covers 1 commands, below required minimum 2",
            errors,
        )


if __name__ == "__main__":
    unittest.main()
