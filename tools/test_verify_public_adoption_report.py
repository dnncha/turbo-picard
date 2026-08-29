from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest


MODULE_PATH = Path(__file__).with_name("verify_public_adoption_report.py")
SPEC = importlib.util.spec_from_file_location("verify_public_adoption_report", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
verify_public_adoption_report = importlib.util.module_from_spec(SPEC)
sys.modules["verify_public_adoption_report"] = verify_public_adoption_report
SPEC.loader.exec_module(verify_public_adoption_report)


def valid_report() -> dict[str, object]:
    return {
        "schema_version": 4,
        "observed_at_utc": "2026-08-14T00:00:00Z",
        "release_state": {
            "workspace_version": "0.1.12",
            "head_commit": "a" * 40,
            "worktree_clean": True,
            "release_tag": "v0.1.12",
            "local_tag_commit": "a" * 40,
            "origin_tag_commit": "a" * 40,
            "tag_matches_head": True,
            "origin_tag_matches_local": True,
            "git_queries_succeeded": True,
            "release_source_ready": True,
            "blockers": [],
        },
        "package": {
            "live_version": "0.1.12",
            "version_matches_workspace": True,
            "long_description_matches_workspace": True,
        },
        "distribution": {
            "workspace_version": "0.1.12",
            "github_release": {
                "status": "available",
                "published_release_count": 1,
                "response_page_size": 1,
                "possibly_truncated": False,
                "latest_published_tag": "v0.1.12",
                "latest_published_version": "0.1.12",
                "workspace_release_published": True,
                "workspace_release_url": "https://github.com/dnncha/turbo-picard/releases/tag/v0.1.12",
            },
            "container": {
                "status": "available",
                "image": "dnncha/turbo-picard",
                "version_tag_count": 1,
                "version_tags": ["0.1.12"],
                "latest_version": "0.1.12",
                "workspace_version_tag_present": True,
                "response_page_size": 1,
                "possibly_truncated": False,
            },
            "bioconda": {
                "main_package": {
                    "status": "available",
                    "package": "turbo-picard",
                    "package_url": "https://api.anaconda.org/package/bioconda/turbo-picard",
                    "latest_version": "0.1.12",
                    "workspace_version_available": True,
                    "version_count": 1,
                },
                "shim_package": {
                    "status": "available",
                    "package": "turbo-picard-picard-shim",
                    "package_url": "https://api.anaconda.org/package/bioconda/turbo-picard-picard-shim",
                    "latest_version": "0.1.12",
                    "workspace_version_available": True,
                    "version_count": 1,
                },
                "pull_request": {
                    "status": "available",
                    "number": 65922,
                    "state": "merged",
                    "title": "Add turbo-picard 0.1.12",
                    "version_in_title": "0.1.12",
                    "url": "https://github.com/bioconda/bioconda-recipes/pull/65922",
                    "updated_at": "2026-08-14T00:00:00Z",
                },
            },
        },
        "community": {
            "open_issue_count_excluding_pull_requests": 1,
            "open_issue_numbers": [4],
            "open_issue_urls": ["https://github.com/dnncha/turbo-picard/issues/4"],
            "maintainer_authored_issue_count": 1,
            "maintainer_authored_issue_numbers": [4],
            "external_authored_issue_count": 0,
            "external_authored_issue_numbers": [],
            "unknown_issue_author_count": 0,
            "unknown_issue_author_numbers": [],
            "pull_requests_excluded": 0,
            "response_page_size": 1,
            "possibly_truncated": False,
            "trial_report_thread": {
                "issue_number": 4,
                "state": "open",
                "comment_count": 1,
                "url": "https://github.com/dnncha/turbo-picard/issues/4",
                "author_provenance": "maintainer",
                "author_is_maintainer": True,
                "comment_page_size": 1,
                "comments_possibly_truncated": False,
                "maintainer_comment_count": 1,
                "external_comment_count": 0,
                "unknown_comment_count": 0,
            },
        },
        "interpretation": {
            "download_counts_are_distribution_signals": True,
            "repository_counts_are_public_interest_signals": True,
            "distribution_channels_are_read_only_signals": True,
            "sustained_external_usage_verified": False,
            "customer_demand_verified": False,
            "production_readiness_verified": False,
            "workflow_owner_trial_reports_verified": False,
            "trial_report_comments_are_community_signals": True,
            "community_provenance_is_recorded": True,
            "release_source_ready_verified": True,
            "public_package_matches_source_verified": True,
            "distribution_channels_match_workspace_verified": True,
        },
    }


class VerifyPublicAdoptionReportTests(unittest.TestCase):
    def test_accepts_valid_report(self) -> None:
        self.assertEqual([], verify_public_adoption_report.collect_errors(valid_report()))

    def test_rejects_schema_downgrade(self) -> None:
        payload = valid_report()
        payload["schema_version"] = 1
        self.assertIn(
            "report schema_version must be 4",
            verify_public_adoption_report.collect_errors(payload),
        )

    def test_rejects_claiming_production_readiness(self) -> None:
        payload = valid_report()
        payload["interpretation"]["production_readiness_verified"] = True  # type: ignore[index]
        self.assertIn(
            "interpretation production_readiness_verified must remain false",
            verify_public_adoption_report.collect_errors(payload),
        )

    def test_rejects_claiming_workflow_owner_trials(self) -> None:
        payload = valid_report()
        payload["interpretation"]["workflow_owner_trial_reports_verified"] = True  # type: ignore[index]
        self.assertIn(
            "interpretation workflow_owner_trial_reports_verified must remain false",
            verify_public_adoption_report.collect_errors(payload),
        )

    def test_rejects_malformed_release_commit(self) -> None:
        payload = valid_report()
        payload["release_state"]["head_commit"] = "not-a-commit"  # type: ignore[index]
        self.assertIn(
            "release_state head_commit must be a full commit hash or null",
            verify_public_adoption_report.collect_errors(payload),
        )


if __name__ == "__main__":
    unittest.main()
