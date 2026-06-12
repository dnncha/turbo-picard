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
<head>
  <meta name="description" content="selected Picard commands with fallback for unsupported commands">
</head>
<section>
  <h2>Current boundaries</h2>
  <p>turbo-picard is not a full Picard suite yet.</p>
  <p>Chart outputs are lightweight PDF summaries; metrics text is the parity target.</p>
  <p>Switch only the commands where the evidence supports the change.</p>
  <p>The benchmark threshold gate is python3 tools/verify_benchmark_thresholds.py with 5.00x floor speedup, 20.00x geometric mean speedup, and 50.00x top speedup.</p>
  <p>The software citation lives in CITATION.cff. Cite the archived turbo-picard release and cite benchmark inputs separately with SHA-256.</p>
  <p>Bioconda release v0.1.2 uses python3 tools/bioconda_release_preflight.py and bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim.</p>
</section>
"""

        self.assertEqual(verify_site_disclosures.validate_site_disclosures(html, version="0.1.2"), [])

    def test_site_disclosure_reports_missing_boundaries(self) -> None:
        errors = verify_site_disclosures.validate_site_disclosures(
            "<section><h2>Benchmarks</h2><p>Fast Picard replacement.</p></section>",
            version="0.1.2",
        )

        self.assertIn("site missing current-boundaries section", errors)
        self.assertIn("site missing not-full-Picard-suite disclosure", errors)
        self.assertIn("site metadata missing selected-command caveat", errors)
        self.assertIn("site metadata missing fallback/unsupported-command caveat", errors)
        self.assertIn("site missing lightweight chart PDF disclosure", errors)
        self.assertIn("site missing evidence-supported switch disclosure", errors)
        self.assertIn("site missing benchmark threshold release-gate disclosure", errors)
        self.assertIn("site missing software-vs-input citation disclosure", errors)
        self.assertIn("site missing Bioconda release/lint disclosure", errors)

    def test_site_disclosure_reports_metadata_overclaim(self) -> None:
        html = """
<head>
  <meta name="description" content="selected Picard commands with fallback for unsupported commands across production genomics workflows">
</head>
<section>
  <h2>Current boundaries</h2>
  <p>turbo-picard is not a full Picard suite yet.</p>
  <p>Chart outputs are lightweight PDF summaries; metrics text is the parity target.</p>
  <p>Switch only the commands where the evidence supports the change.</p>
  <p>The benchmark threshold gate is python3 tools/verify_benchmark_thresholds.py with 5.00x floor speedup, 20.00x geometric mean speedup, and 50.00x top speedup.</p>
  <p>The software citation lives in CITATION.cff. Cite the archived turbo-picard release and cite benchmark inputs separately with SHA-256.</p>
  <p>Bioconda release v0.1.2 uses python3 tools/bioconda_release_preflight.py and bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim.</p>
</section>
"""

        self.assertIn(
            "site metadata contains unsupported production-genomics overclaim",
            verify_site_disclosures.validate_site_disclosures(html, version="0.1.2"),
        )

    def test_site_disclosure_requires_bioconda_release_and_lint(self) -> None:
        html = """
<head>
  <meta name="description" content="selected Picard commands with fallback for unsupported commands">
</head>
<section>
  <h2>Current boundaries</h2>
  <p>turbo-picard is not a full Picard suite yet.</p>
  <p>Chart outputs are lightweight PDF summaries; metrics text is the parity target.</p>
  <p>Switch only the commands where the evidence supports the change.</p>
  <p>The benchmark threshold gate is python3 tools/verify_benchmark_thresholds.py with 5.00x floor speedup, 20.00x geometric mean speedup, and 50.00x top speedup.</p>
  <p>The software citation lives in CITATION.cff. Cite the archived turbo-picard release and cite benchmark inputs separately with SHA-256.</p>
  <p>Bioconda uses python3 tools/bioconda_release_preflight.py.</p>
</section>
"""

        self.assertIn(
            "site missing Bioconda release/lint disclosure",
            verify_site_disclosures.validate_site_disclosures(html, version="0.1.2"),
        )

    def test_site_disclosure_requires_citation_boundary(self) -> None:
        html = """
<head>
  <meta name="description" content="selected Picard commands with fallback for unsupported commands">
</head>
<section>
  <h2>Current boundaries</h2>
  <p>turbo-picard is not a full Picard suite yet.</p>
  <p>Chart outputs are lightweight PDF summaries; metrics text is the parity target.</p>
  <p>Switch only the commands where the evidence supports the change.</p>
  <p>The benchmark threshold gate is python3 tools/verify_benchmark_thresholds.py with 5.00x floor speedup, 20.00x geometric mean speedup, and 50.00x top speedup.</p>
  <p>Bioconda release v0.1.1 uses python3 tools/bioconda_release_preflight.py and bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim.</p>
</section>
"""

        self.assertIn(
            "site missing software-vs-input citation disclosure",
            verify_site_disclosures.validate_site_disclosures(html, version="0.1.2"),
        )


if __name__ == "__main__":
    unittest.main()
