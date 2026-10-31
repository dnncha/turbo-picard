#!/usr/bin/env python3

from __future__ import annotations

import importlib
import json
import os
import re
import subprocess
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.1.17"
TAG = f"v{VERSION}"
REPOSITORY = "dnncha/turbo-picard"
AUTHORIZED_BASE = "a4aeaa83659c581b7e2b7d587ad51faf4d48bc06"


def validate_run(run: dict[str, Any], repository: str) -> str:
    expected = {
        "name": "CI",
        "path": ".github/workflows/ci.yml",
        "event": "push",
        "head_branch": "main",
        "status": "completed",
        "conclusion": "success",
    }
    if repository != REPOSITORY or any(run.get(k) != v for k, v in expected.items()):
        raise ValueError("Release requires completed successful push CI on this repository's main")
    if run.get("head_repository", {}).get("full_name") != repository:
        raise ValueError("Release cannot consume a fork's CI")
    sha = run.get("head_sha", "")
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("CI source SHA must be immutable")
    return str(sha)


def workspace_version() -> str:
    tomllib = importlib.import_module("tomllib")
    version: str = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    return version


def present(value: dict[str, Any] | None) -> dict[str, Any]:
    if value is None:
        raise RuntimeError("GitHub API returned no content")
    return value


def api(path: str, payload: dict[str, Any] | None = None, optional: bool = False) -> dict[str, Any] | None:
    argv = ["gh", "api", f"repos/{REPOSITORY}/{path}"]
    if payload is not None:
        argv += ["--method", "POST", "--input", "-"]
    result = subprocess.run(
        argv, input=None if payload is None else json.dumps(payload), text=True, capture_output=True, check=False
    )
    if result.returncode:
        if optional and "HTTP 404" in result.stderr:
            return None
        raise RuntimeError(f"GitHub API operation failed: {path}: {result.stderr}")
    parsed: dict[str, Any] | None = json.loads(result.stdout) if result.stdout.strip() else None
    return parsed


def main() -> int:
    if os.environ.get("GITHUB_EVENT_NAME") != "workflow_run":
        raise ValueError("Only the guarded workflow_run release is supported")
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
    if event.get("action") != "completed":
        raise ValueError("CI has not completed")
    run = event["workflow_run"]
    repository = os.environ["GITHUB_REPOSITORY"]
    sha = validate_run(run, repository)
    checked_out = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if checked_out != sha:
        raise ValueError("Checkout does not match the tested CI source")
    version = workspace_version()
    if version != VERSION:
        print(f"No release requested for {version}; this workflow authorizes only {VERSION}")
        return 0
    subprocess.run(["git", "merge-base", "--is-ancestor", AUTHORIZED_BASE, sha], check=True)
    if validate_run(present(api(f"actions/runs/{int(run['id'])}")), repository) != sha:
        raise ValueError("Live CI metadata disagrees with the triggering source")
    subprocess.run(["python3", "tools/verify_release_versions.py"], check=True)
    notes = (ROOT / "docs" / "releases" / f"{TAG}.md").read_text()
    ref = api(f"git/ref/tags/{TAG}", optional=True)
    if ref is not None:
        if ref["object"]["type"] != "commit":
            raise ValueError("Release tag must point directly to the tested commit")
        if ref["object"]["sha"] != sha:
            if api(f"releases/tags/{TAG}", optional=True) is not None:
                print(f"{TAG} already released from {ref['object']['sha']}; leaving it immutable")
                return 0
            raise ValueError("Existing unpublished release tag points to another commit")
    else:
        api("git/refs", {"ref": f"refs/tags/{TAG}", "sha": sha})
    release = api(f"releases/tags/{TAG}", optional=True)
    if release is None:
        release = present(
            api(
                "releases",
                {
                    "tag_name": TAG,
                    "target_commitish": sha,
                    "name": f"Turbo Picard {TAG}",
                    "body": notes,
                    "draft": False,
                    "prerelease": False,
                },
            )
        )
    if release.get("draft") or release.get("prerelease"):
        raise ValueError("An existing draft or prerelease needs explicit resolution")
    for workflow, inputs in (("publish-pypi.yml", {"publish": "true"}), ("publish-docker.yml", {})):
        runs = present(api(f"actions/workflows/{workflow}/runs?branch={TAG}&event=workflow_dispatch&per_page=100"))
        if any(r.get("head_sha") == sha for r in runs["workflow_runs"]):
            print(f"{workflow} already dispatched for {sha}")
        else:
            api(f"actions/workflows/{workflow}/dispatches", {"ref": TAG, "inputs": inputs})
            print(f"Dispatched {workflow} for {TAG} ({sha})")
    print(release["html_url"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
