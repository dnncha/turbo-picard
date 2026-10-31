#!/usr/bin/env python3
from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AUDIT = ROOT / "tools" / "audit_real_data.py"


class AuditRealDataTests(unittest.TestCase):
    def test_help_exits_zero(self) -> None:
        completed = subprocess.run(
            [str(AUDIT), "--help"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        assert completed.returncode == 0
        assert "production audit" in completed.stdout.lower()
        assert "markduplicates-arg" in completed.stdout

    def test_missing_compare_input_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            completed = subprocess.run(
                [
                    str(AUDIT),
                    "--input-bam",
                    str(ROOT / "fixtures/markduplicates/paired/input.bam"),
                    "--output-dir",
                    tempdir,
                    "--dataset-id",
                    "audit-smoke",
                    "--input-source-url",
                    "https://example.org/input.bam",
                    "--input-source-commit",
                    "0" * 40,
                    "--skip-build",
                    "--picard-command",
                    "false",
                    "--turbo-picard-command",
                    "false",
                ],
                cwd=ROOT,
                check=False,
                capture_output=True,
                text=True,
            )
            assert completed.returncode != 0


if __name__ == "__main__":
    unittest.main()
