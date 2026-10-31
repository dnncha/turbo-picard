#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

import pytest

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

        assert data["source_artifact"] == "docs/site/assets/bench-suite-output.txt"
        assert data["date"] == "2026-08-14"
        assert data["parity"] == "32/32 PASS"
        assert data["summary"]["command_count"] == 32
        assert data["summary"]["parity_pass_count"] == 32
        assert data["summary"]["top_command"] == "NormalizeFasta"
        assert data["summary"]["top_speedup"] == 272.12
        assert data["summary"]["floor_command"] == "SetNmMdAndUqTags"
        assert data["summary"]["floor_speedup"] == 22.88
        assert data["summary"]["median_speedup"] == 99.51
        assert data["summary"]["geometric_mean_speedup"] == 84.52

        ranks = [row["rank"] for row in data["benchmarks"]]
        speedups = [row["speedup"] for row in data["benchmarks"]]
        parities = {row["parity"] for row in data["benchmarks"]}

        assert ranks == list(range(1, 33))
        assert speedups == sorted(speedups, reverse=True)
        assert parities == {"PASS"}

    def test_even_count_uses_mean_of_both_middle_values(self) -> None:
        data = render_benchmark_assets.build_benchmark_data_from_rows(
            [{"command": str(i), "speedup": x, "parity": "PASS"} for i, x in enumerate([1.0, 2.0, 4.0, 100.0])],
            source="test",
            date="2026-09-05",
        )
        assert data["summary"]["median_speedup"] == 3.0

    def test_nonfinite_performance_evidence_is_rejected(self) -> None:
        for value in [float("nan"), float("inf"), -float("inf"), 0.0, -1.0]:
            for key in ["speedup", "median_turbo_seconds", "median_picard_seconds"]:
                with self.subTest(value=value, key=key), pytest.raises(ValueError, match="positive and finite"):
                    render_benchmark_assets.build_benchmark_data_from_rows(
                        [{"command": "SortSam", "speedup": 2.0, "parity": "PASS", key: value}],
                        source="test",
                        date="2026-09-05",
                    )

    def test_saved_log_retains_absolute_times_and_workload_parameter(self) -> None:
        data = render_benchmark_assets.build_benchmark_data()
        row = next(row for row in data["benchmarks"] if row["command"] == "MarkDuplicates")
        assert row["median_turbo_seconds"] == 0.075464
        assert row["median_picard_seconds"] == 2.238986
        assert row["workload_parameter"] == {"name": "reads", "value": 50000}
        assert row["runs"] == 3
        assert row["speedup"] == 28.7

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

        assert data["date"] == "2026-05-30"
        assert data["parity"] == "3/3 PASS"
        assert data["summary"]["command_count"] == 3
        assert data["summary"]["parity_pass_count"] == 3
        assert data["summary"]["top_command"] == "BuildBamIndex"
        assert data["summary"]["top_speedup"] == 40.0
        assert data["summary"]["floor_command"] == "RevertSam"
        assert data["summary"]["floor_speedup"] == 8.0
        assert data["summary"]["median_speedup"] == 20.0
        assert data["summary"]["geometric_mean_speedup"] == 18.57
        assert [row["command"] for row in data["benchmarks"]] == ["BuildBamIndex", "SortSam", "RevertSam"]

    def test_manifest_rejects_malformed_or_duplicate_rows(self) -> None:
        with pytest.raises(ValueError, match="duplicate benchmark command: SortSam"):
            render_benchmark_assets.build_benchmark_data_from_rows(
                [
                    {"command": "SortSam", "speedup": 20.0, "parity": "PASS"},
                    {"command": "SortSam", "speedup": 19.0, "parity": "PASS"},
                ],
                source="synthetic suite output",
                date="2026-05-30",
            )
        with pytest.raises(ValueError, match="benchmark row 0 missing command"):
            render_benchmark_assets.build_benchmark_data_from_rows(
                [{"speedup": 20.0, "parity": "PASS"}],
                source="synthetic suite output",
                date="2026-05-30",
            )
        with pytest.raises(ValueError, match="benchmark row SortSam has invalid parity"):
            render_benchmark_assets.build_benchmark_data_from_rows(
                [{"command": "SortSam", "speedup": 20.0, "parity": "MAYBE"}],
                source="synthetic suite output",
                date="2026-05-30",
            )
        with pytest.raises(ValueError, match="benchmark row SortSam speedup must be positive"):
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

        assert data["date"] == "2026-05-30"
        assert data["source"] == "synthetic benchmark command"

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

        assert "TinyFastPath" in svg
        assert "MassiveFastPath" in svg
        assert "2/2 parity checks passing" in svg
        assert "from 12.50x to 50.00x" in svg
        assert "in the saved benchmark suite" in svg
        assert "CollectInsertSizeMetrics" not in svg


if __name__ == "__main__":
    unittest.main()
