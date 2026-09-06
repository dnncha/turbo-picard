"""Guard evidence summaries against false parity and invalid measurements."""
from __future__ import annotations
import unittest
from tools import bench_validation_memory as bench

class MemoryBenchmarkTests(unittest.TestCase):
    def test_summaries_preserve_raw_measurements_and_correct_ratios(self):
        result = {"baseline": [{"digest": "same", "seconds": 4.0, "peak_rss_bytes": 800}],
                  "candidate": [{"digest": "same", "seconds": 2.0, "peak_rss_bytes": 200}]}
        summary = bench.summarize(result)
        self.assertEqual(summary["peak_rss_ratio_baseline_over_candidate"], 4.0)
        self.assertEqual(summary["time_ratio_baseline_over_candidate"], 2.0)
        self.assertEqual(summary["measurements"], result)

    def test_mismatch_empty_and_nonfinite_cannot_be_promoted(self):
        good = {"digest": "same", "seconds": 1.0, "peak_rss_bytes": 200}
        for bad in (dict(good, digest="different"), dict(good, seconds=float("nan")),
                    dict(good, seconds=0), dict(good, peak_rss_bytes=0),
                    dict(good, peak_rss_bytes=float("inf")), dict(good, peak_rss_bytes=float("nan"))):
            with self.assertRaises(ValueError):
                bench.summarize({"baseline": [good], "candidate": [bad]})
        with self.assertRaises(ValueError):
            bench.summarize({"baseline": [], "candidate": []})

if __name__ == "__main__":
    unittest.main()
