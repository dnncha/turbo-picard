from __future__ import annotations

import platform
import unittest
from typing import TYPE_CHECKING
from unittest.mock import patch

import pytest

if TYPE_CHECKING:
    from tools import bench_fragment_filter as bench
else:
    import bench_fragment_filter as bench


def cpu_run(cpu_seconds: float | None) -> bench.RunResult:
    return {
        "wall_seconds": None,
        "cpu_seconds": cpu_seconds,
        "peak_rss_bytes": None,
        "bam_sha256": "",
        "metrics_sha256": "",
    }


class FragmentFilterHarnessTests(unittest.TestCase):
    def test_sam_lines_are_coordinate_sorted_with_expected_counts(self) -> None:
        lines = bench.sam_lines(pairs=3, unpaired_loci=2)
        headers = [line for line in lines if line.startswith("@")]
        records = [line for line in lines if not line.startswith("@")]
        assert len(headers) == 3
        assert len(records) == 2 * 3 + 2 * (1 + 2 * bench.PAIRS_PER_TAINTED)
        positions = [int(line.split("\t")[3]) for line in records]
        assert positions == sorted(positions)

    def test_tainted_pairs_share_the_candidate_position(self) -> None:
        lines = bench.sam_lines(pairs=1, unpaired_loci=1)
        records = [line.split("\t") for line in lines if not line.startswith("@")]
        candidate = next(fields for fields in records if fields[0].startswith("unpaired"))
        colliding = [fields for fields in records if fields[0].startswith("coll") and int(fields[1]) == 99]
        assert len(colliding) == bench.PAIRS_PER_TAINTED
        assert all(fields[3] == candidate[3] for fields in colliding), (
            "colliding pairs must start at the unpaired candidate position"
        )

    def test_parse_peak_rss_handles_macos_and_linux_formats(self) -> None:
        macos = "        1.50 real         1.80 user         0.55 sys\n          123456789  maximum resident set size\n"
        linux = "\tMaximum resident set size (kbytes): 1024\n"
        with patch.object(platform, "system", return_value="Darwin"):
            assert bench.parse_peak_rss(macos) == 123456789
        with patch.object(platform, "system", return_value="Linux"):
            assert bench.parse_peak_rss(linux) == 1024 * 1024

    def test_parse_cpu_seconds_handles_both_formats(self) -> None:
        macos = "  0.97 real  1.21 user  0.14 sys\n"
        linux = "\tUser time (seconds): 1.21\n\tSystem time (seconds): 0.14\n"
        assert bench.parse_cpu_seconds(macos) == pytest.approx(1.35)
        assert bench.parse_cpu_seconds(linux) == pytest.approx(1.35)
        assert bench.parse_cpu_seconds("no timings\n") is None

    def test_parse_wall_seconds_reads_real_line(self) -> None:
        assert bench.parse_wall_seconds("  1.08 real  1.52 user  0.36 sys\n") == 1.08
        assert bench.parse_wall_seconds("no timing here\n") is None

    def test_parse_wall_seconds_reads_gnu_verbose_elapsed(self) -> None:
        assert bench.parse_wall_seconds("\tElapsed (wall clock) time (h:mm:ss or m:ss): 0:01.25\n") == 1.25
        assert bench.parse_wall_seconds("\tElapsed (wall clock) time (h:mm:ss or m:ss): 1:02:03\n") == 3723.0

    def test_summarize_reports_median_and_digest_sets(self) -> None:
        runs: list[bench.RunResult] = [
            {"wall_seconds": 2.0, "cpu_seconds": 2.0, "peak_rss_bytes": 200, "bam_sha256": "a", "metrics_sha256": "m"},
            {"wall_seconds": 1.0, "cpu_seconds": 1.0, "peak_rss_bytes": 100, "bam_sha256": "a", "metrics_sha256": "m"},
            {"wall_seconds": 3.0, "cpu_seconds": 3.0, "peak_rss_bytes": 300, "bam_sha256": "a", "metrics_sha256": "m"},
        ]
        summary = bench.summarize("base", runs)
        assert summary["median_wall_seconds"] == 2.0
        assert summary["median_cpu_seconds"] == 2.0
        assert summary["median_peak_rss_bytes"] == 200
        assert summary["bam_sha256"] == ["a"]

    def test_paired_ratio_cancels_shared_drift(self) -> None:
        base = [cpu_run(2.0), cpu_run(3.0), cpu_run(4.0)]
        candidate = [cpu_run(1.0), cpu_run(1.5), cpu_run(2.0)]
        assert bench.paired_ratio(base, candidate, "cpu_seconds") == 2.0

    def test_paired_ratio_skips_missing_values(self) -> None:
        base = [cpu_run(2.0), cpu_run(None)]
        candidate = [cpu_run(1.0), cpu_run(1.0)]
        assert bench.paired_ratio(base, candidate, "cpu_seconds") == 2.0

    def test_rejects_nonpositive_inputs(self) -> None:
        with pytest.raises(ValueError, match="must be positive"):
            bench.sam_lines(pairs=0, unpaired_loci=1)
        with pytest.raises(ValueError, match="must be positive"):
            bench.sam_lines(pairs=1, unpaired_loci=0)


if __name__ == "__main__":
    unittest.main()
