import importlib.util
import json
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("bench_markduplicates_competitors.py")
SPEC = importlib.util.spec_from_file_location("bench_markduplicates_competitors", MODULE_PATH)
module = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = module
SPEC.loader.exec_module(module)


class CompetitorBenchmarkTests(unittest.TestCase):
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
                    "--repeats", "1",
                    "--warmups", "0",
                    "--disk-sample-ms", "10",
                ]
            )
            report = json.loads((output_dir / "report.json").read_text(encoding="utf-8"))
        self.assertEqual(result, 0)
        self.assertEqual(report["claim_status"], "evidence_only")
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


if __name__ == "__main__":
    unittest.main()
