from __future__ import annotations

import unittest

import pytest

from tools import bench_validation_memory as bench


class MemoryBenchmarkTests(unittest.TestCase):
    def test_summaries_preserve_raw_measurements_and_correct_ratios(self) -> None:
        result: dict[str, list[bench.Measurement]] = {
            "baseline": [{"digest": "same", "seconds": 4.0, "peak_rss_bytes": 800}],
            "candidate": [{"digest": "same", "seconds": 2.0, "peak_rss_bytes": 200}],
        }
        summary = bench.summarize(result)
        assert summary["peak_rss_ratio_baseline_over_candidate"] == 4.0
        assert summary["time_ratio_baseline_over_candidate"] == 2.0
        assert summary["measurements"] == result

    def test_mismatch_empty_and_nonfinite_cannot_be_promoted(self) -> None:
        good = bench.Measurement(digest="same", seconds=1.0, peak_rss_bytes=200)
        bad_rows = (
            ("digest mismatch", bench.Measurement(digest="different", seconds=1.0, peak_rss_bytes=200)),
            ("positive and finite", bench.Measurement(digest="same", seconds=float("nan"), peak_rss_bytes=200)),
            ("positive and finite", bench.Measurement(digest="same", seconds=0, peak_rss_bytes=200)),
            ("positive and finite", bench.Measurement(digest="same", seconds=1.0, peak_rss_bytes=0)),
            ("positive and finite", bench.Measurement(digest="same", seconds=1.0, peak_rss_bytes=float("inf"))),
            ("positive and finite", bench.Measurement(digest="same", seconds=1.0, peak_rss_bytes=float("nan"))),
        )
        for message, bad in bad_rows:
            with pytest.raises(ValueError, match=message):
                bench.summarize({"baseline": [good], "candidate": [bad]})
        with pytest.raises(ValueError, match="completed measurements"):
            bench.summarize({"baseline": [], "candidate": []})


if __name__ == "__main__":
    unittest.main()
