#!/usr/bin/env python3
"""Tests for raw benchmark log to rendered manifest consistency checks."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_benchmark_log_evidence.py")
SPEC = importlib.util.spec_from_file_location("verify_benchmark_log_evidence", MODULE_PATH)
assert SPEC is not None
verify_benchmark_log_evidence = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_benchmark_log_evidence"] = verify_benchmark_log_evidence
SPEC.loader.exec_module(verify_benchmark_log_evidence)


class BenchmarkLogEvidenceTests(unittest.TestCase):
    def test_manifest_matches_raw_suite_log(self) -> None:
        suite_output = "\n".join(
            [
                "benchmark_date=2026-05-30 source=python3 tools/bench_suite.py --repeats 1 --skip-build",
                "command=SortSam reads=100000 runs=1 median_turbo_seconds=0.100000 "
                "median_picard_seconds=2.000000 median_speedup=20.00x "
                "best_speedup=20.00x parity=PASS",
                "command=BuildBamIndex reads=50000 runs=1 median_turbo_seconds=0.050000 "
                "median_picard_seconds=2.000000 median_speedup=40.00x "
                "best_speedup=40.00x parity=PASS",
            ]
        )
        manifest = {
            "source": "python3 tools/bench_suite.py --repeats 1 --skip-build",
            "date": "2026-05-30",
            "parity": "2/2 PASS",
            "summary": {
                "command_count": 2,
                "parity_pass_count": 2,
                "top_speedup": 40.0,
                "top_command": "BuildBamIndex",
                "floor_speedup": 20.0,
                "floor_command": "SortSam",
                "median_speedup": 40.0,
                "geometric_mean_speedup": 28.28,
            },
            "benchmarks": [
                {"rank": 1, "command": "BuildBamIndex", "speedup": 40.0, "parity": "PASS"},
                {"rank": 2, "command": "SortSam", "speedup": 20.0, "parity": "PASS"},
            ],
            "source_artifact": "docs/site/assets/bench-suite-output.txt",
        }

        errors = verify_benchmark_log_evidence.validate_benchmark_log_evidence(
            suite_output,
            manifest,
            source_artifact="docs/site/assets/bench-suite-output.txt",
        )

        self.assertEqual(errors, [])

    def test_missing_metadata_is_reported(self) -> None:
        suite_output = (
            "command=SortSam reads=100000 runs=1 median_turbo_seconds=0.100000 "
            "median_picard_seconds=2.000000 median_speedup=20.00x "
            "best_speedup=20.00x parity=PASS"
        )
        manifest = {
            "source": "python3 tools/bench_suite.py --repeats 1 --skip-build",
            "date": "2026-05-30",
            "parity": "1/1 PASS",
            "summary": {
                "command_count": 1,
                "parity_pass_count": 1,
                "top_speedup": 20.0,
                "top_command": "SortSam",
                "floor_speedup": 20.0,
                "floor_command": "SortSam",
                "median_speedup": 20.0,
                "geometric_mean_speedup": 20.0,
            },
            "benchmarks": [
                {"rank": 1, "command": "SortSam", "speedup": 20.0, "parity": "PASS"},
            ],
            "source_artifact": "docs/site/assets/bench-suite-output.txt",
        }

        errors = verify_benchmark_log_evidence.validate_benchmark_log_evidence(
            suite_output,
            manifest,
            source_artifact="docs/site/assets/bench-suite-output.txt",
        )

        self.assertIn("raw benchmark log is missing benchmark_date metadata", errors)
        self.assertIn("raw benchmark log is missing source metadata", errors)

    def test_metadata_and_source_artifact_must_be_release_evidence_shaped(self) -> None:
        suite_output = "\n".join(
            [
                "benchmark_date=2026/05/30 source=bash run-something.sh",
                "command=SortSam reads=100000 runs=1 median_turbo_seconds=0.100000 "
                "median_picard_seconds=2.000000 median_speedup=20.00x "
                "best_speedup=20.00x parity=PASS",
            ]
        )
        manifest = {
            "source": "bash run-something.sh",
            "date": "2026/05/30",
            "parity": "1/1 PASS",
            "summary": {
                "command_count": 1,
                "parity_pass_count": 1,
                "top_speedup": 20.0,
                "top_command": "SortSam",
                "floor_speedup": 20.0,
                "floor_command": "SortSam",
                "median_speedup": 20.0,
                "geometric_mean_speedup": 20.0,
            },
            "benchmarks": [
                {"rank": 1, "command": "SortSam", "speedup": 20.0, "parity": "PASS"},
            ],
            "source_artifact": "../bench-suite-output.txt",
        }

        errors = verify_benchmark_log_evidence.validate_benchmark_log_evidence(
            suite_output,
            manifest,
            source_artifact="../bench-suite-output.txt",
        )

        self.assertIn(
            "raw benchmark log has non-ISO benchmark_date: 2026/05/30",
            errors,
        )
        self.assertIn(
            "raw benchmark log source must start with python3 tools/bench_suite.py: bash run-something.sh",
            errors,
        )
        self.assertIn(
            "benchmark source_artifact must be repository-relative: ../bench-suite-output.txt",
            errors,
        )

    def test_source_artifact_must_exist_under_site_assets(self) -> None:
        suite_output = "\n".join(
            [
                "benchmark_date=2026-05-30 source=python3 tools/bench_suite.py --repeats 1 --skip-build",
                "command=SortSam reads=100000 runs=1 median_turbo_seconds=0.100000 "
                "median_picard_seconds=2.000000 median_speedup=20.00x "
                "best_speedup=20.00x parity=PASS",
            ]
        )
        manifest = {
            "source": "python3 tools/bench_suite.py --repeats 1 --skip-build",
            "date": "2026-05-30",
            "parity": "1/1 PASS",
            "summary": {
                "command_count": 1,
                "parity_pass_count": 1,
                "top_speedup": 20.0,
                "top_command": "SortSam",
                "floor_speedup": 20.0,
                "floor_command": "SortSam",
                "median_speedup": 20.0,
                "geometric_mean_speedup": 20.0,
            },
            "benchmarks": [
                {"rank": 1, "command": "SortSam", "speedup": 20.0, "parity": "PASS"},
            ],
            "source_artifact": "docs/bench-suite-output.txt",
        }

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "docs" / "site" / "assets").mkdir(parents=True)
            old_root = verify_benchmark_log_evidence.ROOT
            try:
                verify_benchmark_log_evidence.ROOT = root
                outside_errors = verify_benchmark_log_evidence.validate_benchmark_log_evidence(
                    suite_output,
                    manifest,
                    source_artifact="docs/bench-suite-output.txt",
                )
                missing_errors = verify_benchmark_log_evidence.validate_benchmark_log_evidence(
                    suite_output,
                    {**manifest, "source_artifact": "docs/site/assets/missing.txt"},
                    source_artifact="docs/site/assets/missing.txt",
                )
                (root / "docs" / "site" / "assets" / "bench-suite-output.txt").write_text(
                    suite_output,
                    encoding="utf-8",
                )
                ok_errors = verify_benchmark_log_evidence.validate_benchmark_log_evidence(
                    suite_output,
                    {**manifest, "source_artifact": "docs/site/assets/bench-suite-output.txt"},
                    source_artifact="docs/site/assets/bench-suite-output.txt",
                )
            finally:
                verify_benchmark_log_evidence.ROOT = old_root

        self.assertIn(
            "benchmark source_artifact must stay under docs/site/assets: docs/bench-suite-output.txt",
            outside_errors,
        )
        self.assertIn(
            "benchmark source_artifact is missing: docs/site/assets/missing.txt",
            missing_errors,
        )
        self.assertEqual(ok_errors, [])

    def test_manifest_drift_is_reported(self) -> None:
        suite_output = "\n".join(
            [
                "benchmark_date=2026-05-30 source=python3 tools/bench_suite.py --repeats 1 --skip-build",
                "command=SortSam reads=100000 runs=1 median_turbo_seconds=0.100000 "
                "median_picard_seconds=2.000000 median_speedup=20.00x "
                "best_speedup=20.00x parity=PASS",
            ]
        )
        manifest = {
            "source": "old command",
            "date": "2026-05-29",
            "parity": "1/1 PASS",
            "summary": {
                "command_count": 1,
                "parity_pass_count": 1,
                "top_speedup": 19.0,
                "top_command": "SortSam",
                "floor_speedup": 19.0,
                "floor_command": "SortSam",
                "median_speedup": 19.0,
                "geometric_mean_speedup": 19.0,
            },
            "benchmarks": [
                {"rank": 1, "command": "SortSam", "speedup": 19.0, "parity": "PASS"},
            ],
            "source_artifact": "docs/site/assets/bench-suite-output.txt",
        }

        errors = verify_benchmark_log_evidence.validate_benchmark_log_evidence(
            suite_output,
            manifest,
            source_artifact="docs/site/assets/bench-suite-output.txt",
        )

        self.assertIn("manifest field date is '2026-05-29', expected '2026-05-30'", errors)
        self.assertIn("manifest field source is 'old command', expected 'python3 tools/bench_suite.py --repeats 1 --skip-build'", errors)
        self.assertIn("manifest summary top_speedup is 19.0, expected 20.0", errors)
        self.assertIn("manifest benchmark SortSam speedup is 19.0, expected 20.0", errors)

    def test_malformed_manifest_benchmark_rows_are_reported(self) -> None:
        suite_output = "\n".join(
            [
                "benchmark_date=2026-05-30 source=python3 tools/bench_suite.py --repeats 1 --skip-build",
                "command=SortSam reads=100000 runs=1 median_turbo_seconds=0.100000 "
                "median_picard_seconds=2.000000 median_speedup=20.00x "
                "best_speedup=20.00x parity=PASS",
            ]
        )
        manifest = {
            "source": "python3 tools/bench_suite.py --repeats 1 --skip-build",
            "date": "2026-05-30",
            "parity": "1/1 PASS",
            "summary": {
                "command_count": 1,
                "parity_pass_count": 1,
                "top_speedup": 20.0,
                "top_command": "SortSam",
                "floor_speedup": 20.0,
                "floor_command": "SortSam",
                "median_speedup": 20.0,
                "geometric_mean_speedup": 20.0,
            },
            "benchmarks": [
                "not-a-row",
                {"rank": 1, "speedup": 20.0, "parity": "PASS"},
                {"rank": 1, "command": "SortSam", "speedup": 20.0, "parity": "PASS"},
                {"rank": 2, "command": "SortSam", "speedup": 19.0, "parity": "PASS"},
            ],
            "source_artifact": "docs/site/assets/bench-suite-output.txt",
        }

        errors = verify_benchmark_log_evidence.validate_benchmark_log_evidence(
            suite_output,
            manifest,
            source_artifact="docs/site/assets/bench-suite-output.txt",
        )

        self.assertIn("manifest benchmark row 0 must be an object", errors)
        self.assertIn("manifest benchmark row 1 missing command", errors)
        self.assertIn("manifest has duplicate benchmark command: SortSam", errors)


if __name__ == "__main__":
    unittest.main()
