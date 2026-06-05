#!/usr/bin/env python3
"""Tests for shared parity comparison helpers."""

from __future__ import annotations

import importlib.util
import shutil
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("parity_compare.py")
SPEC = importlib.util.spec_from_file_location("parity_compare", MODULE_PATH)
assert SPEC is not None
parity_compare = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["parity_compare"] = parity_compare
SPEC.loader.exec_module(parity_compare)


class ParityCompareTests(unittest.TestCase):
    def test_load_metrics_ignores_comment_header(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            path = Path(tempdir) / "metrics.txt"
            path.write_text(
                "# metrics\n"
                "LIBRARY\tUNPAIRED_READS_EXAMINED\tREAD_PAIRS_EXAMINED\n"
                "Unknown Library\t0\t1\n",
                encoding="utf-8",
            )
            self.assertEqual(
                parity_compare.load_metrics(path),
                {"Unknown Library": ["0", "1"]},
            )

    def test_compare_metrics_detects_difference(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.txt"
            turbo = Path(tempdir) / "turbo.txt"
            picard.write_text("LIBRARY\tX\nlib\t1\n", encoding="utf-8")
            turbo.write_text("LIBRARY\tX\nlib\t2\n", encoding="utf-8")
            with self.assertRaises(SystemExit):
                parity_compare.compare_metrics(picard, turbo, "test")

    def test_compare_clean_sam_fields(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.sam"
            turbo = Path(tempdir) / "turbo.sam"
            body = "@HD\tVN:1.6\nread1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\n"
            picard.write_text(body, encoding="utf-8")
            turbo.write_text(body, encoding="utf-8")
            parity_compare.compare_clean_sam_fields(picard, turbo, "CleanSam")

    @unittest.skipUnless(shutil.which("samtools"), "samtools is required")
    def test_sam_records_omits_headers(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            sam = Path(tempdir) / "input.sam"
            sam.write_text(
                "@HD\tVN:1.6\n"
                "@SQ\tSN:chr1\tLN:1000\n"
                "read1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\n",
                encoding="utf-8",
            )
            self.assertEqual(
                parity_compare.sam_records(sam, ""),
                ["read1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF"],
            )

    def test_compare_stable_sam_lines(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.sam"
            turbo = Path(tempdir) / "turbo.sam"
            body = "@HD\tVN:1.6\nread1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\n"
            picard.write_text(body, encoding="utf-8")
            turbo.write_text(body, encoding="utf-8")
            parity_compare.compare_stable_sam_lines(picard, turbo, "test")

    def test_compare_stable_sam_lines_with_sorted_tags(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.sam"
            turbo = Path(tempdir) / "turbo.sam"
            picard.write_text(
                "@HD\tVN:1.6\nread1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tMD:Z:4\tNM:i:0\tUQ:i:0\n",
                encoding="utf-8",
            )
            turbo.write_text(
                "@HD\tVN:1.6\nread1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tUQ:i:0\tMD:Z:4\tNM:i:0\n",
                encoding="utf-8",
            )
            parity_compare.compare_stable_sam_lines_with_sorted_tags(
                picard,
                turbo,
                "SetNmMdAndUqTags",
            )

    def test_compare_validate_summary(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.txt"
            turbo = Path(tempdir) / "turbo.txt"
            picard.write_text("ERROR\tMISSING_READ_GROUP\t1\n", encoding="utf-8")
            turbo.write_text("ERROR\tMISSING_READ_GROUP\t1\n", encoding="utf-8")
            parity_compare.compare_validate_summary(picard, turbo, "ValidateSamFile")

    def test_compare_merge_multiset(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.sam"
            turbo = Path(tempdir) / "turbo.sam"
            body = (
                "@HD\tVN:1.6\tSO:coordinate\n"
                "@SQ\tSN:chr1\tLN:1000\n"
                "read-a\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tFFFF\n"
                "read-b\t0\tchr1\t20\t60\t4M\t*\t0\t0\tACGT\tFFFF\n"
            )
            picard.write_text(body, encoding="utf-8")
            turbo.write_text(body, encoding="utf-8")
            parity_compare.compare_merge_multiset(picard, turbo, "MergeSamFiles")

    def test_compare_fastq_trio(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            paths = [Path(tempdir) / name for name in ("p1", "p2", "pu", "t1", "t2", "tu")]
            for path in paths:
                path.write_text("@read\nACGT\n+\nFFFF\n", encoding="utf-8")
            parity_compare.compare_fastq_trio(
                paths[0],
                paths[1],
                paths[2],
                paths[3],
                paths[4],
                paths[5],
                "SamToFastq",
            )


if __name__ == "__main__":
    unittest.main()
