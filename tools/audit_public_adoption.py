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
from urllib.error import HTTPError
import urllib.request


ROOT = Path(__file__).resolve().parents[1]
PACKAGE_NAME = "turbo-picard"
PYPI_JSON_URL = "https://pypi.org/pypi/turbo-picard/json"
PYPISTATS_OVERALL_URL = (
    "https://pypistats.org/api/packages/turbo-picard/overall?mirrors=false"
)
GITHUB_REPOSITORY_URL = "https://api.github.com/repos/dnncha/turbo-picard"
GITHUB_RELEASES_URL = (
    "https://api.github.com/repos/dnncha/turbo-picard/releases"
    "?per_page=100"
)
GITHUB_ISSUES_URL = (
    "https://api.github.com/repos/dnncha/turbo-picard/issues"
    "?state=open&per_page=100"
)
GITHUB_TRIAL_COMMENTS_URL = (
    "https://api.github.com/repos/dnncha/turbo-picard/issues/4/comments"
    "?per_page=100"
)
GITHUB_OWNER_LOGIN = "dnncha"
GHCR_TOKEN_URL = (
    "https://ghcr.io/token?service=ghcr.io"
    "&scope=repository%3Adnncha%2Fturbo-picard%3Apull"
)
GHCR_TAGS_URL = "https://ghcr.io/v2/dnncha/turbo-picard/tags/list"
ANACONDA_TURBO_PICARD_URL = "https://api.anaconda.org/package/bioconda/turbo-picard"
ANACONDA_SHIM_URL = (
    "https://api.anaconda.org/package/bioconda/turbo-picard-picard-shim"
)
BIOCONDA_PR_URL = "https://api.github.com/repos/bioconda/bioconda-recipes/pulls/65922"
BIOCONDA_PR_NUMBER = 65922
TRIAL_REPORT_ISSUE_NUMBER = 4
TRIAL_REPORT_ISSUE_URL = "https://github.com/dnncha/turbo-picard/issues/4"
USER_AGENT = "turbo-picard-public-adoption-audit/1"
SEMVER = re.compile(r"^\d+\.\d+\.\d+$")


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
    headers: dict[str, str] | None = None,
    allow_not_found: bool = False,
) -> object:
    """Fetch a public JSON endpoint with bounded retries."""

    if retries < 1:
        raise ValueError("retries must be at least 1")
    if timeout <= 0:
        raise ValueError("timeout must be greater than 0")
    if retry_delay < 0:
        raise ValueError("retry_delay must not be negative")

    last_error: Exception | None = None
    for attempt in range(retries):
        request_headers = {
            "Accept": "application/json",
            "User-Agent": USER_AGENT,
        }
        if headers:
            request_headers.update(headers)
        request = urllib.request.Request(
            url,
            headers=request_headers,
        )
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return json.load(response)
        except HTTPError as error:
            if allow_not_found and error.code == 404:
                return None
            last_error = error
            if attempt + 1 < retries:
                time.sleep(retry_delay)
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


def _semver_from_tag(tag: object) -> str | None:
    if not isinstance(tag, str):
        return None
    candidate = tag[1:] if tag.startswith("v") else tag
    return candidate if SEMVER.fullmatch(candidate) else None


def parse_github_releases(payload: object, *, expected_version: str) -> dict[str, Any]:
    if not isinstance(payload, list):
        raise ValueError("GitHub releases response must be a list")
    published: list[dict[str, Any]] = []
    for item in payload:
        if not isinstance(item, dict):
            continue
        if item.get("draft") is True or item.get("prerelease") is True:
            continue
        tag = item.get("tag_name")
        published_at = item.get("published_at")
        if not isinstance(tag, str) or not tag.strip() or not isinstance(published_at, str):
            continue
        published.append(item)
    published.sort(key=lambda item: item["published_at"], reverse=True)
    latest = published[0] if published else None
    workspace_release = next(
        (item for item in published if _semver_from_tag(item.get("tag_name")) == expected_version),
        None,
    )
    latest_tag = latest.get("tag_name") if latest else None
    latest_version = _semver_from_tag(latest_tag)
    workspace_url = workspace_release.get("html_url") if workspace_release else None
    if workspace_url is not None and (
        not isinstance(workspace_url, str) or not workspace_url.strip()
    ):
        workspace_url = None
    return {
        "status": "available",
        "published_release_count": len(published),
        "response_page_size": len(payload),
        "possibly_truncated": len(payload) >= 100,
        "latest_published_tag": latest_tag,
        "latest_published_version": latest_version,
        "workspace_release_published": workspace_release is not None,
        "workspace_release_url": workspace_url,
    }


def parse_ghcr_token(payload: object) -> str:
    data = _require_mapping(payload, "GHCR token")
    return _require_nonempty_string(data.get("token"), "GHCR token")


def parse_ghcr_tags(payload: object, *, expected_version: str) -> dict[str, Any]:
    data = _require_mapping(payload, "GHCR tags")
    name = _require_nonempty_string(data.get("name"), "GHCR image name")
    tags = data.get("tags")
    if not isinstance(tags, list) or not all(isinstance(tag, str) for tag in tags):
        raise ValueError("GHCR tags must be a list of strings")
    version_tags = sorted(
        {version for tag in tags if (version := _semver_from_tag(tag)) is not None},
        key=lambda version: tuple(int(part) for part in version.split(".")),
    )
    return {
        "status": "available",
        "image": name,
        "version_tag_count": len(version_tags),
        "version_tags": version_tags,
        "latest_version": version_tags[-1] if version_tags else None,
        "workspace_version_tag_present": expected_version in version_tags,
        "response_page_size": len(tags),
        "possibly_truncated": len(tags) >= 100,
    }


def unavailable_distribution_channel(reason: str) -> dict[str, Any]:
    return {
        "status": "unavailable",
        "reason": reason,
    }


def parse_anaconda_package(
    payload: object,
    *,
    package_name: str,
    package_url: str,
    expected_version: str,
) -> dict[str, Any]:
    if payload is None:
        return {
            "status": "not_found",
            "package": package_name,
            "package_url": package_url,
            "latest_version": None,
            "workspace_version_available": False,
            "version_count": 0,
        }
    data = _require_mapping(payload, f"Anaconda package {package_name}")
    versions = data.get("versions") or []
    if not isinstance(versions, list) or not all(isinstance(version, str) for version in versions):
        raise ValueError(f"Anaconda package {package_name} versions must be a list of strings")
    latest_version = data.get("latest_version")
    if latest_version is not None and not isinstance(latest_version, str):
        raise ValueError(f"Anaconda package {package_name} latest_version must be text or null")
    if not latest_version:
        semver_versions = [version for version in versions if SEMVER.fullmatch(version)]
        latest_version = max(
            semver_versions,
            key=lambda version: tuple(int(part) for part in version.split(".")),
            default=None,
        )
    return {
        "status": "available",
        "package": _require_nonempty_string(data.get("name", package_name), "Anaconda package name"),
        "package_url": package_url,
        "latest_version": latest_version,
        "workspace_version_available": expected_version in versions,
        "version_count": len(versions),
    }


def parse_bioconda_pull_request(payload: object) -> dict[str, Any]:
    if payload is None:
        return {
            "status": "not_found",
            "number": BIOCONDA_PR_NUMBER,
            "state": "not_found",
            "title": None,
            "version_in_title": None,
            "url": BIOCONDA_PR_URL,
        }
    data = _require_mapping(payload, "Bioconda pull request")
    title = data.get("title")
    if title is not None and not isinstance(title, str):
        raise ValueError("Bioconda pull request title must be text or null")
    version_match = re.search(r"\b(\d+\.\d+\.\d+)\b", title or "")
    url = data.get("html_url")
    if not isinstance(url, str) or not url.strip():
        url = BIOCONDA_PR_URL
    state = data.get("state")
    if not isinstance(state, str) or not state.strip():
        state = "unknown"
    return {
        "status": "available",
        "number": data.get("number", BIOCONDA_PR_NUMBER),
        "state": state,
        "title": title,
        "version_in_title": version_match.group(1) if version_match else None,
        "url": url,
        "updated_at": data.get("updated_at"),
    }


def collect_distribution_state(
    *,
    expected_version: str,
    fetcher: Fetcher,
    timeout: float,
    retries: int,
    retry_delay: float,
) -> dict[str, Any]:
    github_releases = parse_github_releases(
        fetcher(
            GITHUB_RELEASES_URL,
            timeout=timeout,
            retries=retries,
            retry_delay=retry_delay,
        ),
        expected_version=expected_version,
    )
    bioconda = {
        "main_package": parse_anaconda_package(
            fetcher(
                ANACONDA_TURBO_PICARD_URL,
                timeout=timeout,
                retries=retries,
                retry_delay=retry_delay,
                allow_not_found=True,
            ),
            package_name="turbo-picard",
            package_url=ANACONDA_TURBO_PICARD_URL,
            expected_version=expected_version,
        ),
        "shim_package": parse_anaconda_package(
            fetcher(
                ANACONDA_SHIM_URL,
                timeout=timeout,
                retries=retries,
                retry_delay=retry_delay,
                allow_not_found=True,
            ),
            package_name="turbo-picard-picard-shim",
            package_url=ANACONDA_SHIM_URL,
            expected_version=expected_version,
        ),
        "pull_request": parse_bioconda_pull_request(
            fetcher(
                BIOCONDA_PR_URL,
                timeout=timeout,
                retries=retries,
                retry_delay=retry_delay,
                allow_not_found=True,
            )
        ),
    }
    try:
        token = parse_ghcr_token(
            fetcher(
                GHCR_TOKEN_URL,
                timeout=timeout,
                retries=retries,
                retry_delay=retry_delay,
            )
        )
        container = parse_ghcr_tags(
            fetcher(
                GHCR_TAGS_URL,
                timeout=timeout,
                retries=retries,
                retry_delay=retry_delay,
                headers={"Authorization": f"Bearer {token}"},
            ),
            expected_version=expected_version,
        )
    except (RuntimeError, ValueError):
        container = unavailable_distribution_channel("registry_query_failed")
    return {
        "workspace_version": expected_version,
        "github_release": github_releases,
        "container": container,
        "bioconda": bioconda,
    }


def parse_open_issues(
    payload: object,
    *,
    maintainer_login: str = GITHUB_OWNER_LOGIN,
) -> dict[str, Any]:
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
    maintainer_issue_numbers: list[int] = []
    external_issue_numbers: list[int] = []
    unknown_issue_author_numbers: list[int] = []
    for item in issues:
        number = item.get("number")
        if not isinstance(number, int) or isinstance(number, bool):
            continue
        user = item.get("user")
        login = user.get("login") if isinstance(user, dict) else None
        if login == maintainer_login:
            maintainer_issue_numbers.append(number)
        elif isinstance(login, str) and login.strip():
            external_issue_numbers.append(number)
        else:
            unknown_issue_author_numbers.append(number)
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
            "author_provenance": "not_in_open_response",
            "author_is_maintainer": None,
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
        user = trial_issue.get("user")
        login = user.get("login") if isinstance(user, dict) else None
        if login == maintainer_login:
            author_provenance = "maintainer"
        elif isinstance(login, str) and login.strip():
            author_provenance = "external"
        else:
            author_provenance = "unknown"
        trial_report_thread = {
            "issue_number": TRIAL_REPORT_ISSUE_NUMBER,
            "state": trial_issue.get("state") or "open",
            "comment_count": comments,
            "url": issue_url,
            "author_provenance": author_provenance,
            "author_is_maintainer": author_provenance == "maintainer",
        }
    return {
        "open_issue_count_excluding_pull_requests": len(issues),
        "open_issue_numbers": numbers,
        "open_issue_urls": urls,
        "maintainer_authored_issue_count": len(maintainer_issue_numbers),
        "maintainer_authored_issue_numbers": sorted(maintainer_issue_numbers),
        "external_authored_issue_count": len(external_issue_numbers),
        "external_authored_issue_numbers": sorted(external_issue_numbers),
        "unknown_issue_author_count": len(unknown_issue_author_numbers),
        "unknown_issue_author_numbers": sorted(unknown_issue_author_numbers),
        "pull_requests_excluded": len(payload) - len(issues),
        "response_page_size": len(payload),
        "possibly_truncated": len(payload) >= 100,
        "trial_report_thread": trial_report_thread,
    }


def parse_trial_comments(
    payload: object,
    *,
    maintainer_login: str = GITHUB_OWNER_LOGIN,
) -> dict[str, Any]:
    """Count comment-author provenance without retaining public usernames."""

    if not isinstance(payload, list):
        raise ValueError("GitHub trial comments response must be a list")
    maintainer_count = 0
    external_count = 0
    unknown_count = 0
    for item in payload:
        if not isinstance(item, dict):
            unknown_count += 1
            continue
        user = item.get("user")
        login = user.get("login") if isinstance(user, dict) else None
        if login == maintainer_login:
            maintainer_count += 1
        elif isinstance(login, str) and login.strip():
            external_count += 1
        else:
            unknown_count += 1
    return {
        "comment_page_size": len(payload),
        "comments_possibly_truncated": len(payload) >= 100,
        "maintainer_comment_count": maintainer_count,
        "external_comment_count": external_count,
        "unknown_comment_count": unknown_count,
    }


def build_report(
    *,
    observed_at_utc: str,
    pypi: dict[str, Any],
    downloads: dict[str, Any],
    repository: dict[str, Any],
    issues: dict[str, Any],
    distribution: dict[str, Any],
    release_state: dict[str, Any],
    sources: dict[str, str] | None = None,
) -> dict[str, Any]:
    return {
        "schema_version": 4,
        "observed_at_utc": observed_at_utc,
        "release_state": release_state,
        "package": pypi,
        "downloads": downloads,
        "repository": repository,
        "community": issues,
        "distribution": distribution,
        "sources": sources
        or {
            "pypi": PYPI_JSON_URL,
            "pypistats_overall_without_mirrors": PYPISTATS_OVERALL_URL,
            "github_repository": GITHUB_REPOSITORY_URL,
            "github_releases": GITHUB_RELEASES_URL,
            "github_open_issues": GITHUB_ISSUES_URL,
            "github_trial_report_comments": GITHUB_TRIAL_COMMENTS_URL,
            "ghcr_token": GHCR_TOKEN_URL,
            "ghcr_tags": GHCR_TAGS_URL,
            "anaconda_turbo_picard": ANACONDA_TURBO_PICARD_URL,
            "anaconda_picard_shim": ANACONDA_SHIM_URL,
            "bioconda_pull_request": BIOCONDA_PR_URL,
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
            "release_source_ready_verified": release_state["release_source_ready"],
            "public_package_matches_source_verified": bool(
                pypi.get("version_matches_workspace")
                and pypi.get("long_description_matches_workspace")
            ),
            "distribution_channels_match_workspace_verified": bool(
                pypi.get("version_matches_workspace")
                and pypi.get("long_description_matches_workspace")
                and distribution["github_release"].get("workspace_release_published")
                and distribution["container"].get("workspace_version_tag_present")
                and distribution["bioconda"]["main_package"].get(
                    "workspace_version_available"
                )
                and distribution["bioconda"]["shim_package"].get(
                    "workspace_version_available"
                )
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
    trial_comments = parse_trial_comments(
        fetcher(
            GITHUB_TRIAL_COMMENTS_URL,
            timeout=timeout,
            retries=retries,
            retry_delay=retry_delay,
        )
    )
    issues["trial_report_thread"].update(trial_comments)
    distribution = collect_distribution_state(
        expected_version=expected_version,
        fetcher=fetcher,
        timeout=timeout,
        retries=retries,
        retry_delay=retry_delay,
    )
    release_state = collect_release_state(root)
    return build_report(
        observed_at_utc=observed_at_utc or utc_timestamp(),
        pypi=pypi,
        downloads=downloads,
        repository=repository,
        issues=issues,
        distribution=distribution,
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
