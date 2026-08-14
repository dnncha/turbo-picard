from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile
import textwrap
import unittest


MODULE_PATH = Path(__file__).with_name("audit_public_adoption.py")
SPEC = importlib.util.spec_from_file_location("audit_public_adoption", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
audit_public_adoption = importlib.util.module_from_spec(SPEC)
sys.modules["audit_public_adoption"] = audit_public_adoption
SPEC.loader.exec_module(audit_public_adoption)


class AuditPublicAdoptionTests(unittest.TestCase):
    def test_collect_release_state_reports_dirty_and_mismatched_tags(self) -> None:
        responses = {
            ("status", "--porcelain"): (0, [" M README.md"]),
            ("rev-parse", "HEAD"): (0, ["head" * 8]),
            ("rev-parse", "--abbrev-ref", "HEAD"): (0, ["main"]),
            ("rev-list", "-n", "1", "v0.1.12"): (0, ["tag" * 8]),
            ("ls-remote", "--tags", "origin", "v0.1.12*"): (
                0,
                ["origin" * 8 + "\trefs/tags/v0.1.12^{}"],
            ),
        }

        def fake_git(args: list[str], _root: Path) -> tuple[int, list[str]]:
            return responses[tuple(args)]

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "Cargo.toml").write_text(
                "[workspace.package]\nversion = \"0.1.12\"\n",
                encoding="utf-8",
            )
            state = audit_public_adoption.collect_release_state(root, git_runner=fake_git)

        self.assertFalse(state["worktree_clean"])
        self.assertFalse(state["tag_matches_head"])
        self.assertFalse(state["origin_tag_matches_local"])
        self.assertFalse(state["release_source_ready"])
        self.assertIn("worktree has uncommitted changes", state["blockers"])
        self.assertIn(
            "local release tag v0.1.12 does not point at current HEAD",
            state["blockers"],
        )

    def test_collect_release_state_accepts_clean_annotated_remote_tag(self) -> None:
        commit = "a" * 40
        responses = {
            ("status", "--porcelain"): (0, []),
            ("rev-parse", "HEAD"): (0, [commit]),
            ("rev-parse", "--abbrev-ref", "HEAD"): (0, ["release"]),
            ("rev-list", "-n", "1", "v0.1.12"): (0, [commit]),
            ("ls-remote", "--tags", "origin", "v0.1.12*"): (
                0,
                [commit + "\trefs/tags/v0.1.12", commit + "\trefs/tags/v0.1.12^{}"],
            ),
        }

        def fake_git(args: list[str], _root: Path) -> tuple[int, list[str]]:
            return responses[tuple(args)]

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "Cargo.toml").write_text(
                "[workspace.package]\nversion = \"0.1.12\"\n",
                encoding="utf-8",
            )
            state = audit_public_adoption.collect_release_state(root, git_runner=fake_git)

        self.assertTrue(state["worktree_clean"])
        self.assertTrue(state["tag_matches_head"])
        self.assertTrue(state["origin_tag_matches_local"])
        self.assertTrue(state["git_queries_succeeded"])
        self.assertTrue(state["release_source_ready"])
        self.assertEqual(state["blockers"], [])

    def test_parse_downloads_ignores_mirror_category_and_summarizes_windows(self) -> None:
        payload = {
            "package": "turbo-picard",
            "data": [
                {"category": "without_mirrors", "date": "2026-08-01", "downloads": 2},
                {"category": "without_mirrors", "date": "2026-08-02", "downloads": 3},
                {"category": "with_mirrors", "date": "2026-08-02", "downloads": 999},
                {"category": "without_mirrors", "date": "2026-08-13", "downloads": 5},
            ],
        }
        result = audit_public_adoption.parse_downloads(payload)
        self.assertEqual(result["downloads_total"], 10)
        self.assertEqual(result["latest_day_downloads"], 5)
        self.assertEqual(result["downloads_last_7_days"], 5)
        self.assertEqual(result["period_start"], "2026-08-01")
        self.assertEqual(result["period_end"], "2026-08-13")

    def test_parse_downloads_rejects_missing_without_mirrors_rows(self) -> None:
        with self.assertRaisesRegex(ValueError, "without_mirrors"):
            audit_public_adoption.parse_downloads(
                {"data": [{"category": "with_mirrors", "date": "2026-08-01", "downloads": 1}]}
            )

    def test_parse_pypi_metadata_records_workspace_drift(self) -> None:
        result = audit_public_adoption.parse_pypi_metadata(
            {
                "info": {
                    "name": "turbo-picard",
                    "version": "0.1.11",
                    "description": "old README\n",
                    "description_content_type": "text/markdown",
                    "requires_python": ">=3.8",
                },
                "urls": [{"upload_time_iso_8601": "2026-07-25T21:00:00Z"}],
            },
            expected_version="0.1.12",
            readme="new README\n",
        )
        self.assertFalse(result["version_matches_workspace"])
        self.assertFalse(result["long_description_matches_workspace"])
        self.assertEqual(result["latest_file_upload_time"], "2026-07-25T21:00:00Z")

    def test_parse_open_issues_excludes_pull_requests(self) -> None:
        result = audit_public_adoption.parse_open_issues(
            [
                {
                    "number": 4,
                    "html_url": "https://github.com/dnncha/turbo-picard/issues/4",
                    "state": "open",
                    "comments": 2,
                },
                {
                    "number": 20,
                    "html_url": "https://github.com/dnncha/turbo-picard/pull/20",
                    "pull_request": {},
                },
            ]
        )
        self.assertEqual(result["open_issue_count_excluding_pull_requests"], 1)
        self.assertEqual(result["open_issue_numbers"], [4])
        self.assertEqual(result["pull_requests_excluded"], 1)
        self.assertEqual(result["trial_report_thread"]["comment_count"], 2)

    def test_parse_open_issues_records_missing_trial_thread_without_claiming_activity(self) -> None:
        result = audit_public_adoption.parse_open_issues(
            [{"number": 5, "html_url": "https://github.com/dnncha/turbo-picard/issues/5"}]
        )
        self.assertEqual(result["trial_report_thread"]["state"], "not_in_open_response")
        self.assertIsNone(result["trial_report_thread"]["comment_count"])

    def test_collect_report_combines_signals_without_network(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "Cargo.toml").write_text(
                textwrap.dedent(
                    """
                    [workspace.package]
                    version = "0.1.12"
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            (root / "README.md").write_text("README\n", encoding="utf-8")
            payloads = {
                audit_public_adoption.PYPI_JSON_URL: {
                    "info": {
                        "name": "turbo-picard",
                        "version": "0.1.12",
                        "description": "README\n",
                        "description_content_type": "text/markdown",
                    },
                    "urls": [],
                },
                audit_public_adoption.PYPISTATS_OVERALL_URL: {
                    "package": "turbo-picard",
                    "data": [
                        {"category": "without_mirrors", "date": "2026-08-13", "downloads": 3}
                    ],
                },
                audit_public_adoption.GITHUB_REPOSITORY_URL: {
                    "full_name": "dnncha/turbo-picard",
                    "html_url": "https://github.com/dnncha/turbo-picard",
                    "default_branch": "main",
                    "stargazers_count": 0,
                    "forks_count": 0,
                    "watchers_count": 0,
                    "subscribers_count": 0,
                    "open_issues_count": 1,
                },
                audit_public_adoption.GITHUB_ISSUES_URL: [
                    {
                        "number": 4,
                        "html_url": "https://github.com/dnncha/turbo-picard/issues/4",
                        "state": "open",
                        "comments": 1,
                    }
                ],
            }

            def fake_fetch(url: str, **_kwargs: object) -> object:
                return payloads[url]

            report = audit_public_adoption.collect_report(
                root,
                fetcher=fake_fetch,
                observed_at_utc="2026-08-14T12:00:00Z",
            )
            self.assertEqual(report["package"]["live_version"], "0.1.12")
            self.assertTrue(report["package"]["long_description_matches_workspace"])
            self.assertEqual(report["downloads"]["downloads_total"], 3)
            self.assertEqual(report["community"]["open_issue_numbers"], [4])
            self.assertEqual(report["community"]["trial_report_thread"]["comment_count"], 1)
            self.assertEqual(report["schema_version"], 2)
            self.assertIn("release_state", report)
            self.assertFalse(report["interpretation"]["release_source_ready_verified"])
            self.assertFalse(report["interpretation"]["sustained_external_usage_verified"])
            self.assertFalse(report["interpretation"]["workflow_owner_trial_reports_verified"])


if __name__ == "__main__":
    unittest.main()
