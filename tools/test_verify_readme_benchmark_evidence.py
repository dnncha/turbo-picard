#!/usr/bin/env python3
"""Tests for README benchmark evidence consistency checks."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_readme_benchmark_evidence.py")
SPEC = importlib.util.spec_from_file_location("verify_readme_benchmark_evidence", MODULE_PATH)
assert SPEC is not None
verify_readme_benchmark_evidence = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_readme_benchmark_evidence"] = verify_readme_benchmark_evidence
SPEC.loader.exec_module(verify_readme_benchmark_evidence)


class ReadmeBenchmarkEvidenceTests(unittest.TestCase):
    def test_readme_claims_match_benchmark_manifest(self) -> None:
        data = {
            "date": "2026-05-30",
            "source": "python3 tools/bench_suite.py --repeats 1 --skip-build",
            "source_artifact": "docs/site/assets/bench-suite-output.txt",
            "parity": "2/2 PASS",
            "summary": {
                "top_command": "FastCommand",
                "top_speedup": 12.34,
                "floor_command": "SlowCommand",
                "floor_speedup": 5.67,
                "median_speedup": 8.9,
                "geometric_mean_speedup": 8.36,
            },
            "benchmarks": [
                {"command": "FastCommand", "speedup": 12.34, "parity": "PASS"},
                {"command": "SlowCommand", "speedup": 5.67, "parity": "PASS"},
            ],
        }
        readme = """
- `2/2` benchmarked commands passed parity checks.
- `12.34x` top speedup: `FastCommand`.
- `5.67x` floor speedup: `SlowCommand`.
- `8.90x` median speedup.
- `8.36x` geometric mean speedup.
Saved on `2026-05-30` from `python3 tools/bench_suite.py --repeats 1 --skip-build`; raw log: `docs/site/assets/bench-suite-output.txt`.

| Command | Speedup vs Picard | Parity |
| --- | ---: | --- |
| FastCommand | 12.34x | PASS |
| SlowCommand | 5.67x | PASS |

python3 tools/verify_benchmark_log_evidence.py
python3 tools/verify_benchmark_suite_coverage.py
python3 tools/verify_benchmark_thresholds.py
python3 tools/verify_real_data_evidence.py --release-ready
benchmark exceptions
AccelerationStatus
doctor
explain
trial
CollectHsMetrics
IntervalListTools
LiftoverVcf
https://turbo-picard.readthedocs.io/en/latest/adoption.html
https://turbo-picard.readthedocs.io/en/latest/benchmarks.html
https://turbo-picard.readthedocs.io/en/latest/citation.html
CITATION.cff
SHA-256
"""

        errors = verify_readme_benchmark_evidence.validate_readme_benchmark_evidence(
            readme, data
        )

        self.assertEqual(errors, [])

    def test_readme_claim_mismatches_are_reported(self) -> None:
        data = {
            "date": "2026-05-30",
            "source": "python3 tools/bench_suite.py --repeats 1 --skip-build",
            "source_artifact": "docs/site/assets/bench-suite-output.txt",
            "parity": "1/1 PASS",
            "summary": {
                "top_command": "RealCommand",
                "top_speedup": 11.0,
                "floor_command": "RealCommand",
                "floor_speedup": 11.0,
                "median_speedup": 11.0,
                "geometric_mean_speedup": 11.0,
            },
            "benchmarks": [
                {"command": "RealCommand", "speedup": 11.0, "parity": "PASS"},
            ],
        }

        errors = verify_readme_benchmark_evidence.validate_readme_benchmark_evidence(
            "| Command | Speedup vs Picard | Parity |\n| --- | ---: | --- |\n",
            data,
        )

        self.assertIn("missing README parity claim: `1/1`", errors)
        self.assertIn("missing README benchmark table row: RealCommand 11.00x PASS", errors)
        self.assertIn(
            "missing README benchmark-log evidence verifier command",
            errors,
        )
        self.assertIn(
            "missing README benchmark-suite coverage verifier command",
            errors,
        )
        self.assertIn(
            "missing README benchmark-threshold verifier command",
            errors,
        )
        self.assertIn(
            "missing README release-ready real-data verifier command",
            errors,
        )
        self.assertIn("missing README adoption guide link", errors)
        self.assertIn("missing README benchmark documentation link", errors)
        self.assertIn("missing README citation documentation link", errors)
        self.assertIn("missing README software citation pointer", errors)
        self.assertIn("missing README pinned input SHA-256 guidance", errors)
        self.assertIn("missing README benchmark date", errors)
        self.assertIn("missing README benchmark source command", errors)
        self.assertIn("missing README raw benchmark artifact path", errors)

    def test_benchmarks_readme_claims_match_benchmark_manifest(self) -> None:
        data = {
            "summary": {
                "floor_speedup": 5.67,
                "geometric_mean_speedup": 8.36,
                "top_speedup": 12.34,
            }
        }
        benchmarks_readme = """
The saved run reports a `5.67x` floor speedup, an `8.36x` geometric mean speedup,
and a `12.34x` top speedup. This is fixture-specific evidence, not a capacity
or production-scale guarantee.
"""

        errors = verify_readme_benchmark_evidence.validate_benchmarks_readme_evidence(
            benchmarks_readme, data
        )

        self.assertEqual(errors, [])

    def test_benchmarks_readme_claim_mismatches_are_reported(self) -> None:
        data = {
            "summary": {
                "floor_speedup": 5.67,
                "geometric_mean_speedup": 8.36,
                "top_speedup": 12.34,
            }
        }

        errors = verify_readme_benchmark_evidence.validate_benchmarks_readme_evidence(
            "8.55x floor speedup", data
        )

        self.assertIn("missing benchmarks README floor-speedup claim", errors)
        self.assertIn("missing benchmarks README geometric-mean claim", errors)
        self.assertIn("missing benchmarks README top-speedup claim", errors)
        self.assertIn("missing benchmarks README fixture-specific evidence caveat", errors)
        self.assertIn("missing benchmarks README capacity caveat", errors)


if __name__ == "__main__":
    unittest.main()
