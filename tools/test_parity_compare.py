#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import pytest

MODULE_PATH = Path(__file__).with_name("parity_compare.py")
SPEC = importlib.util.spec_from_file_location("parity_compare", MODULE_PATH)
assert SPEC is not None
parity_compare = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["parity_compare"] = parity_compare
SPEC.loader.exec_module(parity_compare)


class ParityCompareTests(unittest.TestCase):
    def test_validation_exit_contract_protects_every_picard_category(self) -> None:
        helper = Path(__file__).with_name("cram_compat.sh")
        for reference in range(4):
            for native in range(4):
                with self.subTest(reference=reference, native=native):
                    result = subprocess.run(
                        [
                            "bash",
                            "-c",
                            'source "$1"; parity_validate_samfile_exit_matches "$2" "$3"',
                            "bash",
                            str(helper),
                            str(reference),
                            str(native),
                        ],
                        check=False,
                    )
                    assert (result.returncode == 0) == (reference == native)

    def test_binary_comparison_streams_equal_files_and_checks_both_eofs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            left, right = Path(directory) / "left", Path(directory) / "right"
            block = b"ACGT" * (1024 * 256)
            for size in (0, 1, len(block), len(block) + 1, 3 * len(block) + 17):
                data = (block * 4)[:size]
                left.write_bytes(data)
                right.write_bytes(data)
                with patch.object(Path, "read_bytes", side_effect=AssertionError("whole-file read")):
                    parity_compare.compare_binary_files(left, right, "fixture")
                for extra in (b"x", b"\x00"):
                    right.write_bytes(data + extra)
                    for first, second in ((left, right), (right, left)):
                        with pytest.raises(SystemExit, match="fixture binary output differs from Picard"):
                            parity_compare.compare_binary_files(first, second, "fixture")

    def test_binary_comparison_detects_changes_across_chunk_boundaries(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            left, right = Path(directory) / "left", Path(directory) / "right"
            data = bytearray(b"A" * (2 * 1024 * 1024 + 7))
            left.write_bytes(data)
            for offset in (0, 1024 * 1024 - 1, 1024 * 1024, len(data) - 1):
                changed = data.copy()
                changed[offset] = ord("T")
                right.write_bytes(changed)
                with pytest.raises(SystemExit, match="binary output differs"):
                    parity_compare.compare_binary_files(left, right, "fixture")

    def test_nonfinite_metrics_do_not_bypass_tolerances(self) -> None:
        for first, second in (("NaN", "7"), ("7", "NaN"), ("NaN", "Infinity"), ("Infinity", "-Infinity")):
            with self.subTest(first=first, second=second), tempfile.TemporaryDirectory() as tmp:
                left, right = Path(tmp) / "a", Path(tmp) / "b"
                left.write_text(f"FIELD\n{first}\n")
                right.write_text(f"FIELD\n{second}\n")
                with pytest.raises(SystemExit):
                    parity_compare.compare_metrics(left, right, "nonfinite")
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "same"
            path.write_text("FIELD\nNaN\n")
            parity_compare.compare_metrics(path, path, "same empty metric")

    def test_load_metrics_ignores_comment_header(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            path = Path(tempdir) / "metrics.txt"
            path.write_text(
                "# metrics\nLIBRARY\tUNPAIRED_READS_EXAMINED\tREAD_PAIRS_EXAMINED\nUnknown Library\t0\t1\n",
                encoding="utf-8",
            )
            assert parity_compare.load_metrics(path) == [
                ["LIBRARY", "UNPAIRED_READS_EXAMINED", "READ_PAIRS_EXAMINED"],
                ["Unknown Library", "0", "1"],
            ]

    def test_compare_metrics_detects_difference(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.txt"
            turbo = Path(tempdir) / "turbo.txt"
            picard.write_text("LIBRARY\tX\nlib\t1\n", encoding="utf-8")
            turbo.write_text("LIBRARY\tX\nlib\t2\n", encoding="utf-8")
            with pytest.raises(SystemExit):
                parity_compare.compare_metrics(picard, turbo, "test")

    def test_compare_metrics_detects_repeated_first_column_difference(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.txt"
            turbo = Path(tempdir) / "turbo.txt"
            picard.write_text(
                "ACCUMULATION_LEVEL\tREADS_USED\tGC\tREAD_STARTS\nAll Reads\tALL\t28\t44\nAll Reads\tALL\t29\t92\n",
                encoding="utf-8",
            )
            turbo.write_text(
                "ACCUMULATION_LEVEL\tREADS_USED\tGC\tREAD_STARTS\nAll Reads\tALL\t28\t40\nAll Reads\tALL\t29\t92\n",
                encoding="utf-8",
            )
            with pytest.raises(SystemExit):
                parity_compare.compare_metrics(picard, turbo, "CollectGcBiasMetrics detail")

    def test_compare_metrics_allows_last_decimal_float_drift(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.txt"
            turbo = Path(tempdir) / "turbo.txt"
            picard.write_text("METRIC\tVALUE\nHET_SNP_SENSITIVITY\t0.069409\n", encoding="utf-8")
            turbo.write_text("METRIC\tVALUE\nHET_SNP_SENSITIVITY\t0.06941\n", encoding="utf-8")
            parity_compare.compare_metrics(picard, turbo, "CollectWgsMetrics")

    def test_compare_metrics_rejects_real_drift_and_names_the_field(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.txt"
            turbo = Path(tempdir) / "turbo.txt"
            picard.write_text("LIBRARY\tPERCENT_DUPLICATION\nlib\t0.069409\n", encoding="utf-8")
            turbo.write_text("LIBRARY\tPERCENT_DUPLICATION\nlib\t0.069419\n", encoding="utf-8")
            with pytest.raises(SystemExit, match=r"row 2 PERCENT_DUPLICATION: Picard='0\.069409' turbo='0\.069419'"):
                parity_compare.compare_metrics(picard, turbo, "MarkDuplicates")

    def test_compare_metrics_rejects_drift_hidden_by_shorter_print(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.txt"
            turbo = Path(tempdir) / "turbo.txt"
            picard.write_text("METRIC\tVALUE\nX\t0.069409\n", encoding="utf-8")
            turbo.write_text("METRIC\tVALUE\nX\t0.06942\n", encoding="utf-8")
            with pytest.raises(SystemExit):
                parity_compare.compare_metrics(picard, turbo, "drift")

    def test_compare_wgs_metrics_requires_exact_metrics_and_histogram(self) -> None:
        header = "GENOME_TERRITORY\tMEAN_COVERAGE\tPCT_EXC_OVERLAP\n"
        histogram = "coverage\thigh_quality_coverage_count\n0\t16000\n1\t568\n"
        picard_text = (
            "## METRICS CLASS\tpicard.analysis.WgsMetrics\n" + header + "16568\t17.153911\t0.057433\n\n" + histogram
        )
        cases = {
            "GENOME_TERRITORY": picard_text.replace("16568\t17", "16569\t17"),
            "PCT_EXC_OVERLAP": picard_text.replace("0.057433", "0.057434"),
            "histogram differs from Picard at row 2": picard_text.replace("1\t568", "1\t567"),
            "histogram differs from Picard at row 3": picard_text + "2\t0\n",
        }
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.txt"
            turbo = Path(tempdir) / "turbo.txt"
            picard.write_text(picard_text, encoding="utf-8")
            turbo.write_text("# turbo-picard note\n" + picard_text, encoding="utf-8")
            parity_compare.compare_wgs_metrics(picard, turbo, "CollectWgsMetrics")
            for expected_message, turbo_text in cases.items():
                turbo.write_text(turbo_text, encoding="utf-8")
                with pytest.raises(SystemExit, match=expected_message):
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
                "@HD\tVN:1.6\n@SQ\tSN:chr1\tLN:1000\nread1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\n",
                encoding="utf-8",
            )
            assert parity_compare.sam_records(sam, "") == ["read1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF"]

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
                "@HD\tVN:1.6\nread1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tAS:i:4\tOQ:Z:FFFF\n",
                encoding="utf-8",
            )
            turbo.write_text(
                "@HD\tVN:1.6\nread1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tNM:i:0\tAS:i:4\tMD:Z:4\tOQ:Z:FFFF\n",
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
                "@HD\tVN:1.6\nread1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tAS:i:4\n",
                encoding="utf-8",
            )
            turbo.write_text(
                "@HD\tVN:1.6\nread1\t0\tchr1\t1\t0\t4M\t*\t0\t0\tACGT\tFFFF\tAS:i:5\tNM:i:0\tMD:Z:4\n",
                encoding="utf-8",
            )
            with pytest.raises(SystemExit, match="stable SAM output differs"):
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

    def test_compare_validate_summary_ignores_named_type(self) -> None:
        with tempfile.TemporaryDirectory() as tempdir:
            picard = Path(tempdir) / "picard.txt"
            turbo = Path(tempdir) / "turbo.txt"
            picard.write_text("ERROR:MATE_NOT_FOUND\t2\n", encoding="utf-8")
            turbo.write_text("ERROR:MATE_NOT_FOUND\t2\nWARNING:MISSING_TAG_NM\t5\n", encoding="utf-8")
            with pytest.raises(SystemExit):
                parity_compare.compare_validate_summary(picard, turbo, "ValidateSamFile")
            parity_compare.compare_validate_summary(picard, turbo, "ValidateSamFile", ("WARNING:MISSING_TAG_NM",))

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
            with pytest.raises(SystemExit, match="coordinate-sorted SAM multiset differs"):
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
                turbo_r1=paths[3],
                turbo_r2=paths[4],
                turbo_unpaired=paths[5],
                label="SamToFastq",
            )


if __name__ == "__main__":
    unittest.main()
