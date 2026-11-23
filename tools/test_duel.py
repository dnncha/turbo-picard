from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from typing import TYPE_CHECKING, Any

import pytest

if TYPE_CHECKING:
    from tools import duel
else:
    import duel

HOST = {"cpu_model": "Test CPU", "nproc": 8, "mem_gb": 50, "os": "linux"}


def timing(tool: str, wall: float, *, run_id: str = "20261122T000000Z-a", **extra: object) -> dict[str, Any]:
    record: dict[str, Any] = {
        "schema": duel.SCHEMA,
        "kind": "timing",
        "run_id": run_id,
        "git_sha": "abc123",
        "host": HOST,
        "threads": 8,
        "task": "wgs",
        "dataset": "30x",
        "tool": tool,
        "variant": "default",
        "repeat": "1",
        "wall_s": wall,
        "user_s": wall * 2,
        "sys_s": 0.0,
        "peak_tree_rss_bytes": 100 * 2**20,
        "exit_code": 0,
        "thread_cpu_s": {"turbo-picard": wall * 2},
    }
    record.update(extra)
    return record


def tools() -> duel.Tools:
    return duel.Tools("tp", "picard", "riker", "dupblaster", "fastdup", "samtools")


class PlanTests(unittest.TestCase):
    def test_plan_races_every_competitor_and_picard_reference(self) -> None:
        ds = duel.Dataset("4x", "/d/4x.bam", "/d/4x.qg.bam", picard_reference=True, repeats=3, warmup=True)
        plan = duel.build_plan([ds], duel.Context("/r.fa", tools(), 8), "/w", list(duel.TASKS))
        by_task: dict[str, set[str]] = {}
        for trial in plan:
            by_task.setdefault(trial.task, set()).add(trial.tool)
        assert by_task["wgs"] == {"turbo-picard", "riker", "picard"}
        assert by_task["multi"] == {"turbo-picard", "riker", "picard"}
        assert by_task["markdup-coord"] == {"turbo-picard", "fastdup", "samtools", "dupblaster", "picard"}
        assert by_task["markdup-qgroup"] == {"turbo-picard", "dupblaster", "picard"}
        turbo = [t for t in plan if t.tool == "turbo-picard"]
        assert all(t.env.get("TURBO_PICARD_REQUIRE_NATIVE") == "1" for t in turbo)
        assert all(t.repeats == 1 for t in plan if t.tool == "picard")

    def test_picard_bundle_runs_wgs_separately_because_picard_rejects_it_in_cmm(self) -> None:
        ds = duel.Dataset("4x", "/d/4x.bam", None, picard_reference=True, repeats=1, warmup=False)
        plan = duel.build_plan([ds], duel.Context("/r.fa", tools(), 8), "/w", ["multi"])
        picard = next(t for t in plan if t.tool == "picard")
        cmm, wgs = picard.shell.split(" && ")
        assert "PROGRAM=CollectWgsMetrics" not in cmm
        assert "CollectWgsMetrics" in wgs
        assert "COUNT_UNPAIRED=true" in wgs
        turbo = next(t for t in plan if t.tool == "turbo-picard")
        assert "PROGRAM=CollectWgsMetrics" in turbo.shell

    def test_perf_symbols_drop_trailing_report_columns(self) -> None:
        rows = duel.parse_perf_report("    19.40%  turbo-picard  turbo-picard  [.] <GcBias>::new        -      -")
        assert rows[0]["symbol"] == "<GcBias>::new"

    def test_qgroup_task_is_skipped_without_query_grouped_input(self) -> None:
        ds = duel.Dataset("30x", "/d/30x.bam", None, picard_reference=False, repeats=2, warmup=False)
        plan = duel.build_plan([ds], duel.Context("/r.fa", tools(), 8), "/w", ["markdup-qgroup"])
        assert plan == []

    def test_dataset_spec_parses(self) -> None:
        ds = duel.parse_dataset("name=15x,coord=/a.bam,qgroup=/b.bam,picard=true,repeats=2,warmup=false")
        assert ds == duel.Dataset("15x", "/a.bam", "/b.bam", picard_reference=True, repeats=2, warmup=False)


class ParserTests(unittest.TestCase):
    def test_stat_ticks_survive_spaces_in_thread_names(self) -> None:
        stat = "42 (tokio worker 3) S 1 2 3 4 5 6 7 8 9 10 250 50 0 0"
        assert duel.parse_stat_ticks(stat) == ("tokio worker 3", 300)

    def test_status_rss_in_bytes(self) -> None:
        assert duel.parse_status_rss("Name:\tx\nVmRSS:\t  2048 kB\n") == 2048 * 1024

    def test_perf_report_rows_are_ranked(self) -> None:
        text = "\n".join(
            [
                "# Overhead  Command  Shared Object  Symbol",
                "    12.50%  turbo-picard  turbo-picard  [.] wgs_metrics::accumulate",
                "    40.00%  turbo-picard  libdeflate.so  [.] deflate_decompress_bmi2",
                "     1.00%  bash  [kernel.kallsyms]  [k] clear_page",
            ]
        )
        rows = duel.parse_perf_report(text)
        assert rows[0]["symbol"] == "deflate_decompress_bmi2"
        assert [r["pct"] for r in rows] == [40.0, 12.5, 1.0]

    def test_numeric_deviation_matches_snake_case_fields(self) -> None:
        rows = duel.numeric_deviation(
            {"MEAN_COVERAGE": "30.0", "PCT_EXC_DUPE": "0.1", "SAMPLE": "x"},
            {"mean_coverage": "30.0", "pct_exc_dupe": "0.2"},
        )
        assert rows[0][0] == "PCT_EXC_DUPE"
        assert abs(rows[0][3] - 1.0) < 1e-9
        assert rows[1][3] == 0.0

    def test_picard_metrics_parse_first_row(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "m.txt"
            path.write_text("## htsjdk\n\n## METRICS CLASS\tx\nA\tB\n1\t2\n\n")
            assert duel.parse_picard_metrics(path) == {"A": "1", "B": "2"}


class ReportTests(unittest.TestCase):
    def test_speed_loss_names_rival_ratio_and_diagnosis(self) -> None:
        records = [timing("turbo-picard", 200.0), timing("riker", 150.0), timing("picard", 2000.0)]
        markdown, losses = duel.render_scoreboard(records, "20261122T000000Z-a")
        speed = [loss for loss in losses if loss["kind"] == "speed"]
        assert len(speed) == 1
        assert "riker" in speed[0]["summary"]
        assert "1.33x slower" in speed[0]["summary"]
        assert "cores busy" in speed[0]["diagnosis"]
        assert "10.0x" in markdown

    def test_no_loss_when_turbo_wins_everything(self) -> None:
        records = [
            timing("turbo-picard", 100.0, peak_tree_rss_bytes=50 * 2**20),
            timing("riker", 150.0),
        ]
        markdown, losses = duel.render_scoreboard(records, "20261122T000000Z-a")
        assert losses == []
        assert "None." in markdown

    def test_memory_loss_is_reported_even_when_faster(self) -> None:
        records = [
            timing("turbo-picard", 100.0, peak_tree_rss_bytes=400 * 2**20),
            timing("riker", 150.0, peak_tree_rss_bytes=100 * 2**20),
        ]
        _, losses = duel.render_scoreboard(records, "20261122T000000Z-a")
        assert [loss["kind"] for loss in losses] == ["memory"]

    def test_refused_native_path_is_the_worst_speed_loss(self) -> None:
        records = [timing("turbo-picard", 1.0, exit_code=3), timing("dupblaster", 50.0)]
        _, losses = duel.render_scoreboard(records, "20261122T000000Z-a")
        assert losses[0]["kind"] == "refused"

    def test_accuracy_loss_outranks_everything_and_fails_report(self) -> None:
        records = [
            timing("turbo-picard", 200.0),
            timing("riker", 150.0),
            {
                "schema": duel.SCHEMA,
                "kind": "accuracy",
                "run_id": "20261122T000000Z-a",
                "git_sha": "abc123",
                "task": "wgs",
                "dataset": "30x",
                "tool": "turbo-picard",
                "variant": "default",
                "against": "picard",
                "verdict": "differs",
                "detail": {},
            },
        ]
        _, losses = duel.render_scoreboard(records, "20261122T000000Z-a")
        assert losses[0]["kind"] == "accuracy"
        with tempfile.TemporaryDirectory() as tmp:
            results = Path(tmp) / "r.jsonl"
            results.write_text("".join(json.dumps(r) + "\n" for r in records))
            status = duel.main(
                [
                    "report",
                    "--ledger",
                    str(Path(tmp) / "ledger.jsonl"),
                    "--add",
                    str(results),
                    "--scoreboard",
                    str(Path(tmp) / "S.md"),
                    "--losses",
                    str(Path(tmp) / "l.json"),
                ]
            )
            assert status == 1
            losses_file = json.loads((Path(tmp) / "l.json").read_text())
            assert losses_file["losses"][0]["kind"] == "accuracy"

    def test_trend_against_previous_run(self) -> None:
        records = [
            timing("turbo-picard", 300.0, run_id="20261101T000000Z-a"),
            timing("turbo-picard", 150.0, run_id="20261122T000000Z-b"),
            timing("riker", 200.0, run_id="20261122T000000Z-b"),
        ]
        markdown, _ = duel.render_scoreboard(records, "20261122T000000Z-b")
        assert "300.0s → 150.0s (2.00x)" in markdown

    def test_turbo_only_rerun_still_races_carried_over_rivals(self) -> None:
        records = [
            timing("turbo-picard", 300.0, run_id="20261122T000000Z-a"),
            timing("riker", 200.0, run_id="20261122T000000Z-a"),
            timing("turbo-picard", 150.0, run_id="20261123T000000Z-b"),
        ]
        markdown, losses = duel.render_scoreboard(records, "20261123T000000Z-b")
        assert "riker (from 20261122T000000Z-a)" in markdown
        assert [loss["kind"] for loss in losses] == []
        slower = [*records, timing("turbo-picard", 250.0, run_id="20261124T000000Z-c")]
        _, losses = duel.render_scoreboard(slower, "20261124T000000Z-c")
        assert [loss["kind"] for loss in losses] == ["speed"]

    def test_ledger_rejects_unknown_records(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "l.jsonl"
            path.write_text(json.dumps({"schema": 99, "kind": "timing"}) + "\n")
            with pytest.raises(ValueError, match=r"l\.jsonl:1"):
                duel.load_records([path])

    def test_ledger_add_is_idempotent(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            results = Path(tmp) / "r.jsonl"
            ledger = Path(tmp) / "ledger.jsonl"
            results.write_text(json.dumps(timing("turbo-picard", 1.0)) + "\n" + json.dumps(timing("riker", 2.0)) + "\n")
            for _ in range(2):
                duel.main(
                    [
                        "report",
                        "--ledger",
                        str(ledger),
                        "--add",
                        str(results),
                        "--scoreboard",
                        str(Path(tmp) / "S.md"),
                        "--losses",
                        str(Path(tmp) / "l.json"),
                    ]
                )
            assert len(ledger.read_text().splitlines()) == 2


if __name__ == "__main__":
    unittest.main()
