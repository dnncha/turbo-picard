import importlib.util
import json
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest.mock import patch


MODULE_PATH = Path(__file__).with_name("bench_markduplicates_competitors.py")
SPEC = importlib.util.spec_from_file_location("bench_markduplicates_competitors", MODULE_PATH)
module = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = module
SPEC.loader.exec_module(module)


class CompetitorBenchmarkTests(unittest.TestCase):
    def test_presets_cover_coordinate_sorted_markdup_tools(self):
        self.assertEqual(
            set(module.preset_tools()),
            {"turbo-picard", "picard", "samtools", "sambamba", "fastdup"},
        )

    def test_presets_can_use_tool_default_read_name_regex(self):
        with patch.object(module, "executable_path", return_value="/bin/true"):
            presets = module.preset_tools(None)
        self.assertNotIn("READ_NAME_REGEX=", presets["picard"].command)

    def test_presets_add_reference_arguments_for_cram(self):
        with patch.object(module, "executable_path", return_value="/bin/true"):
            presets = module.preset_tools("null", Path("reference.fa"))
        self.assertIn("REFERENCE_SEQUENCE={reference}", presets["picard"].command)
        self.assertIn("--reference", presets["samtools"].command)

    def test_presets_can_request_duplicate_set_member_tags(self):
        with patch.object(module, "executable_path", return_value="/bin/true"):
            presets = module.preset_tools("null", None, True)
        self.assertIn("TAG_DUPLICATE_SET_MEMBERS=true", presets["picard"].command)
        self.assertIn("TAG_DUPLICATE_SET_MEMBERS=true", presets["turbo-picard"].command)

    def test_presets_can_request_primary_and_mate_specific_barcode_tags(self):
        with patch.object(module, "executable_path", return_value="/bin/true"):
            presets = module.preset_tools("null", None, False, "RX", "BX", "BY")
        for name in ("picard", "turbo-picard"):
            self.assertIn("BARCODE_TAG=RX", presets[name].command)
            self.assertIn("READ_ONE_BARCODE_TAG=BX", presets[name].command)
            self.assertIn("READ_TWO_BARCODE_TAG=BY", presets[name].command)

    def test_profile_contract_requires_umi_barcode_tags(self):
        with self.assertRaises(SystemExit):
            module.validate_profile("umi_panel", "BAM", None, (None, None, None))
        module.validate_profile("umi_panel", "BAM", None, ("RX", None, None))

    def test_profile_contract_requires_cram_for_cram_profile(self):
        with self.assertRaises(SystemExit):
            module.validate_profile("cram_reference", "BAM", None, (None, None, None))

    def test_command_expansion_preserves_regex_quantifier_braces(self):
        spec = module.ToolSpec(
            "picard",
            (
                "picard",
                "READ_NAME_REGEX=(?:[A-Z]+:){4}([0-9]+)",
                "I={input}",
                "R={reference}",
            ),
            ("picard", "--version"),
            "picard",
        )
        expanded = module.expand_command(
            spec,
            input_path=Path("input.bam"),
            output=Path("out.bam"),
            metrics=Path("metrics.txt"),
            tmp=Path("tmp"),
            threads=1,
            reference_fasta=Path("reference.fa"),
        )
        self.assertIn("(?:[A-Z]+:){4}([0-9]+)", expanded[1])
        self.assertTrue(expanded[-1].endswith("reference.fa"))

    def test_picard_version_probe_invokes_markduplicates(self):
        spec = module.ToolSpec(
            "picard",
            ("picard", "MarkDuplicates", "I={input}", "O={output}"),
            ("picard", "MarkDuplicates", "--version"),
            "picard",
        )
        self.assertEqual(spec.version_command[-2:], ("MarkDuplicates", "--version"))

    def test_custom_template_requires_input_and_output(self):
        with self.assertRaises(Exception):
            module.parse_custom_tool("broken=tool --input {input}")

    def test_expand_command_does_not_invoke_a_shell(self):
        spec = module.parse_custom_tool(
            "candidate=python3 runner.py --input {input} --output {output} "
            "--metrics {metrics} --tmp {tmp} --threads {threads}"
        )
        expanded = module.expand_command(
            spec,
            input_path=Path("input with spaces.bam"),
            output=Path("out.bam"),
            metrics=Path("metrics.txt"),
            tmp=Path("tmp"),
            threads=7,
        )
        self.assertIn(str(Path("input with spaces.bam").resolve()), expanded)
        self.assertEqual(expanded[-1], "7")

    def test_parse_gnu_time_record(self):
        with tempfile.TemporaryDirectory() as tmp:
            record = Path(tmp) / "time.tsv"
            record.write_text("TPBENCH\t1.25\t2.5\t0.25\t1024\t0\n", encoding="utf-8")
            parsed = module.parse_time_file(record)
        self.assertEqual(parsed["wall_seconds"], 1.25)
        self.assertEqual(parsed["peak_rss_bytes"], 1024 * 1024)
        self.assertEqual(parsed["exit_code"], 0)

    def test_non_gnu_time_is_not_used_as_gnu_time(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake_time = Path(tmp) / "time"
            fake_time.write_text("#!/bin/sh\nprintf 'BSD time\\n'\n", encoding="utf-8")
            fake_time.chmod(fake_time.stat().st_mode | 0o111)
            original = module.GNU_TIME
            module.GNU_TIME = fake_time
            try:
                self.assertFalse(module.gnu_time_available())
            finally:
                module.GNU_TIME = original

    def test_streaming_sam_comparison_detects_winner_change(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            reference = root / "reference.sam"
            candidate = root / "candidate.sam"
            metrics = root / "metrics.txt"
            reference.write_text(
                "@HD\tVN:1.6\nread1\t0\tchr1\t1\t60\t1M\t*\t0\t0\tA\tF\n",
                encoding="utf-8",
            )
            candidate.write_text(
                "@HD\tVN:1.6\nread1\t1024\tchr1\t1\t60\t1M\t*\t0\t0\tA\tF\n",
                encoding="utf-8",
            )
            metrics.write_text("", encoding="utf-8")
            comparison = module.compare_outputs(reference, candidate, metrics, metrics)
        self.assertEqual(comparison["status"], "FAIL")
        self.assertEqual(comparison["alignment_mismatch"]["record_index"], 0)

    def test_bam_parity_falls_back_to_samtools_without_pysam(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            bam = root / "output.bam"
            bam.write_bytes(b"not-a-real-bam")
            fake_samtools = root / "samtools"
            fake_samtools.write_text(
                textwrap.dedent(
                    """
                    #!/usr/bin/env python3
                    print("@HD\\tVN:1.6")
                    print("read1\\t1024\\tchr1\\t1\\t60\\t1M\\t*\\t0\\t0\\tA\\tF\\tDT:Z:LB")
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            fake_samtools.chmod(fake_samtools.stat().st_mode | 0o111)
            with patch.dict(sys.modules, {"pysam": None}):
                with patch.object(module.shutil, "which", return_value=str(fake_samtools)):
                    records = list(module.sam_fields(bam))
        self.assertEqual(records[0][0], "read1")
        self.assertTrue(records[0][1])
        self.assertEqual(records[0][8], "LB")

    def test_end_to_end_bundle_records_resources_and_provenance(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.bam"
            input_path.write_bytes(b"not-a-real-bam")
            fake = root / "fake_tool.py"
            fake.write_text(
                textwrap.dedent(
                    """
                    import pathlib, shutil, sys, time
                    if '--version' in sys.argv:
                        print('fake-markdup 1.2.3')
                        raise SystemExit(0)
                    source, output, metrics, temporary = map(pathlib.Path, sys.argv[1:5])
                    temporary.mkdir(parents=True, exist_ok=True)
                    scratch = temporary / 'spill.bin'
                    scratch.write_bytes(b'x' * 16384)
                    time.sleep(0.03)
                    shutil.copyfile(source, output)
                    metrics.write_text('fake metrics\\n')
                    """
                ),
                encoding="utf-8",
            )
            output_dir = root / "evidence"
            result = module.main(
                [
                    "--input", str(input_path),
                    "--output-dir", str(output_dir),
                    "--tools", "",
                    "--tool", f"fake={sys.executable} {fake} {{input}} {{output}} {{metrics}} {{tmp}}",
                    "--reference-tool", "fake",
                    "--profile", "umi_panel",
                    "--barcode-tag", "RX",
                    "--repeats", "1",
                    "--warmups", "0",
                    "--disk-sample-ms", "10",
                ]
            )
            report = json.loads((output_dir / "report.json").read_text(encoding="utf-8"))
        self.assertEqual(result, 0)
        self.assertEqual(report["claim_status"], "evidence_only")
        self.assertEqual(report["protocol"]["profile"], "umi_panel")
        self.assertEqual(report["protocol"]["barcode_tag"], "RX")
        self.assertEqual(report["protocol"]["read_name_regex"], "null")
        self.assertEqual(report["tools"]["fake"]["status"], "complete")
        self.assertEqual(report["tools"]["fake"]["parity"]["status"], "REFERENCE")
        self.assertEqual(report["required_tool_gate"]["status"], "PASS")
        run = report["tools"]["fake"]["runs"][0]
        self.assertGreater(run["peak_rss_bytes"], 0)
        self.assertGreaterEqual(run["temporary_disk_peak_bytes"], 16384)
        self.assertEqual(len(report["input"]["sha256"]), 64)

    def test_required_tool_gate_fails_for_unselected_tool(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.bam"
            input_path.write_bytes(b"fixture")
            result = module.main(
                [
                    "--input", str(input_path),
                    "--output-dir", str(root / "evidence"),
                    "--tools", "",
                    "--require-tools", "picard",
                    "--repeats", "1",
                    "--warmups", "0",
                ]
            )
            report = json.loads((root / "evidence" / "report.json").read_text())
        self.assertEqual(result, 3)
        self.assertEqual(report["required_tool_gate"]["status"], "FAIL")
        self.assertIn("picard: not selected", report["required_tool_gate"]["failures"])

    def test_cram_requires_reference_fasta(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.cram"
            input_path.write_bytes(b"fixture")
            with self.assertRaises(SystemExit):
                module.main(
                    [
                        "--input", str(input_path),
                        "--output-dir", str(root / "evidence"),
                        "--tools", "",
                    ]
                )

    def test_cram_report_records_reference_provenance(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.cram"
            reference = root / "reference.fa"
            input_path.write_bytes(b"cram-fixture")
            reference.write_text(">chr1\nACGT\n", encoding="utf-8")
            fake = root / "fake_tool.py"
            fake.write_text(
                textwrap.dedent(
                    """
                    import pathlib, shutil, sys
                    if '--version' in sys.argv:
                        print('fake-markdup 1.2.3')
                        raise SystemExit(0)
                    source, output, metrics, temporary = map(pathlib.Path, sys.argv[1:5])
                    temporary.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(source, output)
                    metrics.write_text('fake metrics\\n')
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            fake.chmod(fake.stat().st_mode | 0o111)
            output_dir = root / "evidence"
            result = module.main(
                [
                    "--input", str(input_path),
                    "--reference-fasta", str(reference),
                    "--output-dir", str(output_dir),
                    "--tools", "",
                    "--tool", f"fake={sys.executable} {fake} {{input}} {{output}} {{metrics}} {{tmp}}",
                    "--reference-tool", "fake",
                    "--require-tools", "fake",
                    "--repeats", "1",
                    "--warmups", "0",
                ]
            )
            report = json.loads((output_dir / "report.json").read_text(encoding="utf-8"))
        self.assertEqual(result, 0)
        self.assertEqual(report["input"]["format"], "CRAM")
        self.assertEqual(report["input"]["reference_fasta"]["bytes"], len(">chr1\nACGT\n"))
        self.assertEqual(report["required_tool_gate"]["status"], "PASS")


if __name__ == "__main__":
    unittest.main()
