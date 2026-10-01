#!/usr/bin/env python3
"""Publish the authorized v0.1.16 once, from successful immutable main CI."""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.1.16"
TAG = f"v{VERSION}"
REPOSITORY = "dnncha/turbo-picard"
AUTHORIZED_BASE = "3e571f973f7f8b62ae0c0302e67635d1922e3a6d"


def validate_run(run: dict, repository: str) -> str:
    expected = {
        "name": "CI", "path": ".github/workflows/ci.yml",
        "event": "push", "head_branch": "main",
        "status": "completed", "conclusion": "success",
    }
    if repository != REPOSITORY or any(run.get(k) != v for k, v in expected.items()):
        raise ValueError("Release requires completed successful push CI on this repository's main")
    if run.get("head_repository", {}).get("full_name") != repository:
        raise ValueError("Release cannot consume a fork's CI")
    sha = run.get("head_sha", "")
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("CI source SHA must be immutable")
    return sha


def api(path: str, payload: dict | None = None, optional: bool = False):
    argv = ["gh", "api", f"repos/{REPOSITORY}/{path}"]
    if payload is not None:
        argv += ["--method", "POST", "--input", "-"]
    result = subprocess.run(argv, input=None if payload is None else json.dumps(payload),
                            text=True, capture_output=True)
    if result.returncode:
        if optional and "HTTP 404" in result.stderr:
            return None
        raise RuntimeError(f"GitHub API operation failed: {path}: {result.stderr}")
    return json.loads(result.stdout) if result.stdout.strip() else None


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
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    if version != VERSION:
        print(f"No release requested for {version}; this workflow authorizes only {VERSION}")
        return 0
    subprocess.run(["git", "merge-base", "--is-ancestor", AUTHORIZED_BASE, sha], check=True)
    # Re-fetch trusted CI metadata before any mutation; never trust a fork artifact.
    if validate_run(api(f"actions/runs/{int(run['id'])}"), repository) != sha:
        raise ValueError("Live CI metadata disagrees with the triggering source")
    subprocess.run(["python3", "tools/verify_release_versions.py"], check=True)
    notes = (ROOT / "docs" / "releases" / f"{TAG}.md").read_text()
    ref = api(f"git/ref/tags/{TAG}", optional=True)
    if ref is not None:
        if ref["object"]["type"] != "commit":
            raise ValueError("Release tag must point directly to the tested commit")
        if ref["object"]["sha"] != sha:
            # Later main commits must never retag or republish this fixed release.
            if api(f"releases/tags/{TAG}", optional=True) is not None:
                print(f"{TAG} already released from {ref['object']['sha']}; leaving it immutable")
                return 0
            raise ValueError("Existing unpublished release tag points to another commit")
    else:
        api("git/refs", {"ref": f"refs/tags/{TAG}", "sha": sha})
    release = api(f"releases/tags/{TAG}", optional=True)
    if release is None:
        release = api("releases", {"tag_name": TAG, "target_commitish": sha,
                                  "name": f"Turbo Picard {TAG}", "body": notes,
                                  "draft": False, "prerelease": False})
    if release.get("draft") or release.get("prerelease"):
        raise ValueError("An existing draft or prerelease needs explicit resolution")
    # GITHUB_TOKEN release/tag mutations do not trigger workflows. Dispatch the
    # existing publishers explicitly; their exact-tag validation remains intact.
    for workflow, inputs in (("publish-pypi.yml", {"publish": "true"}),
                             ("publish-docker.yml", {})):
        runs = api(f"actions/workflows/{workflow}/runs?branch={TAG}&event=workflow_dispatch&per_page=100")
        if any(r.get("head_sha") == sha for r in runs["workflow_runs"]):
            print(f"{workflow} already dispatched for {sha}")
        else:
            api(f"actions/workflows/{workflow}/dispatches", {"ref": TAG, "inputs": inputs})
            print(f"Dispatched {workflow} for {TAG} ({sha})")
    print(release["html_url"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
