#!/usr/bin/env python3
"""Tests for marketing site link checks."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_site_links.py")
SPEC = importlib.util.spec_from_file_location("verify_site_links", MODULE_PATH)
assert SPEC is not None
verify_site_links = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_site_links"] = verify_site_links
SPEC.loader.exec_module(verify_site_links)


class SiteLinkTests(unittest.TestCase):
    def test_accepts_existing_local_links_and_ignores_external_links(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "assets").mkdir()
            (root / "assets" / "chart.svg").write_text("<svg></svg>", encoding="utf-8")
            (root / "README.md").write_text("# README\n", encoding="utf-8")
            html = """
<section id="benchmarks"></section>
<a href="README.md">README</a>
<a href="#benchmarks">Benchmarks</a>
<a href="https://example.org/project">External</a>
<img src="assets/chart.svg" alt="turbo-picard speedups versus Picard">
"""

            self.assertEqual(verify_site_links.validate_site_links(html, root), [])

    def test_reports_missing_local_links_and_images(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            html = """
<a href="missing.md">Missing</a>
<img src="assets/missing.svg" alt="Missing">
"""

            self.assertEqual(
                verify_site_links.validate_site_links(html, root),
                [
                    "missing local site link target: missing.md",
                    "missing local site image target: assets/missing.svg",
                ],
            )

    def test_handles_fragment_on_existing_local_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "docs.html").write_text("Docs\n", encoding="utf-8")

            self.assertEqual(
                verify_site_links.validate_site_links('<a href="docs.html#usage">Docs</a>', root),
                [],
            )

    def test_rejects_links_to_sphinx_source_rst(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "index.rst").write_text("Docs source\n", encoding="utf-8")

            self.assertEqual(
                verify_site_links.validate_site_links(
                    '<a href="index.rst">Open the docs</a>',
                    root,
                ),
                ["site must link reader-facing docs, not source rst: index.rst"],
            )

    def test_reports_missing_same_page_anchor(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            html = """
<section id="benchmarks"></section>
<a href="#benchmarks">Benchmarks</a>
<a href="#missing">Missing</a>
"""

            self.assertEqual(
                verify_site_links.validate_site_links(html, root),
                ["missing local site anchor target: #missing"],
            )

    def test_accepts_legacy_named_anchor(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            html = """
<a name="top"></a>
<a href="#top">Top</a>
"""

            self.assertEqual(verify_site_links.validate_site_links(html, root), [])

    def test_reports_missing_or_generic_image_alt_text(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "assets").mkdir()
            (root / "assets" / "one.svg").write_text("<svg></svg>", encoding="utf-8")
            (root / "assets" / "two.svg").write_text("<svg></svg>", encoding="utf-8")
            html = """
<img src="assets/one.svg">
<img src="assets/two.svg" alt="Chart">
"""

            self.assertEqual(
                verify_site_links.validate_site_links(html, root),
                [
                    "site image missing meaningful alt text: assets/one.svg",
                    "site image alt text is too generic: assets/two.svg",
                ],
            )


if __name__ == "__main__":
    unittest.main()
