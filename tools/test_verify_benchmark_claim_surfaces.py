#!/usr/bin/env python3
"""Tests for release-facing benchmark claim synchronization."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_benchmark_claim_surfaces.py")
SPEC = importlib.util.spec_from_file_location(
    "verify_benchmark_claim_surfaces", MODULE_PATH
)
assert SPEC is not None
verify_benchmark_claim_surfaces = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_benchmark_claim_surfaces"] = verify_benchmark_claim_surfaces
SPEC.loader.exec_module(verify_benchmark_claim_surfaces)


DATA = {
    "parity": "2/2 PASS",
    "summary": {
        "top_speedup": 12.34,
        "top_command": "FastCommand",
        "floor_speedup": 5.67,
        "floor_command": "SlowCommand",
        "median_speedup": 8.90,
        "geometric_mean_speedup": 8.36,
    },
}


class BenchmarkClaimSurfaceTests(unittest.TestCase):
    def test_all_surface_kinds_accept_current_claims(self) -> None:
        surfaces = {
            path: "12.34x 5.67x 8.36x"
            for path in verify_benchmark_claim_surfaces.CLAIM_SURFACES
        }
        surfaces["packaging/bioconda/BIOCONDA_PR.md"] = (
            "Parity: 2/2 PASS. Geometric mean speedup: 8.36x. "
            "Median speedup: 8.90x. Slowest saved speedup: 5.67x on SlowCommand. "
            "Fastest saved speedup: 12.34x on FastCommand."
        )

        errors = verify_benchmark_claim_surfaces.validate_claim_surfaces(surfaces, DATA)

        self.assertEqual(errors, [])

    def test_reports_missing_surface_and_claim(self) -> None:
        surfaces = {
            "packaging/outreach/nf-core-slack.md": "8.36x",
        }

        errors = verify_benchmark_claim_surfaces.validate_claim_surfaces(surfaces, DATA)

        self.assertIn("missing benchmark claim surface: README.md", errors)
        self.assertIn(
            "packaging/outreach/nf-core-slack.md missing current benchmark claim: 12.34x",
            errors,
        )


if __name__ == "__main__":
    unittest.main()
