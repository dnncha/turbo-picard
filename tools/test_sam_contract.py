"""Mutation tests: each protected scientific invariant must change comparison.

These deliberately corrupt independently written SAM/metrics fixtures rather
than mirroring how the native implementations calculate results.
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

if __package__:
    from . import sam_contract, compare_real_data, verify_real_data_evidence
else:
    import sam_contract
    import compare_real_data
    import verify_real_data_evidence


HEADER = "@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chr1\tLN:1000\n@RG\tID:rg\tSM:sample\tLB:library\n"
RECORD = "read\t0\tchr1\t10\t60\t4M\t*\t0\t0\tACGT\tIIII\tRG:Z:rg\tNM:i:0\tRX:Z:AAAA\tzz:Z:opaque"
METRICS = "## METRICS CLASS\tpicard.sam.DuplicationMetrics\nLIBRARY\tUNPAIRED_READS_EXAMINED\nlibrary\t1\n\n## HISTOGRAM\tjava.lang.Double\nset_size\tall_sets\n1.0\t1\n"


class ScientificContractTests(unittest.TestCase):
    def setUp(self):
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

    def compare(self):
        tool = Path(__file__).with_name("compare_markduplicates.py")
        result = subprocess.run([sys.executable, str(tool), "--picard-bam", str(self.left), "--turbo-picard-bam", str(self.right), "--picard-metrics", str(self.before), "--turbo-picard-metrics", str(self.after), "--json"], capture_output=True, text=True)
        data = json.loads(result.stdout)
        self.assertEqual(data["comparator"], "markduplicates-v2")
        self.assertEqual(data["ok"], result.returncode == 0)
        return data["ok"]

    def test_every_mandatory_sam_field_is_protected_by_both_comparators(self):
        changed = ["other", "16", "chr2", "11", "59", "2M2S", "chr1", "1", "1", "TTTT", "!!!!"]
        original_digest = compare_real_data.digest_markduplicates_semantics(self.left)
        for index, value in enumerate(changed):
            with self.subTest(field=index):
                fields = RECORD.split("\t")
                fields[index] = value
                self.right.write_text(HEADER + "\t".join(fields) + "\n")
                self.assertFalse(self.compare())
                self.assertNotEqual(original_digest, compare_real_data.digest_markduplicates_semantics(self.right))

    def test_unknown_and_typed_tags_are_scientific_data(self):
        for old, new in [("RG:Z:rg", "RG:Z:wrong"), ("NM:i:0", "NM:i:1"), ("RX:Z:AAAA", "RX:Z:TTTT"), ("zz:Z:opaque", "zz:H:ABCD")]:
            with self.subTest(tag=old):
                self.right.write_text(HEADER + RECORD.replace(old, new) + "\n")
                self.assertFalse(self.compare())
        for tag in ["DT:Z:LB", "DS:i:2", "DI:i:0", "BX:Z:A", "BY:Z:C", "OQ:Z:!!!!", "XN:i:1", "XY:B:i,1,2"]:
            with self.subTest(tag=tag):
                self.right.write_text(HEADER + RECORD + "\t" + tag + "\n")
                self.assertFalse(self.compare())

    def test_sequence_and_read_group_definitions_are_protected(self):
        for old, new in [("LN:1000", "LN:999"), ("SM:sample", "SM:other"), ("LB:library", "LB:other"), ("ID:rg", "ID:other"), ("SN:chr1", "SN:chr2")]:
            with self.subTest(change=new):
                self.right.write_text(HEADER.replace(old, new) + RECORD + "\n")
                self.assertFalse(self.compare())
        self.right.write_text(HEADER.replace("LN:1000", "LN:1000\tM5:0" ) + RECORD + "\n")
        self.assertFalse(self.compare())
        for field in ["AS:GRCh38", "SP:Homo sapiens", "AN:1", "AH:chr2:1-1000", "TP:circular", "zz:extension"]:
            with self.subTest(sequence_field=field):
                self.right.write_text(HEADER.replace("LN:1000", "LN:1000\t" + field) + RECORD + "\n")
                self.assertFalse(self.compare())

    def test_reference_uri_and_synthesized_null_assembly_are_incidental(self):
        self.left.write_text(HEADER.replace("LN:1000", "LN:1000\tUR:file:/reference.fa") + RECORD + "\n")
        self.right.write_text(HEADER.replace("LN:1000", "LN:1000\tUR:file:/other.fa\tAS:null") + RECORD + "\n")
        self.assertTrue(self.compare())

    def test_histogram_deletion_corruption_and_schema_changes_fail(self):
        for changed in [METRICS.split("## HISTOGRAM")[0], METRICS.replace("1.0\t1", "1.0\t9"), METRICS.replace("all_sets", "optical_sets"), METRICS.replace("library\t1", "library\t2")]:
            with self.subTest(metrics=changed):
                self.after.write_text(changed)
                self.assertFalse(self.compare())

    def test_incidental_pg_tag_tag_order_and_numeric_float_spelling_are_normalized(self):
        self.left.write_text(HEADER + "@PG\tID:picard\n" + RECORD + "\tPG:Z:picard\tZF:f:1.0\n")
        fields = RECORD.split("\t")
        self.right.write_text(HEADER + "@PG\tID:native\n" + "\t".join(fields[:11] + list(reversed(fields[11:]))) + "\tZF:f:1\tPG:Z:native\n")
        self.assertTrue(self.compare())
        self.assertEqual(compare_real_data.digest_markduplicates_semantics(self.left), compare_real_data.digest_markduplicates_semantics(self.right))

    def test_reference_equals_alias_and_explicit_mate_name_are_equivalent(self):
        self.left.write_text(HEADER + RECORD.replace("\t*\t0\t0\t", "\t=\t30\t24\t") + "\n")
        self.right.write_text(self.left.read_text().replace("\t=\t", "\tchr1\t"))
        self.assertTrue(self.compare())

    def test_multiplicity_is_preserved(self):
        self.right.write_text(self.left.read_text() + RECORD + "\n")
        self.assertFalse(self.compare())

    def test_zoned_run_dates_compare_as_exact_instants(self):
        self.left.write_text(HEADER.replace("LB:library", "LB:library\tDT:2016-01-23T00:00:00.125-0500") + RECORD + "\n")
        self.right.write_text(HEADER.replace("LB:library", "LB:library\tDT:2016-01-23T05:00:00.125+0000") + RECORD + "\n")
        self.assertTrue(self.compare())
        for changed in ["2016-01-23T05:00:00.126+0000", "2016-01-23T05:00:00.125", "2016-01-23T05:00:00.125000001+0000", "2016-01-23T05:00:00,125000001+0000", "invalid-date"]:
            with self.subTest(date=changed):
                self.right.write_text(HEADER.replace("LB:library", f"LB:library\tDT:{changed}") + RECORD + "\n")
                self.assertFalse(self.compare())

    def test_real_data_verifier_recomputes_v2_without_relabeling_historical_evidence(self):
        artifact = self.root / "marked.bam"
        view, metrics = verify_real_data_evidence.markduplicates_sidecars(artifact)
        view.write_text(HEADER + RECORD + "\n")
        metrics.write_text(METRICS)
        old_contract = "duplicate-marking semantic digest plus stable metrics digest"
        legacy = verify_real_data_evidence.recomputable_artifact_digest(artifact, old_contract)
        current = verify_real_data_evidence.recomputable_artifact_digest(artifact, sam_contract.COMPARISON)
        self.assertNotEqual(legacy, current)
        self.assertIn(";headers=", current)
        for changed in [
            (HEADER + RECORD.replace("ACGT", "TTTT") + "\n", METRICS),
            (HEADER.replace("LB:library", "LB:other") + RECORD + "\n", METRICS),
            (HEADER + RECORD + "\n", METRICS.replace("1.0\t1", "1.0\t9")),
            (HEADER + RECORD + "\n", METRICS.replace("java.lang.Double", "java.lang.Integer")),
        ]:
            with self.subTest(changed=changed):
                view.write_text(changed[0]); metrics.write_text(changed[1])
                self.assertNotEqual(current, verify_real_data_evidence.recomputable_artifact_digest(artifact, sam_contract.COMPARISON))
        # Historical v1 keeps its original projection and input hashes.
        view.write_text(HEADER + RECORD.replace("ACGT", "TTTT") + "\n")
        metrics.write_text(METRICS)
        self.assertEqual(legacy, verify_real_data_evidence.recomputable_artifact_digest(artifact, old_contract))


if __name__ == "__main__":
    unittest.main()
