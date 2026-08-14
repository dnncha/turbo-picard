#!/usr/bin/env python3
"""Validate the shape and safety boundaries of a public adoption report."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys
from typing import Any


SCHEMA_VERSION = 2
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

    interpretation = _mapping(report.get("interpretation"), "interpretation", errors)
    for key in (
        "download_counts_are_distribution_signals",
        "repository_counts_are_public_interest_signals",
        "sustained_external_usage_verified",
        "customer_demand_verified",
        "production_readiness_verified",
        "workflow_owner_trial_reports_verified",
        "trial_report_comments_are_community_signals",
        "release_source_ready_verified",
        "public_package_matches_source_verified",
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
