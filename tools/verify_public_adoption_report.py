#!/usr/bin/env python3
"""Validate the shape and safety boundaries of a public adoption report."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys
from typing import Any


SCHEMA_VERSION = 4
HEX40 = re.compile(r"^[0-9a-f]{40}$")
SEMVER = re.compile(r"^\d+\.\d+\.\d+$")


def _mapping(payload: object, label: str, errors: list[str]) -> dict[str, Any]:
    if not isinstance(payload, dict):
        errors.append(f"{label} must be an object")
        return {}
    return payload


def collect_errors(payload: object) -> list[str]:
    errors: list[str] = []
    report = _mapping(payload, "report", errors)
    if report.get("schema_version") != SCHEMA_VERSION:
        errors.append(f"report schema_version must be {SCHEMA_VERSION}")
    if not isinstance(report.get("observed_at_utc"), str) or not report["observed_at_utc"].strip():
        errors.append("report observed_at_utc must be a non-empty string")

    release = _mapping(report.get("release_state"), "release_state", errors)
    version = release.get("workspace_version")
    if not isinstance(version, str) or not SEMVER.fullmatch(version):
        errors.append("release_state workspace_version must be semantic version text")
    for key in ("worktree_clean", "tag_matches_head", "origin_tag_matches_local", "release_source_ready"):
        if not isinstance(release.get(key), bool):
            errors.append(f"release_state {key} must be boolean")
    for key in ("head_commit", "local_tag_commit", "origin_tag_commit"):
        value = release.get(key)
        if value is not None and (not isinstance(value, str) or not HEX40.fullmatch(value)):
            errors.append(f"release_state {key} must be a full commit hash or null")
    blockers = release.get("blockers")
    if not isinstance(blockers, list) or not all(isinstance(item, str) for item in blockers):
        errors.append("release_state blockers must be a list of strings")

    package = _mapping(report.get("package"), "package", errors)
    if not isinstance(package.get("live_version"), str) or not SEMVER.fullmatch(package["live_version"]):
        errors.append("package live_version must be semantic version text")
    for key in ("version_matches_workspace", "long_description_matches_workspace"):
        if not isinstance(package.get(key), bool):
            errors.append(f"package {key} must be boolean")

    distribution = _mapping(report.get("distribution"), "distribution", errors)
    distribution_version = distribution.get("workspace_version")
    if not isinstance(distribution_version, str) or not SEMVER.fullmatch(distribution_version):
        errors.append("distribution workspace_version must be semantic version text")
    elif distribution_version != version:
        errors.append("distribution workspace_version must match release_state workspace_version")
    github_release = _mapping(distribution.get("github_release"), "distribution github_release", errors)
    if github_release.get("status") not in {"available", "not_found", "unavailable"}:
        errors.append("distribution github_release status is invalid")
    for key in ("published_release_count", "response_page_size"):
        value = github_release.get(key)
        if not isinstance(value, int) or isinstance(value, bool) or value < 0:
            errors.append(f"distribution github_release {key} must be a non-negative integer")
    if not isinstance(github_release.get("possibly_truncated"), bool):
        errors.append("distribution github_release possibly_truncated must be boolean")
    for key in ("latest_published_tag", "latest_published_version", "workspace_release_url"):
        value = github_release.get(key)
        if value is not None and (not isinstance(value, str) or not value.strip()):
            errors.append(f"distribution github_release {key} must be non-empty text or null")
    if not isinstance(github_release.get("workspace_release_published"), bool):
        errors.append("distribution github_release workspace_release_published must be boolean")

    container = _mapping(distribution.get("container"), "distribution container", errors)
    container_status = container.get("status")
    if container_status not in {"available", "not_found", "unavailable"}:
        errors.append("distribution container status is invalid")
    if container_status == "available":
        if not isinstance(container.get("image"), str) or not container["image"].strip():
            errors.append("distribution container image must be non-empty text")
        for key in ("version_tag_count", "response_page_size"):
            value = container.get(key)
            if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                errors.append(f"distribution container {key} must be a non-negative integer")
        version_tags = container.get("version_tags")
        if not isinstance(version_tags, list) or not all(
            isinstance(item, str) and SEMVER.fullmatch(item) for item in version_tags
        ):
            errors.append("distribution container version_tags must be a list of semantic versions")
        latest_version = container.get("latest_version")
        if latest_version is not None and (
            not isinstance(latest_version, str) or not SEMVER.fullmatch(latest_version)
        ):
            errors.append("distribution container latest_version must be semantic version text or null")
        if not isinstance(container.get("workspace_version_tag_present"), bool):
            errors.append("distribution container workspace_version_tag_present must be boolean")
        if not isinstance(container.get("possibly_truncated"), bool):
            errors.append("distribution container possibly_truncated must be boolean")
        elif container.get("version_tag_count") != len(container.get("version_tags", [])):
            errors.append("distribution container version_tag_count must match version_tags")
    elif not isinstance(container.get("reason"), str) or not container["reason"].strip():
        errors.append("distribution container reason must be non-empty text when unavailable")

    bioconda = _mapping(distribution.get("bioconda"), "distribution bioconda", errors)
    for key in ("main_package", "shim_package"):
        package_state = _mapping(bioconda.get(key), f"distribution bioconda {key}", errors)
        if package_state.get("status") not in {"available", "not_found", "unavailable"}:
            errors.append(f"distribution bioconda {key} status is invalid")
        if not isinstance(package_state.get("package"), str) or not package_state["package"].strip():
            errors.append(f"distribution bioconda {key} package must be non-empty text")
        if not isinstance(package_state.get("package_url"), str) or not package_state["package_url"].strip():
            errors.append(f"distribution bioconda {key} package_url must be non-empty text")
        latest_version = package_state.get("latest_version")
        if latest_version is not None and (
            not isinstance(latest_version, str) or not SEMVER.fullmatch(latest_version)
        ):
            errors.append(f"distribution bioconda {key} latest_version must be semantic version text or null")
        if not isinstance(package_state.get("workspace_version_available"), bool):
            errors.append(f"distribution bioconda {key} workspace_version_available must be boolean")
        version_count = package_state.get("version_count")
        if not isinstance(version_count, int) or isinstance(version_count, bool) or version_count < 0:
            errors.append(f"distribution bioconda {key} version_count must be a non-negative integer")
    pull_request = _mapping(bioconda.get("pull_request"), "distribution bioconda pull_request", errors)
    if pull_request.get("status") not in {"available", "not_found", "unavailable"}:
        errors.append("distribution bioconda pull_request status is invalid")
    if not isinstance(pull_request.get("number"), int) or isinstance(pull_request.get("number"), bool):
        errors.append("distribution bioconda pull_request number must be an integer")
    if not isinstance(pull_request.get("state"), str) or not pull_request["state"].strip():
        errors.append("distribution bioconda pull_request state must be non-empty text")
    for key in ("title", "version_in_title", "updated_at"):
        value = pull_request.get(key)
        if value is not None and (not isinstance(value, str) or not value.strip()):
            errors.append(f"distribution bioconda pull_request {key} must be non-empty text or null")
    if not isinstance(pull_request.get("url"), str) or not pull_request["url"].strip():
        errors.append("distribution bioconda pull_request url must be non-empty text")

    community = _mapping(report.get("community"), "community", errors)
    for key in (
        "open_issue_count_excluding_pull_requests",
        "pull_requests_excluded",
        "response_page_size",
        "maintainer_authored_issue_count",
        "external_authored_issue_count",
        "unknown_issue_author_count",
    ):
        value = community.get(key)
        if not isinstance(value, int) or isinstance(value, bool) or value < 0:
            errors.append(f"community {key} must be a non-negative integer")
    for key in (
        "open_issue_numbers",
        "maintainer_authored_issue_numbers",
        "external_authored_issue_numbers",
        "unknown_issue_author_numbers",
    ):
        value = community.get(key)
        if not isinstance(value, list) or not all(
            isinstance(item, int) and not isinstance(item, bool) for item in value
        ):
            errors.append(f"community {key} must be a list of integers")
    if not isinstance(community.get("possibly_truncated"), bool):
        errors.append("community possibly_truncated must be boolean")
    trial_thread = _mapping(community.get("trial_report_thread"), "community trial_report_thread", errors)
    if not isinstance(trial_thread.get("issue_number"), int) or isinstance(
        trial_thread.get("issue_number"), bool
    ):
        errors.append("community trial_report_thread issue_number must be an integer")
    if not isinstance(trial_thread.get("state"), str) or not trial_thread["state"].strip():
        errors.append("community trial_report_thread state must be non-empty text")
    if not isinstance(trial_thread.get("url"), str) or not trial_thread["url"].strip():
        errors.append("community trial_report_thread url must be non-empty text")
    comment_count = trial_thread.get("comment_count")
    if comment_count is not None and (
        not isinstance(comment_count, int) or isinstance(comment_count, bool) or comment_count < 0
    ):
        errors.append("community trial_report_thread comment_count must be a non-negative integer or null")
    for key in (
        "comment_page_size",
        "maintainer_comment_count",
        "external_comment_count",
        "unknown_comment_count",
    ):
        value = trial_thread.get(key)
        if not isinstance(value, int) or isinstance(value, bool) or value < 0:
            errors.append(f"community trial_report_thread {key} must be a non-negative integer")
    if not isinstance(trial_thread.get("comments_possibly_truncated"), bool):
        errors.append("community trial_report_thread comments_possibly_truncated must be boolean")
    if trial_thread.get("author_provenance") not in {
        "maintainer",
        "external",
        "unknown",
        "not_in_open_response",
    }:
        errors.append("community trial_report_thread author_provenance is invalid")
    author_is_maintainer = trial_thread.get("author_is_maintainer")
    if author_is_maintainer is not None and not isinstance(author_is_maintainer, bool):
        errors.append("community trial_report_thread author_is_maintainer must be boolean or null")

    interpretation = _mapping(report.get("interpretation"), "interpretation", errors)
    for key in (
        "download_counts_are_distribution_signals",
        "repository_counts_are_public_interest_signals",
        "distribution_channels_are_read_only_signals",
        "sustained_external_usage_verified",
        "customer_demand_verified",
        "production_readiness_verified",
        "workflow_owner_trial_reports_verified",
        "trial_report_comments_are_community_signals",
        "community_provenance_is_recorded",
        "release_source_ready_verified",
        "public_package_matches_source_verified",
        "distribution_channels_match_workspace_verified",
    ):
        if not isinstance(interpretation.get(key), bool):
            errors.append(f"interpretation {key} must be boolean")
    for key in (
        "sustained_external_usage_verified",
        "customer_demand_verified",
        "production_readiness_verified",
        "workflow_owner_trial_reports_verified",
    ):
        if interpretation.get(key) is not False:
            errors.append(f"interpretation {key} must remain false")
    if interpretation.get("trial_report_comments_are_community_signals") is not True:
        errors.append("interpretation trial_report_comments_are_community_signals must remain true")
    if interpretation.get("community_provenance_is_recorded") is not True:
        errors.append("interpretation community_provenance_is_recorded must remain true")
    if interpretation.get("distribution_channels_are_read_only_signals") is not True:
        errors.append("interpretation distribution_channels_are_read_only_signals must remain true")
    return errors


def load_report(path: Path) -> object:
    return json.loads(path.read_text(encoding="utf-8"))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    args = parser.parse_args(argv)
    try:
        errors = collect_errors(load_report(args.report))
    except (OSError, json.JSONDecodeError) as error:
        print(f"public adoption report could not be read: {error}", file=sys.stderr)
        return 1
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("public adoption report shape and safety boundaries are valid")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
