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

    def test_compare_wgs_metrics_allows_documented_real_data_tolerance(self) -> None:
        header = (
            "GENOME_TERRITORY\tMEAN_COVERAGE\tSD_COVERAGE\tMEDIAN_COVERAGE\tMAD_COVERAGE\t"
            "PCT_EXC_ADAPTER\tPCT_EXC_MAPQ\tPCT_EXC_DUPE\tPCT_EXC_UNPAIRED\tPCT_EXC_BASEQ\t"
            "PCT_EXC_OVERLAP\tPCT_EXC_CAPPED\tPCT_EXC_TOTAL\tPCT_1X\tPCT_5X\tPCT_10X\t"
            "PCT_15X\tPCT_20X\tPCT_25X\tPCT_30X\tPCT_40X\tPCT_50X\tPCT_60X\tPCT_70X\t"
            "PCT_80X\tPCT_90X\tPCT_100X\tFOLD_80_BASE_PENALTY\tFOLD_90_BASE_PENALTY\t"
            "FOLD_95_BASE_PENALTY\tHET_SNP_SENSITIVITY\tHET_SNP_Q\n"
        )
        picard_row = (
            "16568\t17.153911\t63.075521\t0\t0\t0\t0.000041\t0.111158\t0\t0.072486\t"
            "0.057433\t0.618996\t0.860114\t0.069411\t0.069411\t0.069351\t0.069351\t"
            "0.06929\t0.06923\t0.06923\t0.069169\t0.069109\t0.069049\t0.068988\t"
            "0.068868\t0.068807\t0.068747\t?\t?\t?\t0.069409\t0\n"
        )
        turbo_row = (
            "16569\t17.152876\t63.073758\t0\t0\t0\t0.000042\t0.111264\t0\t0.07263\t"
            "0.019768\t0.654216\t0.85792\t0.069407\t0.069407\t0.069346\t0.069346\t"
            "0.069286\t0.069226\t0.069226\t0.069165\t0.069105\t0.069045\t0.068984\t"
            "0.068864\t0.068803\t0.068743\t?\t?\t?\t0.069406\t0\n"
        )
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.txt"
            turbo = Path(tempdir) / "turbo.txt"
            picard.write_text(header + picard_row, encoding="utf-8")
            turbo.write_text(header + turbo_row, encoding="utf-8")
            parity_compare.compare_wgs_metrics(picard, turbo, "CollectWgsMetrics")

    def test_compare_wgs_metrics_rejects_large_drift(self) -> None:
        header = (
            "GENOME_TERRITORY\tMEAN_COVERAGE\tSD_COVERAGE\tMEDIAN_COVERAGE\tMAD_COVERAGE\t"
            "PCT_EXC_ADAPTER\tPCT_EXC_MAPQ\tPCT_EXC_DUPE\tPCT_EXC_UNPAIRED\tPCT_EXC_BASEQ\t"
            "PCT_EXC_OVERLAP\tPCT_EXC_CAPPED\tPCT_EXC_TOTAL\tPCT_1X\tPCT_5X\tPCT_10X\t"
            "PCT_15X\tPCT_20X\tPCT_25X\tPCT_30X\tPCT_40X\tPCT_50X\tPCT_60X\tPCT_70X\t"
            "PCT_80X\tPCT_90X\tPCT_100X\tFOLD_80_BASE_PENALTY\tFOLD_90_BASE_PENALTY\t"
            "FOLD_95_BASE_PENALTY\tHET_SNP_SENSITIVITY\tHET_SNP_Q\n"
        )
        row = (
            "100\t10\t1\t0\t0\t0\t0\t0\t0\t0\t0.01\t0.2\t0.21\t1\t1\t1\t1\t1\t1\t1\t"
            "1\t1\t1\t1\t1\t1\t1\t?\t?\t?\t0.1\t0\n"
        )
        drifted = row.replace("0.01\t0.2", "0.20\t0.2")
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.txt"
            turbo = Path(tempdir) / "turbo.txt"
            picard.write_text(header + row, encoding="utf-8")
            turbo.write_text(header + drifted, encoding="utf-8")
            with self.assertRaisesRegex(SystemExit, "PCT_EXC_OVERLAP"):
                parity_compare.compare_wgs_metrics(picard, turbo, "CollectWgsMetrics")

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
            picard.write_text(
                "@HD\tVN:1.6\tSO:coordinate\n"
                "@PG\tID:picard\n"
                "read1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tPG:Z:picard\n",
                encoding="utf-8",
            )
            turbo.write_text(
                "@HD\tVN:1.5\tSO:coordinate\n"
                "@PG\tID:turbo\n"
                "read1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tPG:Z:turbo\n",
                encoding="utf-8",
            )
            parity_compare.compare_stable_sam_lines(picard, turbo, "test")

    def test_compare_stable_sam_lines_ignoring_md_nm(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.sam"
            turbo = Path(tempdir) / "turbo.sam"
            picard.write_text(
                "@HD\tVN:1.6\n"
                "read1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tAS:i:4\tOQ:Z:FFFF\n",
                encoding="utf-8",
            )
            turbo.write_text(
                "@HD\tVN:1.6\n"
                "read1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tNM:i:0\tAS:i:4\tMD:Z:4\tOQ:Z:FFFF\n",
                encoding="utf-8",
            )
            parity_compare.compare_stable_sam_lines_ignoring_md_nm(
                picard,
                turbo,
                "CRAM ViewSam",
            )

    def test_compare_stable_sam_lines_ignoring_md_nm_rejects_other_tag_drift(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.sam"
            turbo = Path(tempdir) / "turbo.sam"
            picard.write_text(
                "@HD\tVN:1.6\n"
                "read1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tAS:i:4\n",
                encoding="utf-8",
            )
            turbo.write_text(
                "@HD\tVN:1.6\n"
                "read1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tAS:i:5\tNM:i:0\tMD:Z:4\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(SystemExit, "stable SAM output differs"):
                parity_compare.compare_stable_sam_lines_ignoring_md_nm(
                    picard,
                    turbo,
                    "CRAM ViewSam",
                )

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

    def test_compare_merge_multiset_reports_record_difference(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.sam"
            turbo = Path(tempdir) / "turbo.sam"
            picard.write_text(
                "@HD\tVN:1.6\tSO:coordinate\n"
                "@SQ\tSN:chr1\tLN:1000\n"
                "read1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tNM:i:0\n",
                encoding="utf-8",
            )
            turbo.write_text(
                "@HD\tVN:1.6\tSO:coordinate\n"
                "@SQ\tSN:chr1\tLN:1000\n"
                "read1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tNM:i:1\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(SystemExit, "coordinate-sorted SAM multiset differs"):
                parity_compare.compare_merge_multiset(picard, turbo, "test")

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
