#!/usr/bin/env python3
"""Tests for marketing site disclosure checks."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_site_disclosures.py")
SPEC = importlib.util.spec_from_file_location("verify_site_disclosures", MODULE_PATH)
assert SPEC is not None
verify_site_disclosures = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_site_disclosures"] = verify_site_disclosures
SPEC.loader.exec_module(verify_site_disclosures)


class SiteDisclosureTests(unittest.TestCase):
    def test_site_disclosure_accepts_current_boundaries(self) -> None:
        html = """
<section>
  <h2>Current boundaries</h2>
  <p>turbo-picard is not a full Picard suite yet.</p>
  <p>Chart outputs are placeholder PDFs; metrics text is the parity target.</p>
  <p>Bioconda recipes are prepared, but submission still needs a tagged release, sha256, release-ready verifier, and mulled-test.</p>
</section>
"""

        self.assertEqual(verify_site_disclosures.validate_site_disclosures(html), [])

    def test_site_disclosure_reports_missing_boundaries(self) -> None:
        errors = verify_site_disclosures.validate_site_disclosures(
            "<section><h2>Benchmarks</h2><p>Fast Picard replacement.</p></section>"
        )

        self.assertIn("site missing current-boundaries section", errors)
        self.assertIn("site missing not-full-Picard-suite disclosure", errors)
        self.assertIn("site missing placeholder chart PDF disclosure", errors)
        self.assertIn("site missing Bioconda tagged-release/sha256 disclosure", errors)


if __name__ == "__main__":
    unittest.main()
