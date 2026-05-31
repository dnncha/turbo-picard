#!/usr/bin/env python3
"""Ensure the marketing site discloses current release boundaries."""

from __future__ import annotations

import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
SITE = ROOT / "docs" / "site" / "index.html"


def normalize(text: str) -> str:
    text = re.sub(r"<[^>]+>", " ", text)
    text = text.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
    return re.sub(r"\s+", " ", text).strip().lower()


def validate_site_disclosures(html: str) -> list[str]:
    text = normalize(html)
    errors: list[str] = []
    if "current boundaries" not in text:
        errors.append("site missing current-boundaries section")
    if "not a full picard suite" not in text:
        errors.append("site missing not-full-Picard-suite disclosure")
    if "placeholder" not in text or "pdf" not in text or "metrics text" not in text:
        errors.append("site missing placeholder chart PDF disclosure")
    if (
        "bioconda" not in text
        or "tagged release" not in text
        or "sha256" not in text
        or "release-ready verifier" not in text
        or "mulled-test" not in text
    ):
        errors.append("site missing Bioconda tagged-release/sha256 disclosure")
    return errors


def main() -> int:
    errors = validate_site_disclosures(SITE.read_text(encoding="utf-8"))
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
