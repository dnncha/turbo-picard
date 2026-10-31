import argparse
import importlib.util
import json
import os
import sys
import tempfile
import textwrap
import time
import unittest
from pathlib import Path
from unittest.mock import patch

import pytest

MODULE_PATH = Path(__file__).with_name("bench_markduplicates_competitors.py")
SPEC = importlib.util.spec_from_file_location("bench_markduplicates_competitors", MODULE_PATH)
assert SPEC is not None
assert SPEC.loader is not None
module = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = module
SPEC.loader.exec_module(module)


class CompetitorBenchmarkTests(unittest.TestCase):
    def test_gnu_time_probe_accepts_capitalized_banner(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            fake_time = Path(tmp) / "time"
            fake_time.write_text("#!/bin/sh\nprintf '(GNU Time) 1.9\\n'\n", encoding="utf-8")
            fake_time.chmod(0o755)
            with patch.object(module, "GNU_TIME", fake_time):
                assert module.gnu_time_available()

    @unittest.skipUnless(module.gnu_time_available(), "requires GNU time")
    def test_gnu_time_timeout_kills_the_measured_child(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = root / "surviving-child"
            script = "import pathlib,sys,time; time.sleep(0.5); pathlib.Path(sys.argv[1]).write_text('survived')"
            with (root / "stdout").open("wb") as stdout, (root / "stderr").open("wb") as stderr:
                code, _resources, error, backend = module.run_metered(
                    [sys.executable, "-c", script, str(output)],
                    stdout,
                    stderr,
                    root / "resources",
                    timeout=0.1,
                    environment=os.environ.copy(),
                )
            assert code is None
            assert error is not None
            assert backend == "gnu-time"
            time.sleep(0.6)
            assert not output.exists()

    def test_full_sam_contract_detects_data_changes_but_excludes_pg(self) -> None:
        baseline = "read1\t0\tchr1\t1\t60\t1M\t*\t0\t0\tA\tF\tRG:Z:rg1\tZZ:i:1\n"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            reference, candidate, metrics = root / "ref.sam", root / "out.sam", root / "metrics"
            reference.write_text(baseline)
            metrics.write_text("")
            for old, new in (
                ("\tA\t", "\tC\t"),
                ("\tF\t", "\t!\t"),
                ("RG:Z:rg1", "RG:Z:rg2"),
                ("\t0\tchr1", "\t256\tchr1"),
                ("\t60\t", "\t20\t"),
                ("ZZ:i:1", "ZZ:Z:1"),
            ):
                with self.subTest(field=old):
                    candidate.write_text(baseline.replace(old, new))
                    assert module.compare_outputs(reference, candidate, metrics, metrics)["status"] == "FAIL"
            candidate.write_text(baseline.rstrip() + "\tPG:Z:tool\n")
            assert module.compare_outputs(reference, candidate, metrics, metrics)["status"] == "PASS"

    def test_auxiliary_tag_order_does_not_change_parity(self) -> None:
        record = "read1\t0\tchr1\t1\t60\t1M\t*\t0\t0\tA\tF"
        assert module.parse_sam_fields(record + "\tRG:Z:rg1\tZZ:i:1\n") == module.parse_sam_fields(
            record + "\tZZ:i:1\tRG:Z:rg1\n"
        )

    def test_later_repeat_mismatch_fails_candidate_and_reference(self) -> None:
        record = "read1\t0\tchr1\t1\t60\t1M\t*\t0\t0\tA\tF\n"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name, text in (
                ("reference.sam", record),
                ("good.sam", record),
                ("bad.sam", record.replace("\t0\tchr1", "\t1024\tchr1")),
                ("metrics", ""),
            ):
                (root / name).write_text(text)
            reference = {
                "warmup": False,
                "status": "success",
                "repeat": 1,
                "output": "reference.sam",
                "metrics": "metrics",
            }
            good = {**reference, "output": "good.sam"}
            bad = {**reference, "repeat": 2, "output": "bad.sam"}
            for first in (good, reference):
                tool = {"runs": [{**bad, "warmup": True}, first, bad]}
                result = module.check_measured_parity(tool, reference, root)
                assert result["status"] == "FAIL"
                assert result["repeat"] == 2
                assert result["checked_repeats"] == 2
                assert result["alignment_mismatch"]["record_index"] == 0

    def test_turbo_runs_remove_inherited_overrides_and_require_native(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            script = root / "fake.py"
            script.write_text(
                textwrap.dedent("""
                import os,pathlib,sys
                assert os.environ['TURBO_PICARD_REQUIRE_NATIVE'] == '1'
                assert os.environ['TURBO_PICARD_THREADS'] == '2'
                assert 'TURBO_PICARD_WRITER_THREADS' not in os.environ
                assert 'TURBO_PICARD_FALLBACK_COMMAND' not in os.environ
                assert os.environ['TMPDIR'] == sys.argv[2]
                pathlib.Path(sys.argv[1]).write_bytes(b'output')
                """)
            )
            spec = module.ToolSpec(
                "turbo-picard",
                (sys.executable, str(script), "{output}", "{tmp}"),
                (sys.executable, "--version"),
                "picard",
                (("TURBO_PICARD_THREADS", "{threads}"),),
            )
            with patch.dict(
                os.environ, {"TURBO_PICARD_WRITER_THREADS": "999", "TURBO_PICARD_FALLBACK_COMMAND": "fake"}
            ):
                result = module.run_once(
                    spec,
                    root / "input.bam",
                    root / "runs/turbo-picard/repeat-1",
                    repeat=1,
                    warmup=False,
                    threads=2,
                    sample_seconds=0.01,
                    timeout=10,
                )
            assert result.status == "success", (root / result.stderr_log).read_text()
            assert "TURBO_PICARD_WRITER_THREADS" in result.removed_environment
            assert result.environment["TURBO_PICARD_REQUIRE_NATIVE"] == "1"
            assert (root / "runs/turbo-picard/repeat-1/removed-environment.json").is_file()

    def test_samtools_spill_prefix_is_inside_measured_tmp(self) -> None:
        with patch.object(module, "executable_path", return_value="/fake/tool"):
            command = module.preset_tools()["samtools"].command
        assert command[command.index("-T") + 1] == "{tmp}/samtools"

    def test_presets_cover_coordinate_sorted_markdup_tools(self) -> None:
        assert set(module.preset_tools()) == {"turbo-picard", "picard", "samtools", "sambamba", "fastdup"}

    def test_presets_can_use_tool_default_read_name_regex(self) -> None:
        with patch.object(module, "executable_path", return_value="/bin/true"):
            presets = module.preset_tools(None)
        assert "READ_NAME_REGEX=" not in presets["picard"].command

    def test_presets_add_reference_arguments_for_cram(self) -> None:
        with patch.object(module, "executable_path", return_value="/bin/true"):
            presets = module.preset_tools("null", Path("reference.fa"))
        assert "REFERENCE_SEQUENCE={reference}" in presets["picard"].command
        assert "--reference" in presets["samtools"].command

    def test_presets_can_request_duplicate_set_member_tags(self) -> None:
        with patch.object(module, "executable_path", return_value="/bin/true"):
            presets = module.preset_tools("null", None, tag_duplicate_set_members=True)
        assert "TAG_DUPLICATE_SET_MEMBERS=true" in presets["picard"].command
        assert "TAG_DUPLICATE_SET_MEMBERS=true" in presets["turbo-picard"].command

    def test_presets_can_request_primary_and_mate_specific_barcode_tags(self) -> None:
        with patch.object(module, "executable_path", return_value="/bin/true"):
            presets = module.preset_tools(
                "null", None, barcode_tag="RX", read_one_barcode_tag="BX", read_two_barcode_tag="BY"
            )
        for name in ("picard", "turbo-picard"):
            assert "BARCODE_TAG=RX" in presets[name].command
            assert "READ_ONE_BARCODE_TAG=BX" in presets[name].command
            assert "READ_TWO_BARCODE_TAG=BY" in presets[name].command

    def test_profile_contract_requires_umi_barcode_tags(self) -> None:
        with pytest.raises(SystemExit):
            module.validate_profile("umi_panel", "BAM", None, (None, None, None))
        module.validate_profile("umi_panel", "BAM", None, ("RX", None, None))

    def test_profile_contract_requires_cram_for_cram_profile(self) -> None:
        with pytest.raises(SystemExit):
            module.validate_profile("cram_reference", "BAM", None, (None, None, None))

    def test_command_expansion_preserves_regex_quantifier_braces(self) -> None:
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
        assert "(?:[A-Z]+:){4}([0-9]+)" in expanded[1]
        assert expanded[-1].endswith("reference.fa")

    def test_picard_version_probe_invokes_markduplicates(self) -> None:
        spec = module.ToolSpec(
            "picard",
            ("picard", "MarkDuplicates", "I={input}", "O={output}"),
            ("picard", "MarkDuplicates", "--version"),
            "picard",
        )
        assert spec.version_command[-2:] == ("MarkDuplicates", "--version")

    def test_custom_template_requires_input_and_output(self) -> None:
        with pytest.raises(argparse.ArgumentTypeError, match="must contain"):
            module.parse_custom_tool("broken=tool --input {input}")

    def test_expand_command_does_not_invoke_a_shell(self) -> None:
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
        assert str(Path("input with spaces.bam").resolve()) in expanded
        assert expanded[-1] == "7"

    def test_parse_gnu_time_record(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            record = Path(tmp) / "time.tsv"
            record.write_text("TPBENCH\t1.25\t2.5\t0.25\t1024\t0\n", encoding="utf-8")
            parsed = module.parse_time_file(record)
        assert parsed["wall_seconds"] == 1.25
        assert parsed["peak_rss_bytes"] == 1024 * 1024
        assert parsed["exit_code"] == 0

    def test_non_gnu_time_is_not_used_as_gnu_time(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            fake_time = Path(tmp) / "time"
            fake_time.write_text("#!/bin/sh\nprintf 'BSD time\\n'\n", encoding="utf-8")
            fake_time.chmod(fake_time.stat().st_mode | 0o111)
            with patch.object(module, "GNU_TIME", fake_time):
                assert not module.gnu_time_available()

    def test_streaming_sam_comparison_detects_winner_change(self) -> None:
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
        assert comparison["status"] == "FAIL"
        assert comparison["alignment_mismatch"]["record_index"] == 0

    def test_bam_parity_falls_back_to_samtools_without_pysam(self) -> None:
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
            with (
                patch.dict(sys.modules, {"pysam": None}),
                patch.object(module.shutil, "which", return_value=str(fake_samtools)),
            ):
                records = list(module.sam_fields(bam))
        assert records[0][0] == "read1"
        assert records[0][1]
        assert records[0][8] == "LB"

    def test_end_to_end_bundle_records_resources_and_provenance(self) -> None:
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
                    "--input",
                    str(input_path),
                    "--output-dir",
                    str(output_dir),
                    "--tools",
                    "",
                    "--tool",
                    f"fake={sys.executable} {fake} {{input}} {{output}} {{metrics}} {{tmp}}",
                    "--reference-tool",
                    "fake",
                    "--profile",
                    "umi_panel",
                    "--barcode-tag",
                    "RX",
                    "--repeats",
                    "1",
                    "--warmups",
                    "0",
                    "--disk-sample-ms",
                    "10",
                ]
            )
            report = json.loads((output_dir / "report.json").read_text(encoding="utf-8"))
        assert result == 0
        assert report["claim_status"] == "evidence_only"
        assert report["protocol"]["profile"] == "umi_panel"
        assert report["protocol"]["barcode_tag"] == "RX"
        assert report["protocol"]["read_name_regex"] == "null"
        assert report["tools"]["fake"]["status"] == "complete"
        assert report["tools"]["fake"]["parity"]["status"] == "REFERENCE"
        assert report["required_tool_gate"]["status"] == "PASS"
        run = report["tools"]["fake"]["runs"][0]
        assert run["peak_rss_bytes"] > 0
        assert run["temporary_disk_peak_bytes"] >= 16384
        assert len(report["input"]["sha256"]) == 64

    def test_required_tool_gate_fails_for_unselected_tool(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.bam"
            input_path.write_bytes(b"fixture")
            result = module.main(
                [
                    "--input",
                    str(input_path),
                    "--output-dir",
                    str(root / "evidence"),
                    "--tools",
                    "",
                    "--require-tools",
                    "picard",
                    "--repeats",
                    "1",
                    "--warmups",
                    "0",
                ]
            )
            report = json.loads((root / "evidence" / "report.json").read_text())
        assert result == 3
        assert report["required_tool_gate"]["status"] == "FAIL"
        assert "picard: not selected" in report["required_tool_gate"]["failures"]

    def test_cram_requires_reference_fasta(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.cram"
            input_path.write_bytes(b"fixture")
            with pytest.raises(SystemExit):
                module.main(
                    [
                        "--input",
                        str(input_path),
                        "--output-dir",
                        str(root / "evidence"),
                        "--tools",
                        "",
                    ]
                )

    def test_cram_report_records_reference_provenance(self) -> None:
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
                    "--input",
                    str(input_path),
                    "--reference-fasta",
                    str(reference),
                    "--output-dir",
                    str(output_dir),
                    "--tools",
                    "",
                    "--tool",
                    f"fake={sys.executable} {fake} {{input}} {{output}} {{metrics}} {{tmp}}",
                    "--reference-tool",
                    "fake",
                    "--require-tools",
                    "fake",
                    "--repeats",
                    "1",
                    "--warmups",
                    "0",
                ]
            )
            report = json.loads((output_dir / "report.json").read_text(encoding="utf-8"))
        assert result == 0
        assert report["input"]["format"] == "CRAM"
        assert report["input"]["reference_fasta"]["bytes"] == len(">chr1\nACGT\n")
        assert report["required_tool_gate"]["status"] == "PASS"


if __name__ == "__main__":
    unittest.main()
