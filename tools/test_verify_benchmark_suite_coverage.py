#!/usr/bin/env python3

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
                "LiftoverVcf liftover benchmark needs a larger chain-backed fixture before public speedup claims"
            ),
            minimum_benchmark_count=3,
        )

        assert errors == []

    def test_suite_manifest_and_matrix_mismatches_are_reported(self) -> None:
        errors = verify_benchmark_suite_coverage.validate_benchmark_suite_coverage(
            matrix_commands={"SortSam", "BuildBamIndex"},
            suite_commands={"SortSam", "CreateSequenceDictionary"},
            manifest_commands={"BuildBamIndex", "UnknownCommand"},
            benchmark_exemptions={"MissingExemption": "has a reason"},
            benchmark_docs="",
            minimum_benchmark_count=3,
        )

        assert "benchmarked command missing from command matrix: UnknownCommand" in errors
        assert "matrix native command missing benchmark or exemption: BuildBamIndex" in errors
        assert "benchmark exemption is not a matrix native command: MissingExemption" in errors
        assert "suite benchmark missing from manifest: SortSam" in errors
        assert "manifest benchmark missing from suite: BuildBamIndex" in errors
        assert "manifest benchmark missing from suite: UnknownCommand" in errors
        assert "benchmark suite covers 2 commands, below required minimum 3" in errors

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

        assert commands == {"SortSam"}
        assert "benchmark-data has duplicate command row: SortSam" in errors
        assert "benchmark-data row 2 missing command" in errors
        assert "benchmark-data row 3 must be an object" in errors

        commands, errors = verify_benchmark_suite_coverage.manifest_benchmark_commands_with_errors(
            {"benchmarks": "not-a-list"}
        )
        assert commands == set()
        assert errors == ["benchmark-data benchmarks must be a list"]

    def test_exemption_requires_reason_and_docs(self) -> None:
        errors = verify_benchmark_suite_coverage.validate_benchmark_suite_coverage(
            matrix_commands={"LiftoverVcf"},
            suite_commands=set(),
            manifest_commands=set(),
            benchmark_exemptions={"LiftoverVcf": "todo"},
            benchmark_docs="LiftoverVcf",
            minimum_benchmark_count=0,
        )

        assert "benchmark exemption for LiftoverVcf has no useful reason" in errors
        assert "benchmark docs missing exemption reason for LiftoverVcf" in errors

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

        assert "benchmark exemption also appears in suite: IntervalListTools" in errors

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

        assert "matrix native command missing benchmark or exemption: BuildBamIndex" in errors
        assert "benchmark suite covers 1 commands, below required minimum 2" in errors


if __name__ == "__main__":
    unittest.main()
