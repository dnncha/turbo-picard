#!/usr/bin/env python3
"""Verify local links and images in the checked-in marketing site."""

from __future__ import annotations

from html.parser import HTMLParser
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlparse


ROOT = Path(__file__).resolve().parents[1]
SITE_ROOT = ROOT / "docs" / "site"
SITE = SITE_ROOT / "index.html"


class SiteReferenceParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.links: list[str] = []
        self.images: list[tuple[str, str | None]] = []
        self.anchors: set[str] = set()

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attr_map = {key: value for key, value in attrs if value is not None}
        if "id" in attr_map:
            self.anchors.add(attr_map["id"])
        if tag == "a" and "name" in attr_map:
            self.anchors.add(attr_map["name"])
        if tag == "a" and "href" in attr_map:
            self.links.append(attr_map["href"])
        elif tag == "img" and "src" in attr_map:
            self.images.append((attr_map["src"], attr_map.get("alt")))


def local_reference_path(site_root: Path, reference: str) -> Path | None:
    parsed = urlparse(reference)
    if parsed.scheme or parsed.netloc:
        return None
    if parsed.path == "":
        return None
    return (site_root / unquote(parsed.path)).resolve()


def validate_site_links(html: str, site_root: Path = SITE_ROOT) -> list[str]:
    parser = SiteReferenceParser()
    parser.feed(html)

    errors: list[str] = []
    for href in parser.links:
        path = local_reference_path(site_root, href)
        parsed = urlparse(href)
        if parsed.scheme or parsed.netloc:
            continue
        if parsed.path.endswith(".rst"):
            errors.append(f"site must link reader-facing docs, not source rst: {href}")
            continue
        if parsed.fragment and parsed.path == "":
            if unquote(parsed.fragment) not in parser.anchors:
                errors.append(f"missing local site anchor target: {href}")
            continue
        if path is not None and not path.exists():
            errors.append(f"missing local site link target: {href}")
    for src, alt in parser.images:
        path = local_reference_path(site_root, src)
        if path is not None and not path.exists():
            errors.append(f"missing local site image target: {src}")
        if alt is None or not alt.strip():
            errors.append(f"site image missing meaningful alt text: {src}")
        elif re.search(r"\b(image|graphic|picture|chart)\b", alt, re.IGNORECASE):
            errors.append(f"site image alt text is too generic: {src}")
    return errors


def main() -> int:
    errors = []
    for page in sorted(SITE_ROOT.rglob("*.html")):
        errors.extend(validate_site_links(page.read_text(encoding="utf-8"), page.parent))
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
