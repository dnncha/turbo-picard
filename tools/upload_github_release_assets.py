#!/usr/bin/env python3
"""Upload verified distributions and provenance to an existing GitHub release."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen


ROOT = Path(__file__).resolve().parents[1]
API_ROOT = "https://api.github.com"
UPLOAD_ROOT = "https://uploads.github.com"


def workspace_version(root: Path = ROOT) -> str:
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(
        r'(?ms)^\[workspace\.package\]\s+.*?^version\s*=\s*"([^\"]+)"',
        text,
    )
    if match is None:
        raise ValueError("Cargo.toml is missing [workspace.package] version")
    return match.group(1)


def asset_digest(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def validate_assets(directory: Path, version: str) -> list[Path]:
    if not directory.is_dir():
        raise ValueError(f"release asset directory does not exist: {directory}")
    files = sorted(path for path in directory.iterdir() if path.is_file())
    required_metadata = {
        "GITHUB_SOURCE_SHA256.txt",
        "SHA256SUMS.txt",
        "turbo-picard-release-manifest.json",
    }
    names = {path.name for path in files}
    missing = sorted(required_metadata - names)
    if missing:
        raise ValueError("release assets are missing: " + ", ".join(missing))

    wheels = [path for path in files if path.name.startswith(f"turbo_picard-{version}-") and path.suffix == ".whl"]
    source_dists = [path for path in files if path.name == f"turbo_picard-{version}.tar.gz"]
    if len(wheels) != 4:
        raise ValueError(f"expected four {version} platform wheels, found {len(wheels)}")
    if len(source_dists) != 1:
        raise ValueError(f"expected one {version} source distribution, found {len(source_dists)}")
    allowed = required_metadata | {path.name for path in wheels + source_dists}
    extras = sorted(names - allowed)
    if extras:
        raise ValueError("unexpected release assets: " + ", ".join(extras))

    source_checksum = (directory / "GITHUB_SOURCE_SHA256.txt").read_text(encoding="utf-8").strip()
    if not re.fullmatch(rf"[0-9a-f]{{64}}  v{re.escape(version)}\.tar\.gz", source_checksum):
        raise ValueError("GITHUB_SOURCE_SHA256.txt must hash the matching v<version>.tar.gz archive")
    sums_text = (directory / "SHA256SUMS.txt").read_text(encoding="utf-8")
    sum_entries: dict[str, str] = {}
    for line in sums_text.splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  \./([^/]+)", line)
        if match is None:
            raise ValueError("SHA256SUMS.txt contains a malformed entry")
        sum_entries[match.group(2)] = match.group(1)
    distribution_names = {path.name for path in wheels + source_dists}
    if set(sum_entries) != distribution_names:
        raise ValueError("SHA256SUMS.txt must cover exactly the wheel and source distributions")
    for path in wheels + source_dists:
        if sum_entries[path.name] != asset_digest(path):
            raise ValueError(f"SHA256SUMS.txt digest does not match {path.name}")

    try:
        manifest = json.loads((directory / "turbo-picard-release-manifest.json").read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise ValueError(f"release handoff manifest is not valid JSON: {error}") from error
    if manifest.get("workspace_version") != version:
        raise ValueError("release handoff manifest version does not match Cargo.toml")
    return files


def request_json(
    url: str,
    token: str,
    *,
    method: str = "GET",
    data: bytes | None = None,
    content_type: str = "application/json",
    opener=urlopen,
) -> dict[str, Any]:
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    if data is not None:
        headers["Content-Type"] = content_type
    request = Request(url, data=data, headers=headers, method=method)
    try:
        with opener(request, timeout=60) as response:
            payload = response.read()
    except HTTPError as error:
        # Never include request headers in this error; they contain the token.
        details = error.read().decode("utf-8", errors="replace")[:500]
        raise RuntimeError(f"GitHub release asset request failed ({error.code}): {details}") from error
    except URLError as error:
        raise RuntimeError(f"GitHub release asset request failed: {error.reason}") from error
    try:
        value = json.loads(payload.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError("GitHub release asset API returned invalid JSON") from error
    if not isinstance(value, dict):
        raise RuntimeError("GitHub release asset API returned an unexpected response")
    return value


def upload_release_assets(
    directory: Path,
    *,
    repository: str,
    tag: str,
    token: str,
    root: Path = ROOT,
    opener=urlopen,
) -> list[str]:
    version = workspace_version(root)
    expected_tag = f"v{version}"
    if tag != expected_tag:
        raise ValueError(f"release tag {tag!r} must match workspace version tag {expected_tag!r}")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("GITHUB_REPOSITORY must be in owner/name form")
    files = validate_assets(directory, version)

    release_url = f"{API_ROOT}/repos/{repository}/releases/tags/{quote(tag, safe='')}"
    release = request_json(release_url, token, opener=opener)
    if release.get("tag_name") != tag or release.get("draft") is not False:
        raise RuntimeError("the matching published GitHub release was not found")
    upload_url = release.get("upload_url")
    expected_upload_prefix = f"{UPLOAD_ROOT}/repos/{repository}/releases/"
    if not isinstance(upload_url, str) or not upload_url.startswith(expected_upload_prefix):
        raise RuntimeError("GitHub returned an unexpected release asset upload URL")
    upload_url = upload_url.split("{", 1)[0]

    existing = release.get("assets", [])
    if not isinstance(existing, list):
        raise RuntimeError("GitHub release asset list is malformed")
    by_name = {asset.get("name"): asset for asset in existing if isinstance(asset, dict)}

    # Reject conflicting names before uploading anything. Identical assets make
    # reruns safe without replacing or deleting already-published files.
    pending: list[Path] = []
    for path in files:
        current = by_name.get(path.name)
        if current is None:
            pending.append(path)
            continue
        current_digest = current.get("digest")
        expected_digest = f"sha256:{asset_digest(path)}"
        if current.get("size") != path.stat().st_size or current_digest != expected_digest:
            raise RuntimeError(f"release already contains a different asset named {path.name}")

    uploaded: list[str] = []
    for path in pending:
        data = path.read_bytes()
        url = f"{upload_url}?name={quote(path.name, safe='')}"
        response = request_json(
            url,
            token,
            method="POST",
            data=data,
            content_type="application/octet-stream",
            opener=opener,
        )
        expected_digest = f"sha256:{hashlib.sha256(data).hexdigest()}"
        if response.get("name") != path.name or response.get("size") != len(data):
            raise RuntimeError(f"GitHub did not confirm the uploaded asset {path.name}")
        response_digest = response.get("digest")
        if response_digest is not None and response_digest != expected_digest:
            raise RuntimeError(f"GitHub reported a different digest for {path.name}")
        uploaded.append(path.name)
    return uploaded


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args(argv)
    repository = os.environ.get("GITHUB_REPOSITORY", "")
    tag = os.environ.get("GITHUB_REF_NAME", "")
    token = os.environ.get("GITHUB_TOKEN", "")
    if os.environ.get("GITHUB_REF_TYPE") != "tag":
        parser.error("release assets may be uploaded only from a tag ref")
    if not token:
        parser.error("GITHUB_TOKEN is required")
    try:
        uploaded = upload_release_assets(
            args.directory,
            repository=repository,
            tag=tag,
            token=token,
        )
    except (OSError, ValueError, RuntimeError) as error:
        print(f"release asset upload failed: {error}", file=sys.stderr)
        return 1
    print("Uploaded release assets: " + (", ".join(uploaded) if uploaded else "all matching assets already exist"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
