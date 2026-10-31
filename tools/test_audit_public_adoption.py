from __future__ import annotations

import importlib.util
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

import pytest

MODULE_PATH = Path(__file__).with_name("audit_public_adoption.py")
SPEC = importlib.util.spec_from_file_location("audit_public_adoption", MODULE_PATH)
assert SPEC is not None
assert SPEC.loader is not None
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
                '[workspace.package]\nversion = "0.1.12"\n',
                encoding="utf-8",
            )
            state = audit_public_adoption.collect_release_state(root, git_runner=fake_git)

        assert not state["worktree_clean"]
        assert not state["tag_matches_head"]
        assert not state["origin_tag_matches_local"]
        assert not state["release_source_ready"]
        assert "worktree has uncommitted changes" in state["blockers"]
        assert "local release tag v0.1.12 does not point at current HEAD" in state["blockers"]

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
                '[workspace.package]\nversion = "0.1.12"\n',
                encoding="utf-8",
            )
            state = audit_public_adoption.collect_release_state(root, git_runner=fake_git)

        assert state["worktree_clean"]
        assert state["tag_matches_head"]
        assert state["origin_tag_matches_local"]
        assert state["git_queries_succeeded"]
        assert state["release_source_ready"]
        assert state["blockers"] == []

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
        assert result["downloads_total"] == 10
        assert result["latest_day_downloads"] == 5
        assert result["downloads_last_7_days"] == 5
        assert result["period_start"] == "2026-08-01"
        assert result["period_end"] == "2026-08-13"

    def test_parse_downloads_rejects_missing_without_mirrors_rows(self) -> None:
        with pytest.raises(ValueError, match="without_mirrors"):
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
        assert not result["version_matches_workspace"]
        assert not result["long_description_matches_workspace"]
        assert result["latest_file_upload_time"] == "2026-07-25T21:00:00Z"

    def test_parse_distribution_channels_records_release_container_and_missing_conda(self) -> None:
        releases = audit_public_adoption.parse_github_releases(
            [
                {
                    "tag_name": "v0.1.11",
                    "published_at": "2026-07-25T20:37:57Z",
                    "draft": False,
                    "prerelease": False,
                    "html_url": "https://github.com/dnncha/turbo-picard/releases/tag/v0.1.11",
                }
            ],
            expected_version="0.1.12",
        )
        container = audit_public_adoption.parse_ghcr_tags(
            {"name": "dnncha/turbo-picard", "tags": ["0.1.10", "0.1.11", "latest"]},
            expected_version="0.1.12",
        )
        package = audit_public_adoption.parse_anaconda_package(
            None,
            package_name="turbo-picard",
            package_url=audit_public_adoption.ANACONDA_TURBO_PICARD_URL,
            expected_version="0.1.12",
        )
        assert not releases["workspace_release_published"]
        assert releases["latest_published_version"] == "0.1.11"
        assert not container["workspace_version_tag_present"]
        assert container["latest_version"] == "0.1.11"
        assert package["status"] == "not_found"
        assert not package["workspace_version_available"]

    def test_parse_open_issues_excludes_pull_requests(self) -> None:
        result = audit_public_adoption.parse_open_issues(
            [
                {
                    "number": 4,
                    "html_url": "https://github.com/dnncha/turbo-picard/issues/4",
                    "state": "open",
                    "comments": 2,
                    "user": {"login": "dnncha"},
                },
                {
                    "number": 20,
                    "html_url": "https://github.com/dnncha/turbo-picard/pull/20",
                    "pull_request": {},
                },
            ]
        )
        assert result["open_issue_count_excluding_pull_requests"] == 1
        assert result["open_issue_numbers"] == [4]
        assert result["pull_requests_excluded"] == 1
        assert result["trial_report_thread"]["comment_count"] == 2
        assert result["maintainer_authored_issue_numbers"] == [4]
        assert result["external_authored_issue_count"] == 0
        assert result["trial_report_thread"]["author_provenance"] == "maintainer"

    def test_parse_trial_comments_separates_maintainer_and_external_authors(self) -> None:
        result = audit_public_adoption.parse_trial_comments(
            [
                {"user": {"login": "dnncha"}},
                {"user": {"login": "workflow-owner"}},
                {"user": None},
            ]
        )
        assert result["maintainer_comment_count"] == 1
        assert result["external_comment_count"] == 1
        assert result["unknown_comment_count"] == 1
        assert not result["comments_possibly_truncated"]

    def test_parse_open_issues_records_missing_trial_thread_without_claiming_activity(self) -> None:
        result = audit_public_adoption.parse_open_issues(
            [{"number": 5, "html_url": "https://github.com/dnncha/turbo-picard/issues/5"}]
        )
        assert result["trial_report_thread"]["state"] == "not_in_open_response"
        assert result["trial_report_thread"]["comment_count"] is None

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
                    "data": [{"category": "without_mirrors", "date": "2026-08-13", "downloads": 3}],
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
                audit_public_adoption.GITHUB_RELEASES_URL: [
                    {
                        "tag_name": "v0.1.11",
                        "published_at": "2026-07-25T20:37:57Z",
                        "draft": False,
                        "prerelease": False,
                        "html_url": "https://github.com/dnncha/turbo-picard/releases/tag/v0.1.11",
                    }
                ],
                audit_public_adoption.GITHUB_ISSUES_URL: [
                    {
                        "number": 4,
                        "html_url": "https://github.com/dnncha/turbo-picard/issues/4",
                        "state": "open",
                        "comments": 1,
                        "user": {"login": "dnncha"},
                    }
                ],
                audit_public_adoption.GITHUB_TRIAL_COMMENTS_URL: [{"user": {"login": "dnncha"}}],
                audit_public_adoption.GHCR_PULL_AUTH_URL: {"token": "test-token"},
                audit_public_adoption.GHCR_TAGS_URL: {
                    "name": "dnncha/turbo-picard",
                    "tags": ["0.1.11", "latest"],
                },
                audit_public_adoption.ANACONDA_TURBO_PICARD_URL: None,
                audit_public_adoption.ANACONDA_SHIM_URL: None,
                audit_public_adoption.BIOCONDA_PR_URL: {
                    "number": 65922,
                    "state": "open",
                    "title": "Add turbo-picard 0.1.10",
                    "html_url": "https://github.com/bioconda/bioconda-recipes/pull/65922",
                    "updated_at": "2026-07-23T17:22:37Z",
                },
            }

            def fake_fetch(url: str, **_kwargs: object) -> object:
                return payloads[url]

            report = audit_public_adoption.collect_report(
                root,
                fetcher=fake_fetch,
                observed_at_utc="2026-08-14T12:00:00Z",
            )
            assert report["package"]["live_version"] == "0.1.12"
            assert report["package"]["long_description_matches_workspace"]
            assert report["downloads"]["downloads_total"] == 3
            assert report["community"]["open_issue_numbers"] == [4]
            assert report["community"]["trial_report_thread"]["comment_count"] == 1
            assert report["schema_version"] == 4
            assert "release_state" in report
            assert not report["interpretation"]["release_source_ready_verified"]
            assert not report["interpretation"]["sustained_external_usage_verified"]
            assert not report["interpretation"]["workflow_owner_trial_reports_verified"]
            assert report["interpretation"]["community_provenance_is_recorded"]
            assert report["community"]["external_authored_issue_count"] == 0
            assert report["community"]["trial_report_thread"]["external_comment_count"] == 0
            assert report["distribution"]["github_release"]["latest_published_version"] == "0.1.11"
            assert not report["distribution"]["container"]["workspace_version_tag_present"]
            assert report["distribution"]["bioconda"]["pull_request"]["version_in_title"] == "0.1.10"
            assert not report["interpretation"]["distribution_channels_match_workspace_verified"]


if __name__ == "__main__":
    unittest.main()
