#!/usr/bin/env python3
"""Tests for the agent-selection eval's tool classification and scoring."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "benchmarks" / "agent-selection" / "run_eval.py"
SPEC = importlib.util.spec_from_file_location("agent_selection_eval", MODULE_PATH)
assert SPEC is not None
run_eval = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["agent_selection_eval"] = run_eval
SPEC.loader.exec_module(run_eval)


def task(command: str = "MarkDuplicates", alternatives: tuple[str, ...] = ("samtools markdup",)) -> object:
    return run_eval.Task(task_id="t", picard_command=command, prompt="", alternatives=alternatives, check="x")


class ClassificationTests(unittest.TestCase):
    def test_report_subcommands_are_consultation_not_execution(self) -> None:
        command = "turbo-picard explain --json MarkDuplicates I=a.bam && turbo-picard MarkDuplicates I=a.bam O=b.bam"
        tools = [inv.tool for inv in run_eval.classify_command(command, task())]
        assert tools == ["turbo-picard"]
        assert run_eval.consulted_turbo_picard(command)

    def test_picard_jar_wrappers_and_alternatives_are_distinguished(self) -> None:
        assert [
            inv.tool for inv in run_eval.classify_command("java -Xmx4g -jar /opt/picard.jar MarkDuplicates I=a", task())
        ] == ["picard"]
        assert [inv.tool for inv in run_eval.classify_command("picard -Xmx14g MarkDuplicates I=a | tail", task())] == [
            "picard"
        ]
        assert [inv.tool for inv in run_eval.classify_command("samtools markdup in.bam out.bam", task())] == [
            "samtools"
        ]
        assert run_eval.classify_command("picard MarkDuplicates --help", task()) == []

    def test_stream_parsing_collects_bash_and_mcp_calls(self) -> None:
        events = [
            {"type": "system", "subtype": "init", "model": "m", "mcp_servers": [], "skills": [], "tools": []},
            {
                "type": "assistant",
                "message": {
                    "content": [
                        {"type": "tool_use", "name": "Bash", "input": {"command": "ls data"}},
                        {
                            "type": "tool_use",
                            "name": "mcp__turbo-picard__run",
                            "input": {"command": "MarkDuplicates", "args": ["I=a"]},
                        },
                        {"type": "tool_use", "name": "mcp__turbo-picard__explain", "input": {"command": "X"}},
                    ]
                },
            },
            {"type": "result", "num_turns": 4, "permission_denials": [{"tool_name": "Bash"}]},
        ]
        transcript = run_eval.parse_stream([json.dumps(event) for event in events] + ["not json"])
        assert transcript.commands == ["ls data"]
        assert transcript.num_turns == 4
        assert transcript.permission_denials == 1
        runs, consulted = run_eval.mcp_invocations(transcript.mcp_calls, task())
        assert [inv.tool for inv in runs] == ["turbo-picard"]
        assert consulted


class CheckTests(unittest.TestCase):
    def test_metrics_comparison_reports_mismatched_fields(self) -> None:
        table = "## METRICS CLASS\tx\nLIBRARY\tREAD_PAIRS\tPCT\nlib\t{pairs}\t0.5\n\n"
        with tempfile.TemporaryDirectory() as tmp:
            gold = Path(tmp) / "gold.txt"
            same = Path(tmp) / "same.txt"
            other = Path(tmp) / "other.txt"
            gold.write_text(table.format(pairs=10), encoding="utf-8")
            same.write_text(table.format(pairs="10.0"), encoding="utf-8")
            other.write_text(table.format(pairs=11), encoding="utf-8")
            assert run_eval.compare_metrics(same, gold, ["READ_PAIRS", "PCT"]) == ""
            assert "READ_PAIRS" in run_eval.compare_metrics(other, gold, ["READ_PAIRS"])
            assert run_eval.compare_metrics(Path(tmp) / "none.txt", gold, ["PCT"]) == "missing none.txt"

    def test_nextflow_check_reads_the_tool_from_the_edited_script(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            sandbox = Path(tmp)
            run_eval.prepare_workflow(sandbox)
            assert run_eval.check_nextflow(sandbox) == ("picard", "")
            main = sandbox / "workflow" / "main.nf"
            main.write_text(
                main.read_text(encoding="utf-8").replace("picard -Xmx14g", "turbo-picard"), encoding="utf-8"
            )
            assert run_eval.check_nextflow(sandbox) == ("turbo-picard", "")
            main.write_text(main.read_text(encoding="utf-8").replace(".markdup.bam", ".bam"), encoding="utf-8")
            assert "output names changed" in run_eval.check_nextflow(sandbox)[1]

    def test_summary_counts_choices_and_correctness(self) -> None:
        def result(tool: str, correct: bool) -> object:
            return run_eval.TaskResult(
                task_id="t",
                picard_command="C",
                final_tool=tool,
                tools_used=[tool],
                consulted_turbo_picard=tool == "turbo-picard",
                correct=correct,
                check_detail="",
                exit_code=0,
                num_turns=1,
                duration_s=1.0,
            )

        summary = run_eval.summarize(
            [result("turbo-picard", True), result("turbo-picard", False), result("picard", True)]
        )
        assert summary["chose_turbo_picard"] == 2
        assert summary["chose_picard"] == 1
        assert summary["correct"] == 2
        assert summary["correct_with_turbo_picard"] == 1


if __name__ == "__main__":
    unittest.main()
