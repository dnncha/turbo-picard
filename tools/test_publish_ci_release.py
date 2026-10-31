from __future__ import annotations

import unittest
from unittest.mock import patch

import publish_ci_release as release  # type: ignore[import-not-found]
import pytest


class ReleaseGateTests(unittest.TestCase):
    def valid_run(self) -> dict[str, object]:
        return {
            "name": "CI",
            "path": ".github/workflows/ci.yml",
            "event": "push",
            "head_branch": "main",
            "status": "completed",
            "conclusion": "success",
            "head_repository": {"full_name": release.REPOSITORY},
            "head_sha": "a" * 40,
        }

    def test_accepts_successful_main_push(self) -> None:
        assert release.validate_run(self.valid_run(), release.REPOSITORY) == "a" * 40

    def test_rejects_untrusted_or_untested_sources(self) -> None:
        for key, value in (
            ("event", "pull_request"),
            ("head_branch", "feature"),
            ("conclusion", "failure"),
            ("status", "in_progress"),
            ("path", ".github/workflows/other.yml"),
            ("name", "Other"),
            ("head_sha", "main"),
            ("head_repository", {"full_name": "fork/turbo-picard"}),
        ):
            with self.subTest(key=key):
                run = self.valid_run()
                run[key] = value
                with pytest.raises(ValueError, match=r"Release|CI source SHA"):
                    release.validate_run(run, release.REPOSITORY)

    def test_rejects_other_repository(self) -> None:
        with pytest.raises(ValueError, match="Release requires completed"):
            release.validate_run(self.valid_run(), "other/project")

    def test_rejects_non_ci_invocation_before_api(self) -> None:
        with patch.dict(release.os.environ, {"GITHUB_EVENT_NAME": "pull_request"}), patch.object(release, "api") as api:
            with pytest.raises(ValueError, match="guarded workflow_run"):
                release.main()
            api.assert_not_called()


if __name__ == "__main__":
    unittest.main()
