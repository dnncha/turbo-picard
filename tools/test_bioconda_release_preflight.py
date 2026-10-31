#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path
from unittest import mock

MODULE_PATH = Path(__file__).with_name("bioconda_release_preflight.py")
SPEC = importlib.util.spec_from_file_location("bioconda_release_preflight", MODULE_PATH)
assert SPEC is not None
bioconda_release_preflight = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["bioconda_release_preflight"] = bioconda_release_preflight
SPEC.loader.exec_module(bioconda_release_preflight)


class BiocondaReleasePreflightTests(unittest.TestCase):
    def test_reports_expected_archive_wait_state(self) -> None:
        def fake_run(command: list[str], root: Path) -> tuple[int, list[str]]:
            if command[:3] == ["git", "status", "--porcelain"]:
                return 0, []
            if command[:4] == ["git", "show-ref", "--verify", "--quiet"]:
                return 0, []
            if command[:4] == ["git", "rev-list", "-n", "1"]:
                return 0, ["abc123"]
            if command[:2] == ["git", "rev-parse"]:
                return 0, ["abc123"]
            if command[:4] == ["git", "ls-remote", "--tags", "origin"]:
                return 0, ["abc123\trefs/tags/v0.1.0"]
            if command[-1] == "--release-ready" and command[1].endswith("verify_bioconda_recipes.py"):
                return 1, sorted(bioconda_release_preflight.EXPECTED_ARCHIVE_ERRORS)
            return 0, []

        with (
            mock.patch.object(bioconda_release_preflight, "run_check", fake_run),
            mock.patch.object(bioconda_release_preflight, "workspace_version", return_value="0.1.0"),
        ):
            status, report = bioconda_release_preflight.preflight_report(Path("/tmp/repo"))

        assert status == 1
        assert "OK: release tag v0.1.0 exists locally and on origin" in report
        assert "OK: real-data release evidence" in report
        assert "WAIT: Bioconda release-ready source metadata" in report
        assert "prepare_bioconda_release.py --archive" in report

    def test_reports_unexpected_release_ready_failure(self) -> None:
        def fake_run(command: list[str], root: Path) -> tuple[int, list[str]]:
            if command[:3] == ["git", "status", "--porcelain"]:
                return 0, []
            if command[:4] == ["git", "show-ref", "--verify", "--quiet"]:
                return 0, []
            if command[:4] == ["git", "rev-list", "-n", "1"]:
                return 0, ["abc123"]
            if command[:2] == ["git", "rev-parse"]:
                return 0, ["abc123"]
            if command[:4] == ["git", "ls-remote", "--tags", "origin"]:
                return 0, ["abc123\trefs/tags/v0.1.0"]
            if command[-1] == "--release-ready" and command[1].endswith("verify_bioconda_recipes.py"):
                return 1, ["unexpected packaging error"]
            return 0, []

        with (
            mock.patch.object(bioconda_release_preflight, "run_check", fake_run),
            mock.patch.object(bioconda_release_preflight, "workspace_version", return_value="0.1.0"),
        ):
            status, report = bioconda_release_preflight.preflight_report(Path("/tmp/repo"))

        assert status == 1
        assert "FAIL: Bioconda release-ready source metadata" in report
        assert "unexpected packaging error" in report

    def test_reports_success_when_all_checks_pass(self) -> None:
        def fake_run(command: list[str], root: Path) -> tuple[int, list[str]]:
            if command[:4] == ["git", "rev-list", "-n", "1"]:
                return 0, ["abc123"]
            if command[:2] == ["git", "rev-parse"]:
                return 0, ["abc123"]
            if command[:4] == ["git", "ls-remote", "--tags", "origin"]:
                return 0, ["abc123\trefs/tags/v0.1.0"]
            return 0, []

        with (
            mock.patch.object(bioconda_release_preflight, "run_check", fake_run),
            mock.patch.object(bioconda_release_preflight, "workspace_version", return_value="0.1.0"),
        ):
            status, report = bioconda_release_preflight.preflight_report(Path("/tmp/repo"))

        assert status == 0
        assert "OK: git worktree clean for release tagging" in report
        assert "OK: Bioconda release-ready source metadata" in report

    def test_reports_dirty_git_wait_state(self) -> None:
        def fake_run(command: list[str], root: Path) -> tuple[int, list[str]]:
            if command[:3] == ["git", "status", "--porcelain"]:
                return 0, [" M README.md", "?? CITATION.cff"]
            if command[:4] == ["git", "show-ref", "--verify", "--quiet"]:
                return 0, []
            if command[:4] == ["git", "rev-list", "-n", "1"]:
                return 0, ["abc123"]
            if command[:2] == ["git", "rev-parse"]:
                return 0, ["abc123"]
            if command[:4] == ["git", "ls-remote", "--tags", "origin"]:
                return 0, ["abc123\trefs/tags/v0.1.0"]
            return 0, []

        with (
            mock.patch.object(bioconda_release_preflight, "run_check", fake_run),
            mock.patch.object(bioconda_release_preflight, "workspace_version", return_value="0.1.0"),
        ):
            status, report = bioconda_release_preflight.preflight_report(Path("/tmp/repo"))

        assert status == 1
        assert "WAIT: git worktree has uncommitted changes" in report
        assert "Commit the intended release state before tagging." in report
        assert "M README.md" in report

    def test_reports_git_status_failure(self) -> None:
        def fake_run(command: list[str], root: Path) -> tuple[int, list[str]]:
            if command[:3] == ["git", "status", "--porcelain"]:
                return 128, ["not a git repository"]
            if command[:4] == ["git", "show-ref", "--verify", "--quiet"]:
                return 0, []
            if command[:4] == ["git", "rev-list", "-n", "1"]:
                return 0, ["abc123"]
            if command[:2] == ["git", "rev-parse"]:
                return 0, ["abc123"]
            if command[:4] == ["git", "ls-remote", "--tags", "origin"]:
                return 0, ["abc123\trefs/tags/v0.1.0"]
            return 0, []

        with (
            mock.patch.object(bioconda_release_preflight, "run_check", fake_run),
            mock.patch.object(bioconda_release_preflight, "workspace_version", return_value="0.1.0"),
        ):
            status, report = bioconda_release_preflight.preflight_report(Path("/tmp/repo"))

        assert status == 1
        assert "FAIL: git worktree status" in report
        assert "not a git repository" in report

    def test_reports_missing_local_tag_wait_state(self) -> None:
        def fake_run(command: list[str], root: Path) -> tuple[int, list[str]]:
            if command[:3] == ["git", "status", "--porcelain"]:
                return 0, []
            if command[:4] == ["git", "show-ref", "--verify", "--quiet"]:
                return 1, []
            return 0, []

        with (
            mock.patch.object(bioconda_release_preflight, "run_check", fake_run),
            mock.patch.object(bioconda_release_preflight, "workspace_version", return_value="0.1.0"),
        ):
            status, report = bioconda_release_preflight.preflight_report(Path("/tmp/repo"))

        assert status == 1
        assert "WAIT: release tag not ready" in report
        assert "local tag v0.1.0 does not exist yet" in report

    def test_reports_missing_origin_tag_wait_state(self) -> None:
        def fake_run(command: list[str], root: Path) -> tuple[int, list[str]]:
            if command[:3] == ["git", "status", "--porcelain"]:
                return 0, []
            if command[:4] == ["git", "show-ref", "--verify", "--quiet"]:
                return 0, []
            if command[:4] == ["git", "rev-list", "-n", "1"]:
                return 0, ["abc123"]
            if command[:2] == ["git", "rev-parse"]:
                return 0, ["abc123"]
            if command[:4] == ["git", "ls-remote", "--tags", "origin"]:
                return 0, []
            return 0, []

        with (
            mock.patch.object(bioconda_release_preflight, "run_check", fake_run),
            mock.patch.object(bioconda_release_preflight, "workspace_version", return_value="0.1.0"),
        ):
            status, report = bioconda_release_preflight.preflight_report(Path("/tmp/repo"))

        assert status == 1
        assert "origin tag v0.1.0 does not exist yet" in report

    def test_reports_current_head_mismatch(self) -> None:
        def fake_run(command: list[str], root: Path) -> tuple[int, list[str]]:
            if command[:4] == ["git", "show-ref", "--verify", "--quiet"]:
                return 0, []
            if command[:4] == ["git", "rev-list", "-n", "1"]:
                return 0, ["tag-commit"]
            if command[:2] == ["git", "rev-parse"]:
                return 0, ["head-commit"]
            return 0, []

        with (
            mock.patch.object(bioconda_release_preflight, "run_check", fake_run),
            mock.patch.object(bioconda_release_preflight, "workspace_version", return_value="0.1.0"),
        ):
            status, lines = bioconda_release_preflight.git_tag_status(Path("/tmp/repo"))

        assert status == "WAIT"
        assert "current HEAD head-commit differs from v0.1.0 commit tag-commit" in lines


if __name__ == "__main__":
    unittest.main()
