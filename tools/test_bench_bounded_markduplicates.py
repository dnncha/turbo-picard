from __future__ import annotations

import os
import shutil
import sys
import tempfile
import time
import unittest
from pathlib import Path

import pytest
from bench_bounded_markduplicates import (  # type: ignore[import-not-found]
    Case,
    canonical_header,
    canonical_record,
    cases,
    measure,
    read_histograms,
    sam_lines,
)


class BoundedMarkduplicatesEvidenceTests(unittest.TestCase):
    def test_run_dates_compare_exact_instants_without_masking_other_values(self) -> None:
        original = "@RG\tID:r\tLB:lib\tDT:2016-01-23T00:00:00-0500\n"
        pacific = original.replace("2016-01-23T00:00:00-0500", "2016-01-22T21:00:00-0800")
        utc = original.replace("2016-01-23T00:00:00-0500", "2016-01-23T05:00:00Z")
        assert canonical_header(original) == canonical_header(pacific)
        assert canonical_header(original) == canonical_header(utc)
        for changed in [
            original.replace("00:00:00", "00:00:01"),
            original.replace("LB:lib", "LB:other"),
            original.replace("-0500", "-0600"),
            original.replace("-0500", ""),
            original.replace("00:00:00", "00:00:00.000001"),
        ]:
            assert canonical_header(original) != canonical_header(changed)
        for value in [
            "2016-99-23T00:00:00-0500",
            "2016-01-23T00:00:00.0000001-0500",
            "2016-01-23T00:00:00+0060",
            "bad",
        ]:
            assert "DT:" + value in canonical_header("@RG\tID:r\tDT:" + value)
        assert "DT:2016-01-23T00:00:00-0500" in canonical_header(original.replace("@RG", "@SQ"))

    def test_canonical_record_only_ignores_program_provenance_and_tag_order(self) -> None:
        core = "read\t99\tchr1\t101\t60\t20M\t=\t201\t120\tAAAA\tIIII"
        original = core + "\tRG:Z:rg1\tRX:Z:AAAA\tDS:i:2\tDI:i:0\tDT:Z:SQ\tPG:Z:first\n"
        reordered = core + "\tDI:i:0\tRX:Z:AAAA\tRG:Z:rg1\tDT:Z:SQ\tDS:i:2\tPG:Z:second\n"
        assert canonical_record(original) == canonical_record(reordered)
        for old, new in [
            ("rg1", "rg2"),
            ("\t99\t", "\t1123\t"),
            ("DS:i:2", "DS:i:3"),
            ("DI:i:0", "DI:i:2"),
            ("DT:Z:SQ", "DT:Z:LB"),
            ("RX:Z:AAAA", "RX:Z:CCCC"),
            ("\tAAAA\tIIII", "\tCAAA\tIIII"),
            ("\tIIII", "\tHHHH"),
        ]:
            with self.subTest(field=old):
                assert canonical_record(original) != canonical_record(original.replace(old, new))

    def test_malformed_record_is_rejected(self) -> None:
        with pytest.raises(ValueError, match="malformed alignment record"):
            canonical_record("too\tfew\tfields\n")

    def test_all_cases_have_valid_coordinate_order_and_complementary_mates(self) -> None:
        for case in cases():
            small = Case(case.name, case.shape, 3, min(case.family_size, 5), case.remove, case.paired_padding)
            lines = list(sam_lines(small))
            assert lines == list(sam_lines(small))
            rows = [line.rstrip("\n").split("\t") for line in lines if not line.startswith("@")]
            previous: tuple[int, int] | None = None
            templates: dict[tuple[str, str], list[int]] = {}
            for row in rows:
                flag = int(row[1])
                coordinate = (1, 0) if row[2] == "*" else (0, int(row[3]))
                if previous is not None:
                    assert coordinate >= previous
                previous = coordinate
                assert len(row[9]) == len(row[10])
                if flag & 1:
                    group = next(tag for tag in row[11:] if tag.startswith("RG:Z:"))
                    templates.setdefault((group, row[0]), []).append(flag & 0xC0)
            for flags in templates.values():
                assert sorted(flags) == [64, 128]

    def test_paired_cases_include_full_length_unique_templates(self) -> None:
        rows = [
            line.rstrip("\n").split("\t")
            for line in sam_lines(Case("paired", "ties", 3, paired_padding=True))
            if line.startswith("pair-")
        ]
        assert len(rows) == 6
        for first, second in zip(rows[::2], rows[1::2]):
            assert first[0] == second[0]
            assert (int(first[1]), int(second[1])) == (99, 147)
            assert (first[5], second[5]) == ("150M", "150M")
            assert first[7] == second[3]
            assert second[7] == first[3]
            assert (len(first[9]), len(second[9])) == (150, 150)
        assert len({row[3] for row in rows}) == 6

    def test_histogram_comparison_preserves_columns_bins_and_counts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "metrics.txt"
            original = "## HISTOGRAM\tjava.lang.Double\nBIN\tCoverageMult\tall_sets\n1.0\t1.000000\t5\n2.0\t1.5\t2\n"
            path.write_text(original)
            expected = read_histograms(path)
            path.write_text(original.replace("1.0\t1.000000\t5", "1\t1\t5.0"))
            assert read_histograms(path) == expected
            for old, new in [("all_sets", "optical_sets"), ("\t5\n", "\t6\n"), ("2.0\t", "3.0\t")]:
                path.write_text(original.replace(old, new))
                assert read_histograms(path) != expected
            path.write_text("")
            assert read_histograms(path) != expected
            for malformed in [
                original.replace("\t5\n", "\n"),
                original.replace("1.5", "NaN"),
                original + "2.0\t1.5\t2\n",
                "## HISTOGRAM\tjava.lang.Double\n",
            ]:
                path.write_text(malformed)
                with pytest.raises(ValueError, match="histogram"):
                    read_histograms(path)

    @unittest.skipUnless(sys.platform.startswith("linux") and shutil.which("time"), "GNU time on Linux required")
    def test_timeout_kills_measured_descendants(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            marker = root / "survivor"
            descendant = "import pathlib,sys,time;time.sleep(0.5);pathlib.Path(sys.argv[1]).write_text('survived')"
            parent = "import subprocess,sys,time;subprocess.Popen([sys.executable,'-c',sys.argv[1],sys.argv[2]]);time.sleep(10)"
            result = measure(
                [sys.executable, "-c", parent, descendant, str(marker)],
                root / "run",
                os.environ.copy(),
                timeout_seconds=0.1,
            )
            assert result["exit_code"] == 124
            time.sleep(0.6)
            assert not marker.exists()

    def test_shapes_retain_the_adversarial_properties(self) -> None:
        ties = [line.split("\t") for line in sam_lines(Case("ties", "ties", 0)) if not line.startswith("@")]
        assert ties[0][3] == ties[1][3]
        assert ties[0][0] > ties[1][0]
        assert ties[-1][2] == "*"
        shared = [line.split("\t") for line in sam_lines(Case("shared", "same-library", 0)) if not line.startswith("@")]
        assert shared[0][0] == shared[1][0]
        assert shared[0][-1] != shared[1][-1]
        family = [line.split("\t") for line in sam_lines(Case("family", "family", 0, 5)) if not line.startswith("@")]
        assert [row[0] for row in family[:5]] == sorted(row[0] for row in family[:5])
        assert family[-1][2] != "*"


if __name__ == "__main__":
    unittest.main()
