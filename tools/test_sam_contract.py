from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING or __package__:
    from . import compare_real_data, sam_contract, verify_real_data_evidence
else:
    import compare_real_data
    import sam_contract
    import verify_real_data_evidence


HEADER = "@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chr1\tLN:1000\n@RG\tID:rg\tSM:sample\tLB:library\n"
RECORD = "read\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII\tRG:Z:rg\tNM:i:0\tRX:Z:AAAA\tzz:Z:opaque"
METRICS = "## METRICS CLASS\tpicard.sam.DuplicationMetrics\nLIBRARY\tUNPAIRED_READS_EXAMINED\nlibrary\t1\n\n## HISTOGRAM\tjava.lang.Double\nset_size\tall_sets\n1.0\t1\n"


class ScientificContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.left = self.root / "left.sam"
        self.right = self.root / "right.sam"
        self.left.write_text(HEADER + RECORD + "\n")
        self.right.write_text(self.left.read_text())
        self.before = self.root / "before.metrics"
        self.after = self.root / "after.metrics"
        self.before.write_text(METRICS)
        self.after.write_text(METRICS)

    def compare(self) -> bool:
        tool = Path(__file__).with_name("compare_markduplicates.py")
        result = subprocess.run(
            [
                sys.executable,
                str(tool),
                "--picard-bam",
                str(self.left),
                "--turbo-picard-bam",
                str(self.right),
                "--picard-metrics",
                str(self.before),
                "--turbo-picard-metrics",
                str(self.after),
                "--json",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        data = json.loads(result.stdout)
        assert data["comparator"] == "markduplicates-v2"
        assert data["ok"] == (result.returncode == 0)
        return bool(data["ok"])

    def test_every_mandatory_sam_field_is_protected_by_both_comparators(self) -> None:
        changed = ["other", "16", "chr2", "11", "59", "2M2S", "chr1", "1", "1", "TTTT", "!!!!"]
        original_digest = compare_real_data.digest_markduplicates_semantics(self.left)
        for index, value in enumerate(changed):
            with self.subTest(field=index):
                fields = RECORD.split("\t")
                fields[index] = value
                self.right.write_text(HEADER + "\t".join(fields) + "\n")
                assert not self.compare()
                assert original_digest != compare_real_data.digest_markduplicates_semantics(self.right)

    def test_unknown_and_typed_tags_are_scientific_data(self) -> None:
        for old, new in [
            ("RG:Z:rg", "RG:Z:wrong"),
            ("NM:i:0", "NM:i:1"),
            ("RX:Z:AAAA", "RX:Z:TTTT"),
            ("zz:Z:opaque", "zz:H:ABCD"),
        ]:
            with self.subTest(tag=old):
                self.right.write_text(HEADER + RECORD.replace(old, new) + "\n")
                assert not self.compare()
        for tag in ["DT:Z:LB", "DS:i:2", "DI:i:0", "BX:Z:A", "BY:Z:C", "OQ:Z:!!!!", "XN:i:1", "XY:B:i,1,2"]:
            with self.subTest(tag=tag):
                self.right.write_text(HEADER + RECORD + "\t" + tag + "\n")
                assert not self.compare()

    def test_sequence_and_read_group_definitions_are_protected(self) -> None:
        for old, new in [
            ("LN:1000", "LN:999"),
            ("SM:sample", "SM:other"),
            ("LB:library", "LB:other"),
            ("ID:rg", "ID:other"),
            ("SN:chr1", "SN:chr2"),
        ]:
            with self.subTest(change=new):
                self.right.write_text(HEADER.replace(old, new) + RECORD + "\n")
                assert not self.compare()
        self.right.write_text(HEADER.replace("LN:1000", "LN:1000\tM5:0") + RECORD + "\n")
        assert not self.compare()
        for field in ["AS:GRCh38", "SP:Homo sapiens", "AN:1", "AH:chr2:1-1000", "TP:circular", "zz:extension"]:
            with self.subTest(sequence_field=field):
                self.right.write_text(HEADER.replace("LN:1000", "LN:1000\t" + field) + RECORD + "\n")
                assert not self.compare()

    def test_reference_uri_and_synthesized_null_assembly_are_incidental(self) -> None:
        self.left.write_text(HEADER.replace("LN:1000", "LN:1000\tUR:file:/reference.fa") + RECORD + "\n")
        self.right.write_text(HEADER.replace("LN:1000", "LN:1000\tUR:file:/other.fa\tAS:null") + RECORD + "\n")
        assert self.compare()

    def test_histogram_deletion_corruption_and_schema_changes_fail(self) -> None:
        for changed in [
            METRICS.split("## HISTOGRAM", maxsplit=1)[0],
            METRICS.replace("1.0\t1", "1.0\t9"),
            METRICS.replace("all_sets", "optical_sets"),
            METRICS.replace("library\t1", "library\t2"),
        ]:
            with self.subTest(metrics=changed):
                self.after.write_text(changed)
                assert not self.compare()

    def test_incidental_pg_tag_tag_order_and_numeric_float_spelling_are_normalized(self) -> None:
        self.left.write_text(HEADER + "@PG\tID:picard\n" + RECORD + "\tPG:Z:picard\tZF:f:1.0\n")
        fields = RECORD.split("\t")
        self.right.write_text(
            HEADER
            + "@PG\tID:native\n"
            + "\t".join(fields[:11] + list(reversed(fields[11:])))
            + "\tZF:f:1\tPG:Z:native\n"
        )
        assert self.compare()
        assert compare_real_data.digest_markduplicates_semantics(
            self.left
        ) == compare_real_data.digest_markduplicates_semantics(self.right)

    def test_reference_equals_alias_and_explicit_mate_name_are_equivalent(self) -> None:
        self.left.write_text(HEADER + RECORD.replace("\t*\t0\t0\t", "\t=\t30\t24\t") + "\n")
        self.right.write_text(self.left.read_text().replace("\t=\t", "\tchr1\t"))
        assert self.compare()

    def test_multiplicity_is_preserved(self) -> None:
        self.right.write_text(self.left.read_text() + RECORD + "\n")
        assert not self.compare()

    def test_zoned_run_dates_compare_as_exact_instants(self) -> None:
        self.left.write_text(
            HEADER.replace("LB:library", "LB:library\tDT:2016-01-23T00:00:00.125-0500") + RECORD + "\n"
        )
        self.right.write_text(
            HEADER.replace("LB:library", "LB:library\tDT:2016-01-23T05:00:00.125+0000") + RECORD + "\n"
        )
        assert self.compare()
        for changed in [
            "2016-01-23T05:00:00.126+0000",
            "2016-01-23T05:00:00.125",
            "2016-01-23T05:00:00.125000001+0000",
            "2016-01-23T05:00:00,125000001+0000",
            "invalid-date",
        ]:
            with self.subTest(date=changed):
                self.right.write_text(HEADER.replace("LB:library", f"LB:library\tDT:{changed}") + RECORD + "\n")
                assert not self.compare()

    def test_real_data_verifier_recomputes_v2_without_relabeling_historical_evidence(self) -> None:
        artifact = self.root / "marked.bam"
        view, metrics = verify_real_data_evidence.markduplicates_sidecars(artifact)
        view.write_text(HEADER + RECORD + "\n")
        metrics.write_text(METRICS)
        old_contract = "duplicate-marking semantic digest plus stable metrics digest"
        legacy = verify_real_data_evidence.recomputable_artifact_digest(artifact, old_contract)
        current = verify_real_data_evidence.recomputable_artifact_digest(artifact, sam_contract.COMPARISON)
        assert legacy != current
        assert current is not None
        assert ";headers=" in current
        for changed in [
            (HEADER + RECORD.replace("ACGT", "TTTT") + "\n", METRICS),
            (HEADER.replace("LB:library", "LB:other") + RECORD + "\n", METRICS),
            (HEADER + RECORD + "\n", METRICS.replace("1.0\t1", "1.0\t9")),
            (HEADER + RECORD + "\n", METRICS.replace("java.lang.Double", "java.lang.Integer")),
        ]:
            with self.subTest(changed=changed):
                view.write_text(changed[0])
                metrics.write_text(changed[1])
                assert current != verify_real_data_evidence.recomputable_artifact_digest(
                    artifact, sam_contract.COMPARISON
                )
        view.write_text(HEADER + RECORD.replace("ACGT", "TTTT") + "\n")
        metrics.write_text(METRICS)
        assert legacy == verify_real_data_evidence.recomputable_artifact_digest(artifact, old_contract)


if __name__ == "__main__":
    unittest.main()
