#!/usr/bin/env python3
"""Collect read-only, source-backed public adoption signals.

This audit measures distribution and public-interest signals for Turbo Picard.
It deliberately does not infer active users, customer demand, or production
readiness from download counts or repository metadata.
"""

from __future__ import annotations

import argparse
from datetime import date, datetime, timedelta, timezone
import hashlib
import json
from pathlib import Path
import re
import sys
import subprocess
import time
from typing import Any, Callable
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
PACKAGE_NAME = "turbo-picard"
PYPI_JSON_URL = "https://pypi.org/pypi/turbo-picard/json"
PYPISTATS_OVERALL_URL = (
    "https://pypistats.org/api/packages/turbo-picard/overall?mirrors=false"
)
GITHUB_REPOSITORY_URL = "https://api.github.com/repos/dnncha/turbo-picard"
GITHUB_ISSUES_URL = (
    "https://api.github.com/repos/dnncha/turbo-picard/issues"
    "?state=open&per_page=100"
)
TRIAL_REPORT_ISSUE_NUMBER = 4
TRIAL_REPORT_ISSUE_URL = "https://github.com/dnncha/turbo-picard/issues/4"
USER_AGENT = "turbo-picard-public-adoption-audit/1"


def normalize_text(text: str) -> str:
    return text.replace("\r\n", "\n").replace("\r", "\n").rstrip()


def workspace_version(root: Path = ROOT) -> str:
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(
        r'(?ms)^\[workspace\.package\]\s+.*?^version\s*=\s*"([^"]+)"',
        text,
    )
    if not match:
        raise ValueError("Cargo.toml missing [workspace.package] version")
    return match.group(1)


GitRunner = Callable[[list[str], Path], tuple[int, list[str]]]


def run_git(args: list[str], root: Path = ROOT) -> tuple[int, list[str]]:
    """Run a read-only git query and return its status plus non-empty lines."""

    completed = subprocess.run(
        ["git", *args],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    lines = [
        line.strip()
        for stream in (completed.stdout, completed.stderr)
        for line in stream.splitlines()
        if line.strip()
    ]
    return completed.returncode, lines


def _first_line(status: int, lines: list[str]) -> str | None:
    return lines[0] if status == 0 and lines else None


def _remote_tag_commit(lines: list[str], tag: str) -> str | None:
    """Resolve an annotated or lightweight remote tag without exposing URLs."""

    for line in lines:
        parts = line.split()
        if len(parts) == 2 and parts[1] == f"refs/tags/{tag}^{{}}":
            return parts[0]
    for line in lines:
        parts = line.split()
        if len(parts) == 2 and parts[1] == f"refs/tags/{tag}":
            return parts[0]
    return None


def collect_release_state(
    root: Path = ROOT,
    *,
    git_runner: GitRunner = run_git,
) -> dict[str, Any]:
    """Capture local source/tag state without recording filesystem paths."""

    version = workspace_version(root)
    tag = f"v{version}"
    status, status_lines = git_runner(["status", "--porcelain"], root)
    head_status, head_lines = git_runner(["rev-parse", "HEAD"], root)
    branch_status, branch_lines = git_runner(
        ["rev-parse", "--abbrev-ref", "HEAD"], root
    )
    local_tag_status, local_tag_lines = git_runner(["rev-list", "-n", "1", tag], root)
    remote_tag_status, remote_tag_lines = git_runner(
        ["ls-remote", "--tags", "origin", f"{tag}*"], root
    )

    head = _first_line(head_status, head_lines)
    local_tag_commit = _first_line(local_tag_status, local_tag_lines)
    origin_tag_commit = (
        _remote_tag_commit(remote_tag_lines, tag) if remote_tag_status == 0 else None
    )
    worktree_clean = status == 0 and not status_lines
    tag_matches_head = head is not None and local_tag_commit == head
    origin_tag_matches_local = (
        local_tag_commit is not None
        and origin_tag_commit is not None
        and origin_tag_commit == local_tag_commit
    )
    git_queries_succeeded = all(
        query_status == 0
        for query_status in (
            status,
            head_status,
            branch_status,
            local_tag_status,
            remote_tag_status,
        )
    )
    blockers: list[str] = []
    if status != 0:
        blockers.append("git worktree status could not be read")
    elif not worktree_clean:
        blockers.append("worktree has uncommitted changes")
    if head is None:
        blockers.append("current HEAD could not be resolved")
    if local_tag_commit is None:
        blockers.append(f"local release tag {tag} is missing")
    elif not tag_matches_head:
        blockers.append(f"local release tag {tag} does not point at current HEAD")
    if remote_tag_status != 0:
        blockers.append(f"origin release tag {tag} could not be checked")
    elif origin_tag_commit is None:
        blockers.append(f"origin release tag {tag} is missing")
    elif not origin_tag_matches_local:
        blockers.append(f"origin release tag {tag} differs from the local tag")

    return {
        "workspace_version": version,
        "branch": _first_line(branch_status, branch_lines),
        "head_commit": head,
        "worktree_clean": worktree_clean,
        "changed_path_count": len(status_lines) if status == 0 else None,
        "release_tag": tag,
        "local_tag_commit": local_tag_commit,
        "origin_tag_commit": origin_tag_commit,
        "tag_matches_head": tag_matches_head,
        "origin_tag_matches_local": origin_tag_matches_local,
        "git_queries_succeeded": git_queries_succeeded,
        "release_source_ready": not blockers,
        "blockers": blockers,
    }


def fetch_json(
    url: str,
    *,
    timeout: float = 15.0,
    retries: int = 3,
    retry_delay: float = 1.0,
) -> object:
    """Fetch a public JSON endpoint with bounded retries and no credentials."""

    if retries < 1:
        raise ValueError("retries must be at least 1")
    if timeout <= 0:
        raise ValueError("timeout must be greater than 0")
    if retry_delay < 0:
        raise ValueError("retry_delay must not be negative")

    last_error: Exception | None = None
    for attempt in range(retries):
        request = urllib.request.Request(
            url,
            headers={
                "Accept": "application/json",
                "User-Agent": USER_AGENT,
            },
        )
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return json.load(response)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            last_error = error
            if attempt + 1 < retries:
                time.sleep(retry_delay)
    raise RuntimeError(f"could not fetch JSON from {url}: {last_error}") from last_error


def _require_mapping(payload: object, label: str) -> dict[str, Any]:
    if not isinstance(payload, dict):
        raise ValueError(f"{label} response must be a JSON object")
    return payload


def _require_nonempty_string(value: object, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{label} must be a non-empty string")
    return value


def parse_pypi_metadata(
    payload: object,
    *,
    expected_version: str | None = None,
    readme: str | None = None,
) -> dict[str, Any]:
    data = _require_mapping(payload, "PyPI")
    info = _require_mapping(data.get("info"), "PyPI info")
    name = _require_nonempty_string(info.get("name"), "PyPI package name")
    version = _require_nonempty_string(info.get("version"), "PyPI version")
    description = info.get("description")
    if not isinstance(description, str):
        raise ValueError("PyPI description must be a string")

    files = data.get("urls", [])
    if not isinstance(files, list):
        raise ValueError("PyPI urls must be a list")
    upload_times = [
        item.get("upload_time_iso_8601")
        for item in files
        if isinstance(item, dict) and isinstance(item.get("upload_time_iso_8601"), str)
    ]

    result: dict[str, Any] = {
        "name": name,
        "live_version": version,
        "requires_python": info.get("requires_python"),
        "description_content_type": info.get("description_content_type"),
        "description_sha256": hashlib.sha256(description.encode("utf-8")).hexdigest(),
        "latest_file_upload_time": max(upload_times) if upload_times else None,
        "project_urls": info.get("project_urls") or {},
    }
    if expected_version is not None:
        result["workspace_version"] = expected_version
        result["version_matches_workspace"] = version == expected_version
    if readme is not None:
        result["long_description_matches_workspace"] = normalize_text(description) == normalize_text(readme)
    return result


def _parse_day(value: object, label: str) -> date:
    if not isinstance(value, str):
        raise ValueError(f"{label} must be an ISO date")
    try:
        return date.fromisoformat(value)
    except ValueError as error:
        raise ValueError(f"{label} must be an ISO date") from error


def parse_downloads(payload: object) -> dict[str, Any]:
    data = _require_mapping(payload, "PyPIStats")
    rows = data.get("data")
    if not isinstance(rows, list):
        raise ValueError("PyPIStats data must be a list")

    parsed: list[tuple[date, int]] = []
    for index, row in enumerate(rows):
        if not isinstance(row, dict):
            raise ValueError(f"PyPIStats row {index} must be an object")
        if row.get("category") != "without_mirrors":
            continue
        day = _parse_day(row.get("date"), f"PyPIStats row {index} date")
        downloads = row.get("downloads")
        if not isinstance(downloads, int) or isinstance(downloads, bool) or downloads < 0:
            raise ValueError(f"PyPIStats row {index} downloads must be a non-negative integer")
        parsed.append((day, downloads))

    if not parsed:
        raise ValueError("PyPIStats has no without_mirrors rows")
    parsed.sort()
    latest_day = parsed[-1][0]

    def total_for_days(days: int) -> int:
        cutoff = latest_day - timedelta(days=days - 1)
        return sum(downloads for day, downloads in parsed if cutoff <= day <= latest_day)

    return {
        "package": data.get("package"),
        "category": "without_mirrors",
        "days_returned": len(parsed),
        "period_start": parsed[0][0].isoformat(),
        "period_end": latest_day.isoformat(),
        "downloads_total": sum(downloads for _day, downloads in parsed),
        "latest_day_downloads": parsed[-1][1],
        "downloads_last_7_days": total_for_days(7),
        "downloads_last_30_days": total_for_days(30),
    }


def parse_repository(payload: object) -> dict[str, Any]:
    data = _require_mapping(payload, "GitHub repository")
    return {
        "full_name": _require_nonempty_string(data.get("full_name"), "GitHub full_name"),
        "html_url": _require_nonempty_string(data.get("html_url"), "GitHub html_url"),
        "default_branch": data.get("default_branch"),
        "stargazers_count": data.get("stargazers_count"),
        "forks_count": data.get("forks_count"),
        "watchers_count": data.get("watchers_count"),
        "subscribers_count": data.get("subscribers_count"),
        "open_issues_count_including_pull_requests": data.get("open_issues_count"),
        "updated_at": data.get("updated_at"),
        "pushed_at": data.get("pushed_at"),
    }


def parse_open_issues(payload: object) -> dict[str, Any]:
    if not isinstance(payload, list):
        raise ValueError("GitHub issues response must be a list")
    issues = [item for item in payload if isinstance(item, dict) and "pull_request" not in item]
    numbers = sorted(
        item["number"]
        for item in issues
        if isinstance(item.get("number"), int) and not isinstance(item.get("number"), bool)
    )
    urls = sorted(
        item["html_url"]
        for item in issues
        if isinstance(item.get("html_url"), str) and item["html_url"]
    )
    trial_issue = next(
        (
            item
            for item in issues
            if item.get("number") == TRIAL_REPORT_ISSUE_NUMBER
        ),
        None,
    )
    if trial_issue is None:
        trial_report_thread = {
            "issue_number": TRIAL_REPORT_ISSUE_NUMBER,
            "state": "not_in_open_response",
            "comment_count": None,
            "url": TRIAL_REPORT_ISSUE_URL,
        }
    else:
        comments = trial_issue.get("comments")
        if comments is not None and (
            not isinstance(comments, int)
            or isinstance(comments, bool)
            or comments < 0
        ):
            raise ValueError("trial report issue comments must be a non-negative integer")
        issue_url = trial_issue.get("html_url")
        if not isinstance(issue_url, str) or not issue_url.strip():
            issue_url = TRIAL_REPORT_ISSUE_URL
        trial_report_thread = {
            "issue_number": TRIAL_REPORT_ISSUE_NUMBER,
            "state": trial_issue.get("state") or "open",
            "comment_count": comments,
            "url": issue_url,
        }
    return {
        "open_issue_count_excluding_pull_requests": len(issues),
        "open_issue_numbers": numbers,
        "open_issue_urls": urls,
        "pull_requests_excluded": len(payload) - len(issues),
        "response_page_size": len(payload),
        "possibly_truncated": len(payload) >= 100,
        "trial_report_thread": trial_report_thread,
    }


def build_report(
    *,
    observed_at_utc: str,
    pypi: dict[str, Any],
    downloads: dict[str, Any],
    repository: dict[str, Any],
    issues: dict[str, Any],
    release_state: dict[str, Any],
    sources: dict[str, str] | None = None,
) -> dict[str, Any]:
    return {
        "schema_version": 2,
        "observed_at_utc": observed_at_utc,
        "release_state": release_state,
        "package": pypi,
        "downloads": downloads,
        "repository": repository,
        "community": issues,
        "sources": sources
        or {
            "pypi": PYPI_JSON_URL,
            "pypistats_overall_without_mirrors": PYPISTATS_OVERALL_URL,
            "github_repository": GITHUB_REPOSITORY_URL,
            "github_open_issues": GITHUB_ISSUES_URL,
        },
        "interpretation": {
            "download_counts_are_distribution_signals": True,
            "repository_counts_are_public_interest_signals": True,
            "sustained_external_usage_verified": False,
            "customer_demand_verified": False,
            "production_readiness_verified": False,
            "workflow_owner_trial_reports_verified": False,
            "trial_report_comments_are_community_signals": True,
            "release_source_ready_verified": release_state["release_source_ready"],
            "public_package_matches_source_verified": bool(
                pypi.get("version_matches_workspace")
                and pypi.get("long_description_matches_workspace")
            ),
        },
    }


def utc_timestamp() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


Fetcher = Callable[..., object]


def collect_report(
    root: Path = ROOT,
    *,
    fetcher: Fetcher = fetch_json,
    timeout: float = 15.0,
    retries: int = 3,
    retry_delay: float = 1.0,
    observed_at_utc: str | None = None,
) -> dict[str, Any]:
    expected_version = workspace_version(root)
    readme = (root / "README.md").read_text(encoding="utf-8")
    pypi = parse_pypi_metadata(
        fetcher(PYPI_JSON_URL, timeout=timeout, retries=retries, retry_delay=retry_delay),
        expected_version=expected_version,
        readme=readme,
    )
    downloads = parse_downloads(
        fetcher(
            PYPISTATS_OVERALL_URL,
            timeout=timeout,
            retries=retries,
            retry_delay=retry_delay,
        )
    )
    repository = parse_repository(
        fetcher(
            GITHUB_REPOSITORY_URL,
            timeout=timeout,
            retries=retries,
            retry_delay=retry_delay,
        )
    )
    issues = parse_open_issues(
        fetcher(
            GITHUB_ISSUES_URL,
            timeout=timeout,
            retries=retries,
            retry_delay=retry_delay,
        )
    )
    release_state = collect_release_state(root)
    return build_report(
        observed_at_utc=observed_at_utc or utc_timestamp(),
        pypi=pypi,
        downloads=downloads,
        repository=repository,
        issues=issues,
        release_state=release_state,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="write JSON to this path instead of stdout")
    parser.add_argument("--pretty", action="store_true", help="indent JSON output")
    parser.add_argument("--timeout", type=float, default=15.0)
    parser.add_argument("--retries", type=int, default=3)
    parser.add_argument("--retry-delay", type=float, default=1.0)
    args = parser.parse_args(argv)

    try:
        report = collect_report(
            timeout=args.timeout,
            retries=args.retries,
            retry_delay=args.retry_delay,
        )
    except (OSError, RuntimeError, ValueError) as error:
        print(f"public adoption audit failed: {error}", file=sys.stderr)
        return 1

    text = json.dumps(report, indent=2 if args.pretty else None, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
