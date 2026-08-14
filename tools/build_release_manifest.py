#!/usr/bin/env python3
"""Build a privacy-conscious, source-backed release handoff manifest."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import re
import subprocess
import sys
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
HEX40 = re.compile(r"^[0-9a-f]{40}$")
VERSION = re.compile(r"^\d+\.\d+\.\d+$")
ARTIFACT_VERSION = re.compile(r"turbo_picard[-_]?(\d+\.\d+\.\d+)")


def workspace_version(root: Path = ROOT) -> str:
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(
        r'(?ms)^\[workspace\.package\]\s+.*?^version\s*=\s*"([^"]+)"',
        text,
    )
    if match is None:
        raise ValueError("Cargo.toml missing [workspace.package] version")
    version = match.group(1)
    if not VERSION.fullmatch(version):
        raise ValueError(f"workspace version is not semantic version text: {version}")
    return version


GitRunner = Callable[[list[str], Path], tuple[int, list[str]]]
ArtifactValidator = Callable[[Path, Path], list[str]]


def run_git(args: list[str], root: Path = ROOT) -> tuple[int, list[str]]:
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
    for line in lines:
        parts = line.split()
        if len(parts) == 2 and parts[1] == f"refs/tags/{tag}^{{}}":
            return parts[0]
    for line in lines:
        parts = line.split()
        if len(parts) == 2 and parts[1] == f"refs/tags/{tag}":
            return parts[0]
    return None


def collect_source_state(
    root: Path = ROOT,
    *,
    git_runner: GitRunner = run_git,
) -> dict[str, Any]:
    version = workspace_version(root)
    tag = f"v{version}"
    status, status_lines = git_runner(["status", "--porcelain"], root)
    head_status, head_lines = git_runner(["rev-parse", "HEAD"], root)
    branch_status, branch_lines = git_runner(
        ["rev-parse", "--abbrev-ref", "HEAD"], root
    )
    local_tag_status, local_tag_lines = git_runner(["rev-list", "-n", "1", tag], root)
    remote_status, remote_lines = git_runner(
        ["ls-remote", "--tags", "origin", f"{tag}*"], root
    )

    head = _first_line(head_status, head_lines)
    local_tag_commit = _first_line(local_tag_status, local_tag_lines)
    origin_tag_commit = _remote_tag_commit(remote_lines, tag) if remote_status == 0 else None
    worktree_clean = status == 0 and not status_lines
    tag_matches_head = head is not None and local_tag_commit == head
    origin_tag_matches_local = (
        local_tag_commit is not None
        and origin_tag_commit is not None
        and local_tag_commit == origin_tag_commit
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
    if remote_status != 0:
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
        "release_source_ready": not blockers,
        "blockers": blockers,
    }


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def artifact_records(dist: Path, version: str) -> list[dict[str, Any]]:
    if not dist.is_dir():
        raise ValueError(f"distribution directory does not exist: {dist}")
    files = sorted(path for path in dist.iterdir() if path.is_file())
    if not files:
        raise ValueError(f"distribution directory contains no files: {dist}")
    records: list[dict[str, Any]] = []
    for path in files:
        if not (path.name.endswith(".whl") or path.name.endswith(".tar.gz")):
            raise ValueError(f"unexpected distribution artifact: {path.name}")
        match = ARTIFACT_VERSION.search(path.name)
        if match is None:
            raise ValueError(f"artifact filename has no version: {path.name}")
        artifact_version = match.group(1)
        if artifact_version != version:
            raise ValueError(
                f"artifact {path.name} version {artifact_version} must be {version}"
            )
        records.append(
            {
                "filename": path.name,
                "kind": "wheel" if path.name.endswith(".whl") else "sdist",
                "bytes": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        )
    return records


def validate_artifacts(dist: Path, root: Path = ROOT) -> list[str]:
    completed = subprocess.run(
        [sys.executable, str(root / "tools" / "verify_release_artifacts.py"), "--dist", str(dist)],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode == 0:
        return []
    return [line.strip() for line in (completed.stdout + completed.stderr).splitlines() if line.strip()]


def summarize_benchmark(path: Path) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read benchmark profile {path}: {error}") from error
    if not isinstance(payload, dict) or not isinstance(payload.get("benchmarks"), list):
        raise ValueError("benchmark profile must contain a benchmarks list")
    rows = payload["benchmarks"]
    if not rows:
        raise ValueError("benchmark profile contains no benchmark rows")
    speeds: list[float] = []
    commands: dict[str, float] = {}
    for index, row in enumerate(rows):
        if not isinstance(row, dict):
            raise ValueError(f"benchmark row {index} must be an object")
        if row.get("parity") != "PASS":
            raise ValueError(f"benchmark row {index} parity must be PASS")
        command = row.get("command")
        speedup = row.get("median_speedup")
        if not isinstance(command, str) or not command.strip():
            raise ValueError(f"benchmark row {index} command must be non-empty")
        if not isinstance(speedup, (int, float)) or isinstance(speedup, bool) or speedup <= 0:
            raise ValueError(f"benchmark row {index} median_speedup must be positive")
        speeds.append(float(speedup))
        commands[command] = float(speedup)
    return {
        "profile_filename": path.name,
        "command_count": len(rows),
        "all_parity_pass": True,
        "geometric_mean_speedup": math_geometric_mean(speeds),
        "minimum_speedup": min(speeds),
        "maximum_speedup": max(speeds),
        "markduplicates_speedup": commands.get("MarkDuplicates"),
    }


def math_geometric_mean(values: list[float]) -> float:
    return math.exp(sum(math.log(value) for value in values) / len(values))


def build_manifest(
    *,
    root: Path,
    dist: Path,
    benchmark_profile: Path | None = None,
    git_runner: GitRunner = run_git,
    artifact_validator: ArtifactValidator = validate_artifacts,
) -> dict[str, Any]:
    version = workspace_version(root)
    artifact_errors = artifact_validator(dist, root)
    if artifact_errors:
        raise ValueError("release artifact validation failed: " + "; ".join(artifact_errors))
    source = collect_source_state(root, git_runner=git_runner)
    manifest: dict[str, Any] = {
        "schema_version": 1,
        "observed_at_utc": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "workspace_version": version,
        "source": source,
        "artifacts": artifact_records(dist, version),
        "interpretation": {
            "status": "release_candidate" if not source["release_source_ready"] else "release_source_ready",
            "publication_performed": False,
            "production_scale_verified": False,
            "independent_reproduction_verified": False,
        },
    }
    if benchmark_profile is not None:
        manifest["benchmark"] = summarize_benchmark(benchmark_profile)
    return manifest


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dist", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--benchmark-profile", type=Path)
    args = parser.parse_args(argv)
    try:
        manifest = build_manifest(
            root=ROOT,
            dist=args.dist,
            benchmark_profile=args.benchmark_profile,
        )
    except (OSError, ValueError) as error:
        print(f"release manifest failed: {error}", file=sys.stderr)
        return 1
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
