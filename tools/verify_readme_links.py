#!/usr/bin/env python3
"""Verify local README links and images resolve inside the repository."""

from __future__ import annotations

from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlparse


ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "README.md"


LINK_PATTERN = re.compile(r"(!?)\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")


def local_reference_path(root: Path, reference: str) -> Path | None:
    parsed = urlparse(reference)
    if parsed.scheme or parsed.netloc:
        return None
    if parsed.path == "":
        return None
    return (root / unquote(parsed.path)).resolve()


def validate_readme_links(readme_text: str, root: Path = ROOT) -> list[str]:
    errors: list[str] = []
    for is_image, target in LINK_PATTERN.findall(readme_text):
        path = local_reference_path(root, target)
        if path is None:
            continue
        try:
            path.relative_to(root.resolve())
        except ValueError:
            errors.append(f"README local reference escapes repository: {target}")
            continue
        if not path.exists():
            kind = "image" if is_image else "link"
            errors.append(f"README missing local {kind} target: {target}")
    return errors


def main() -> int:
    errors = validate_readme_links(README.read_text(encoding="utf-8"))
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
