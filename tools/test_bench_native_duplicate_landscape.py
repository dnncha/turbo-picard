from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import TYPE_CHECKING
from unittest.mock import patch

import pytest

if TYPE_CHECKING:
    from tools.bench_native_duplicate_landscape import main, pipeline_commands, run_pipeline
else:
    from bench_native_duplicate_landscape import main, pipeline_commands, run_pipeline


class NativeLandscapeTests(unittest.TestCase):
    def test_validation_only_samtools_has_provenance_without_being_measured(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "tool"
            binary.write_bytes(b"fixture executable")
            source = root / "input.bam"
            source.write_bytes(b"fixture input")
            output = root / "evidence"
            argv = [
                "bench_native_duplicate_landscape.py",
                "--candidate",
                str(binary),
                "--picard",
                str(binary),
                "--samtools",
                str(binary),
                "--fastdup",
                str(binary),
                "--tools",
                "fastdup",
                "--input",
                f"sample={source}",
                "--output-dir",
                str(output),
                "--candidate-source",
                "test",
                "--repeats",
                "1",
            ]
            version = subprocess.CompletedProcess([], 0, "fixture version\n", "")
            with (
                patch.object(sys, "argv", argv),
                patch("bench_native_duplicate_landscape.os.sched_getaffinity", return_value={0}, create=True),
                patch("bench_native_duplicate_landscape.subprocess.run", return_value=version),
                patch(
                    "bench_native_duplicate_landscape.measure",
                    side_effect=lambda *args: {"exit_code": 1, "wall_seconds": 0.0},
                ),
            ):
                assert main() == 1
            report = json.loads((output / "report.json").read_text())
            validator = report["executables"]["samtools"]
            assert validator["path"] == str(binary.resolve())
            assert validator["sha256"] == hashlib.sha256(binary.read_bytes()).hexdigest()
            assert validator["version"] == "fixture version\n"
            assert validator["version_exit"] == 0
            assert report["selected_competitors"] == ["fastdup"]
            assert "samtools" not in {row["tool"] for row in report["runs"]}

    def test_selected_competitors_are_required_before_creating_outputs(self) -> None:
        script = Path(__file__).with_name("bench_native_duplicate_landscape.py")
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "evidence"
            common = [
                sys.executable,
                str(script),
                "--candidate",
                "candidate",
                "--picard",
                "picard",
                "--samtools",
                "samtools",
                "--input",
                "sample=input.bam",
                "--output-dir",
                str(output),
                "--candidate-source",
                "test",
            ]
            for options, error in [
                ([], "--tools fastdup requires --fastdup"),
                (["--tools", "dupblaster"], "--tools dupblaster requires --dupblaster"),
                (["--tools", "samtools", "samtools"], "--tools must not contain duplicates"),
            ]:
                result = subprocess.run([*common, *options], capture_output=True, text=True, check=False)
                assert result.returncode == 2
                assert error in result.stderr
                assert not output.exists()

    def test_preparation_and_coordinate_output_are_inside_both_pipelines(self) -> None:
        for tool in ("samtools", "dupblaster"):
            commands = pipeline_commands(
                tool,
                "dupblaster",
                "samtools",
                source=Path("original.bam"),
                output=Path("result.bam"),
                metrics=Path("metrics"),
                scratch=Path("scratch"),
            )
            assert commands[0][1] == "collate"
            assert "original.bam" in commands[0]
            assert "result.bam" in commands[-1]
            sort = next(command for command in commands if command[1] == "sort")
            assert sort[sort.index("-m") + 1] == "256M"
            assert sort[sort.index("-T") + 1] == "scratch/sort"
        with pytest.raises(ValueError, match="unknown pipeline"):
            pipeline_commands("unknown", "", "", source=Path(""), output=Path(""), metrics=Path(""), scratch=Path(""))

    def test_upstream_failure_is_not_masked_by_successful_last_stage(self) -> None:
        assert (
            run_pipeline(
                [[sys.executable, "-c", "raise SystemExit(7)"], [sys.executable, "-c", "import sys;sys.stdin.read()"]]
            )
            == 7
        )
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "result"
            assert (
                run_pipeline(
                    [
                        [sys.executable, "-c", 'print("payload")'],
                        [
                            sys.executable,
                            "-c",
                            "import pathlib,sys;pathlib.Path(sys.argv[1]).write_text(sys.stdin.read())",
                            str(target),
                        ],
                    ]
                )
                == 0
            )
            assert target.read_text() == "payload\n"
