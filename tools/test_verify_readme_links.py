#!/usr/bin/env python3
"""Tests for README local link checks."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_readme_links.py")
SPEC = importlib.util.spec_from_file_location("verify_readme_links", MODULE_PATH)
assert SPEC is not None
verify_readme_links = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_readme_links"] = verify_readme_links
SPEC.loader.exec_module(verify_readme_links)


class ReadmeLinkTests(unittest.TestCase):
    def test_accepts_existing_local_links_and_ignores_external_links(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "docs").mkdir()
            (root / "docs" / "index.rst").write_text("Docs\n", encoding="utf-8")
            (root / "assets").mkdir()
            (root / "assets" / "hero.png").write_text("image\n", encoding="utf-8")
            readme = """
![hero](assets/hero.png)
[docs](docs/index.rst)
[external](https://example.org/docs)
"""

            self.assertEqual(
                verify_readme_links.validate_readme_links(readme, root),
                [],
            )

    def test_reports_missing_local_link_and_image_targets(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            readme = """
![hero](assets/missing.png)
[docs](docs/missing.rst)
"""

            self.assertEqual(
                verify_readme_links.validate_readme_links(readme, root),
                [
                    "README missing local image target: assets/missing.png",
                    "README missing local link target: docs/missing.rst",
                ],
            )

    def test_rejects_local_references_that_escape_repository(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            readme = "[outside](../outside.txt)\n"

            self.assertEqual(
                verify_readme_links.validate_readme_links(readme, root),
                ["README local reference escapes repository: ../outside.txt"],
            )


if __name__ == "__main__":
    unittest.main()
