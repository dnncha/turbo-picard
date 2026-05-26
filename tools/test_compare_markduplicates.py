#!/usr/bin/env python3
"""Tests for the MarkDuplicates semantic comparison harness."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("compare_markduplicates.py")
SPEC = importlib.util.spec_from_file_location("compare_markduplicates", MODULE_PATH)
assert SPEC is not None
compare_markduplicates = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["compare_markduplicates"] = compare_markduplicates
SPEC.loader.exec_module(compare_markduplicates)


class MetricsParsingTests(unittest.TestCase):
    def test_reads_duplication_metrics_block_after_comments(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            metrics = Path(tempdir) / "metrics.txt"
            metrics.write_text(
                "\n".join(
                    [
                        "## htsjdk.samtools.metrics.StringHeader",
                        "# command",
                        "",
                        "## METRICS CLASS\tpicard.sam.DuplicationMetrics",
                        "LIBRARY\tUNPAIRED_READS_EXAMINED",
                        "lib1\t7",
                        "",
                        "## HISTOGRAM\tjava.lang.Double",
                        "BIN\tCoverageMult",
                    ]
                ),
                encoding="utf-8",
            )

            rows = compare_markduplicates.read_metrics(metrics)

        self.assertEqual(
            rows,
            [["LIBRARY", "UNPAIRED_READS_EXAMINED"], ["lib1", "7"]],
        )

    def test_missing_duplication_metrics_block_is_empty(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            metrics = Path(tempdir) / "metrics.txt"
            metrics.write_text(
                "## htsjdk.samtools.metrics.StringHeader\n# no duplication block\n",
                encoding="utf-8",
            )

            rows = compare_markduplicates.read_metrics(metrics)

        self.assertEqual(rows, [])

    def test_missing_metrics_block_is_a_difference(self) -> None:
        differences = compare_markduplicates.metric_differences(
            [],
            [["LIBRARY"], ["lib1"]],
            Path("picard.metrics.txt"),
            Path("turbo-picard.metrics.txt"),
        )

        self.assertEqual(
            differences,
            [
                "Picard metrics file has no picard.sam.DuplicationMetrics block: "
                "picard.metrics.txt"
            ],
        )


if __name__ == "__main__":
    unittest.main()
