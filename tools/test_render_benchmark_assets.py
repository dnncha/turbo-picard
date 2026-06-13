#!/usr/bin/env python3
"""Tests for benchmark evidence asset rendering."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("render_benchmark_assets.py")
SPEC = importlib.util.spec_from_file_location("render_benchmark_assets", MODULE_PATH)
assert SPEC is not None
render_benchmark_assets = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["render_benchmark_assets"] = render_benchmark_assets
SPEC.loader.exec_module(render_benchmark_assets)


class BenchmarkManifestTests(unittest.TestCase):
    def test_manifest_summarizes_ranked_parity_checked_benchmarks(self) -> None:
        data = render_benchmark_assets.build_benchmark_data()

        self.assertEqual(data["source_artifact"], "docs/site/assets/bench-suite-output.txt")
        self.assertEqual(data["date"], "2026-06-13")
        self.assertEqual(data["parity"], "32/32 PASS")
        self.assertEqual(data["summary"]["command_count"], 32)
        self.assertEqual(data["summary"]["parity_pass_count"], 32)
        self.assertEqual(data["summary"]["top_command"], "UpdateVcfSequenceDictionary")
        self.assertEqual(data["summary"]["top_speedup"], 94.36)
        self.assertEqual(data["summary"]["floor_command"], "RevertSam")
        self.assertEqual(data["summary"]["floor_speedup"], 6.86)
        self.assertEqual(data["summary"]["median_speedup"], 26.72)
        self.assertEqual(data["summary"]["geometric_mean_speedup"], 24.94)

        ranks = [row["rank"] for row in data["benchmarks"]]
        speedups = [row["speedup"] for row in data["benchmarks"]]
        parities = {row["parity"] for row in data["benchmarks"]}

        self.assertEqual(ranks, list(range(1, 33)))
        self.assertEqual(speedups, sorted(speedups, reverse=True))
        self.assertEqual(parities, {"PASS"})

    def test_manifest_can_be_built_from_bench_suite_output(self) -> None:
        suite_output = "\n".join(
            [
                "command=SortSam reads=100000 runs=1 median_turbo_seconds=0.100000 "
                "median_picard_seconds=2.000000 median_speedup=20.00x "
                "best_speedup=20.00x parity=PASS",
                "command=RevertSam reads=100000 runs=1 median_turbo_seconds=0.250000 "
                "median_picard_seconds=2.000000 median_speedup=8.00x "
                "best_speedup=8.00x parity=PASS",
                "command=BuildBamIndex reads=50000 runs=1 median_turbo_seconds=0.050000 "
                "median_picard_seconds=2.000000 median_speedup=40.00x "
                "best_speedup=40.00x parity=PASS",
            ]
        )

        data = render_benchmark_assets.build_benchmark_data_from_suite_output(
            suite_output,
            source="python3 tools/bench_suite.py --repeats 1 --skip-build",
            date="2026-05-30",
        )

        self.assertEqual(data["date"], "2026-05-30")
        self.assertEqual(data["parity"], "3/3 PASS")
        self.assertEqual(data["summary"]["command_count"], 3)
        self.assertEqual(data["summary"]["parity_pass_count"], 3)
        self.assertEqual(data["summary"]["top_command"], "BuildBamIndex")
        self.assertEqual(data["summary"]["top_speedup"], 40.0)
        self.assertEqual(data["summary"]["floor_command"], "RevertSam")
        self.assertEqual(data["summary"]["floor_speedup"], 8.0)
        self.assertEqual(data["summary"]["median_speedup"], 20.0)
        self.assertEqual(data["summary"]["geometric_mean_speedup"], 18.57)
        self.assertEqual(
            [row["command"] for row in data["benchmarks"]],
            ["BuildBamIndex", "SortSam", "RevertSam"],
        )

    def test_manifest_rejects_malformed_or_duplicate_rows(self) -> None:
        with self.assertRaisesRegex(ValueError, "duplicate benchmark command: SortSam"):
            render_benchmark_assets.build_benchmark_data_from_rows(
                [
                    {"command": "SortSam", "speedup": 20.0, "parity": "PASS"},
                    {"command": "SortSam", "speedup": 19.0, "parity": "PASS"},
                ],
                source="synthetic suite output",
                date="2026-05-30",
            )
        with self.assertRaisesRegex(ValueError, "benchmark row 0 missing command"):
            render_benchmark_assets.build_benchmark_data_from_rows(
                [{"speedup": 20.0, "parity": "PASS"}],
                source="synthetic suite output",
                date="2026-05-30",
            )
        with self.assertRaisesRegex(ValueError, "benchmark row SortSam has invalid parity"):
            render_benchmark_assets.build_benchmark_data_from_rows(
                [{"command": "SortSam", "speedup": 20.0, "parity": "MAYBE"}],
                source="synthetic suite output",
                date="2026-05-30",
            )
        with self.assertRaisesRegex(ValueError, "benchmark row SortSam speedup must be positive"):
            render_benchmark_assets.build_benchmark_data_from_rows(
                [{"command": "SortSam", "speedup": 0.0, "parity": "PASS"}],
                source="synthetic suite output",
                date="2026-05-30",
            )

    def test_suite_output_metadata_supplies_date_and_source(self) -> None:
        suite_output = "\n".join(
            [
                "benchmark_date=2026-05-30 source=synthetic benchmark command",
                "command=SortSam reads=100000 runs=1 median_turbo_seconds=0.100000 "
                "median_picard_seconds=2.000000 median_speedup=20.00x "
                "best_speedup=20.00x parity=PASS",
            ]
        )

        data = render_benchmark_assets.build_benchmark_data_from_suite_output(suite_output)

        self.assertEqual(data["date"], "2026-05-30")
        self.assertEqual(data["source"], "synthetic benchmark command")

    def test_speedup_chart_uses_manifest_benchmarks(self) -> None:
        data = render_benchmark_assets.build_benchmark_data_from_rows(
            [
                {"command": "TinyFastPath", "speedup": 12.5, "parity": "PASS"},
                {"command": "MassiveFastPath", "speedup": 50.0, "parity": "PASS"},
            ],
            source="synthetic suite output",
            date="2026-05-30",
        )

        svg = render_benchmark_assets.render_speedup_chart(data)

        self.assertIn("TinyFastPath", svg)
        self.assertIn("MassiveFastPath", svg)
        self.assertIn("2/2 parity checks passing", svg)
        self.assertIn("from 12.50x to 50.00x", svg)
        self.assertIn("in the saved benchmark suite", svg)
        self.assertNotIn("CollectInsertSizeMetrics", svg)


if __name__ == "__main__":
    unittest.main()
