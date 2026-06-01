#!/usr/bin/env python3
"""Print a concise Bioconda release preflight report."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EXPECTED_ARCHIVE_ERRORS = {
    "packaging/bioconda/BIOCONDA_PR.md still contains source archive SHA placeholder",
    "packaging/bioconda/BIOCONDA_PR.md missing concrete source archive SHA-256",
    "turbo-picard meta.yaml still uses local source.path",
    "turbo-picard meta.yaml missing release source url",
    "turbo-picard meta.yaml missing release source sha256",
    "turbo-picard-picard-shim meta.yaml still uses local source.path",
    "turbo-picard-picard-shim meta.yaml missing release source url",
    "turbo-picard-picard-shim meta.yaml missing release source sha256",
}


def git_status(root: Path = ROOT) -> tuple[str, list[str]]:
    status, lines = run_check(["git", "status", "--porcelain"], root)
    if status != 0:
        return "FAIL", lines or ["git status --porcelain failed"]
    if lines:
        preview = lines[:8]
        if len(lines) > len(preview):
            preview.append(f"... {len(lines) - len(preview)} more changed paths")
        return "WAIT", preview
    return "OK", []


def workspace_version(root: Path = ROOT) -> str | None:
    cargo_toml = root / "Cargo.toml"
    if not cargo_toml.is_file():
        return None
    match = re.search(
        r"(?ms)^\[workspace\.package\]\s+.*?^version\s*=\s*\"([^\"]+)\"",
        cargo_toml.read_text(encoding="utf-8"),
    )
    return match.group(1) if match else None


def git_tag_status(root: Path = ROOT) -> tuple[str, list[str]]:
    version = workspace_version(root)
    if not version:
        return "FAIL", ["Cargo.toml is missing [workspace.package] version"]
    tag = f"v{version}"
    local_status, _local_lines = run_check(
        ["git", "show-ref", "--verify", "--quiet", f"refs/tags/{tag}"],
        root,
    )
    if local_status != 0:
        return "WAIT", [f"local tag {tag} does not exist yet"]

    remote_status, remote_lines = run_check(["git", "ls-remote", "--tags", "origin", tag], root)
    if remote_status != 0:
        return "WAIT", [f"could not confirm origin tag {tag}"] + remote_lines
    if not remote_lines:
        return "WAIT", [f"origin tag {tag} does not exist yet"]
    return "OK", [tag]


def run_check(command: list[str], root: Path = ROOT) -> tuple[int, list[str]]:
    completed = subprocess.run(
        command,
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


def preflight_report(root: Path = ROOT) -> tuple[int, str]:
    checks = [
        ("real-data release evidence", ["python3", "tools/verify_real_data_evidence.py", "--release-ready"]),
        ("release version and citation metadata", ["python3", "tools/verify_release_versions.py"]),
        ("release-facing prose quality", ["python3", "tools/verify_release_text_quality.py"]),
        ("Bioconda recipe shape", ["python3", "tools/verify_bioconda_recipes.py"]),
    ]
    output = ["Bioconda release preflight", ""]
    failed = False
    git_state, git_lines = git_status(root)
    if git_state == "OK":
        output.append("OK: git worktree clean for release tagging")
    elif git_state == "WAIT":
        failed = True
        output.append("WAIT: git worktree has uncommitted changes")
        output.append("  Commit the intended release state before tagging.")
        output.extend(f"  {line}" for line in git_lines)
    else:
        failed = True
        output.append("FAIL: git worktree status")
        output.extend(f"  {line}" for line in git_lines)

    tag_state, tag_lines = git_tag_status(root)
    if tag_state == "OK":
        output.append(f"OK: release tag {tag_lines[0]} exists locally and on origin")
    elif tag_state == "WAIT":
        failed = True
        output.append("WAIT: release tag not ready")
        output.extend(f"  {line}" for line in tag_lines)
    else:
        failed = True
        output.append("FAIL: release tag check")
        output.extend(f"  {line}" for line in tag_lines)

    for label, command in checks:
        status, lines = run_check(command, root)
        if status == 0:
            output.append(f"OK: {label}")
        else:
            failed = True
            output.append(f"FAIL: {label}")
            output.extend(f"  {line}" for line in lines)

    status, release_lines = run_check(
        ["python3", "tools/verify_bioconda_recipes.py", "--release-ready"],
        root,
    )
    if status == 0:
        output.append("OK: Bioconda release-ready source metadata")
        return (0 if not failed else 1), "\n".join(output) + "\n"

    release_errors = set(release_lines)
    if release_errors and release_errors <= EXPECTED_ARCHIVE_ERRORS:
        output.append("WAIT: Bioconda release-ready source metadata")
        output.append("  The recipes are still in local source.path mode.")
        output.append("  After tagging the exact release commit, download the GitHub archive and run:")
        output.append("  python3 tools/prepare_bioconda_release.py --archive ~/Downloads/turbo-picard-0.1.0.tar.gz")
        output.append("  python3 tools/verify_bioconda_recipes.py --release-ready")
        output.append("  Then copy both recipes into bioconda-recipes and run lint plus Docker/mulled builds.")
        return 1, "\n".join(output) + "\n"

    output.append("FAIL: Bioconda release-ready source metadata")
    output.extend(f"  {line}" for line in release_lines)
    return 1, "\n".join(output) + "\n"


def main() -> int:
    status, report = preflight_report()
    stream = sys.stdout if status == 0 else sys.stderr
    stream.write(report)
    return status


if __name__ == "__main__":
    raise SystemExit(main())
