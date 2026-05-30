#!/usr/bin/env python3
"""Tests for marketing-site benchmark evidence consistency checks."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_site_benchmark_evidence.py")
SPEC = importlib.util.spec_from_file_location("verify_site_benchmark_evidence", MODULE_PATH)
assert SPEC is not None
verify_site_benchmark_evidence = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_site_benchmark_evidence"] = verify_site_benchmark_evidence
SPEC.loader.exec_module(verify_site_benchmark_evidence)


class SiteBenchmarkEvidenceTests(unittest.TestCase):
    def test_site_claims_match_benchmark_manifest(self) -> None:
        data = {
            "parity": "2/2 PASS",
            "source_artifact": "docs/site/assets/bench-suite-output.txt",
            "summary": {
                "command_count": 2,
                "top_command": "FastCommand",
                "top_speedup": 12.34,
                "floor_command": "SlowCommand",
                "floor_speedup": 5.67,
                "geometric_mean_speedup": 8.36,
            },
        }
        site = """
<strong>2/2</strong>
<strong>12.34x</strong>
<strong>5.67x</strong>
<div class="benchmark-highlight"><b>2 commands</b></div>
<div class="benchmark-highlight"><b>12.34x</b><span>FastCommand led the current local benchmark run.</span></div>
<div class="benchmark-highlight"><b>5.67x</b><span>SlowCommand is the current floor after its default-unmapped fast path.</span></div>
<div class="benchmark-highlight"><b>8.36x</b></div>
<a href="assets/benchmark-data.json">Open evidence JSON</a>
<a href="assets/bench-suite-output.txt">Open raw suite log</a>
<code>python3 tools/verify_benchmark_log_evidence.py</code>
"""

        errors = verify_site_benchmark_evidence.validate_site_benchmark_evidence(
            site, data
        )

        self.assertEqual(errors, [])

    def test_missing_site_claims_are_reported(self) -> None:
        data = {
            "parity": "1/1 PASS",
            "source_artifact": "docs/site/assets/bench-suite-output.txt",
            "summary": {
                "command_count": 1,
                "top_command": "RealCommand",
                "top_speedup": 11.0,
                "floor_command": "RealCommand",
                "floor_speedup": 11.0,
                "geometric_mean_speedup": 11.0,
            },
        }

        errors = verify_site_benchmark_evidence.validate_site_benchmark_evidence(
            "<html></html>", data
        )

        self.assertIn("missing site parity claim: 1/1", errors)
        self.assertIn("missing site raw suite log link: assets/bench-suite-output.txt", errors)
        self.assertIn(
            "missing site benchmark-log evidence verifier command",
            errors,
        )


if __name__ == "__main__":
    unittest.main()
