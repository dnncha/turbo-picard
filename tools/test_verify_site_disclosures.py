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
  <meta name="description" content="command-by-command evaluation with fallback for unsupported Picard surfaces">
</head>
<section>
  <h2>Current boundaries</h2>
  <p>turbo-picard is not a full Picard suite yet.</p>
  <p>Chart outputs are lightweight PDF summaries; metrics text is the parity target.</p>
  <p>Switch only the workflow surfaces where the evidence supports a narrow change.</p>
  <p>The benchmark threshold gate is python3 tools/verify_benchmark_thresholds.py with 5.00x floor speedup, 20.00x geometric mean speedup, and 50.00x top speedup.</p>
  <p>The software citation lives in CITATION.cff. Cite the archived turbo-picard release and cite benchmark inputs separately with SHA-256.</p>
  <p>Bioconda recipes are prepared, but submission still needs a tagged release, python3 tools/bioconda_release_preflight.py, sha256, release-ready verifier, cp -R packaging/bioconda/turbo-picard recipes/turbo-picard, cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim, bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim, and mulled-test.</p>
</section>
"""

        self.assertEqual(verify_site_disclosures.validate_site_disclosures(html), [])

    def test_site_disclosure_reports_missing_boundaries(self) -> None:
        errors = verify_site_disclosures.validate_site_disclosures(
            "<section><h2>Benchmarks</h2><p>Fast Picard replacement.</p></section>"
        )

        self.assertIn("site missing current-boundaries section", errors)
        self.assertIn("site missing not-full-Picard-suite disclosure", errors)
        self.assertIn("site metadata missing command-by-command evaluation caveat", errors)
        self.assertIn("site metadata missing fallback/unsupported-surface caveat", errors)
        self.assertIn("site missing lightweight chart PDF disclosure", errors)
        self.assertIn("site missing narrow evidence-supported switch disclosure", errors)
        self.assertIn("site missing benchmark threshold release-gate disclosure", errors)
        self.assertIn("site missing software-vs-input citation disclosure", errors)
        self.assertIn("site missing Bioconda tagged-release/sha256/lint disclosure", errors)

    def test_site_disclosure_reports_metadata_overclaim(self) -> None:
        html = """
<head>
  <meta name="description" content="command-by-command evaluation with fallback for unsupported Picard surfaces across production genomics workflows">
</head>
<section>
  <h2>Current boundaries</h2>
  <p>turbo-picard is not a full Picard suite yet.</p>
  <p>Chart outputs are lightweight PDF summaries; metrics text is the parity target.</p>
  <p>Switch only the workflow surfaces where the evidence supports a narrow change.</p>
  <p>The benchmark threshold gate is python3 tools/verify_benchmark_thresholds.py with 5.00x floor speedup, 20.00x geometric mean speedup, and 50.00x top speedup.</p>
  <p>The software citation lives in CITATION.cff. Cite the archived turbo-picard release and cite benchmark inputs separately with SHA-256.</p>
  <p>Bioconda recipes need a tagged release, python3 tools/bioconda_release_preflight.py, sha256, release-ready verifier, bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim, and mulled-test.</p>
</section>
"""

        self.assertIn(
            "site metadata contains unsupported production-genomics overclaim",
            verify_site_disclosures.validate_site_disclosures(html),
        )

    def test_site_disclosure_requires_bioconda_copy_commands(self) -> None:
        html = """
<head>
  <meta name="description" content="command-by-command evaluation with fallback for unsupported Picard surfaces">
</head>
<section>
  <h2>Current boundaries</h2>
  <p>turbo-picard is not a full Picard suite yet.</p>
  <p>Chart outputs are lightweight PDF summaries; metrics text is the parity target.</p>
  <p>Switch only the workflow surfaces where the evidence supports a narrow change.</p>
  <p>The benchmark threshold gate is python3 tools/verify_benchmark_thresholds.py with 5.00x floor speedup, 20.00x geometric mean speedup, and 50.00x top speedup.</p>
  <p>The software citation lives in CITATION.cff. Cite the archived turbo-picard release and cite benchmark inputs separately with SHA-256.</p>
  <p>Bioconda recipes need a tagged release, python3 tools/bioconda_release_preflight.py, sha256, release-ready verifier, bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim, and mulled-test.</p>
</section>
"""

        self.assertIn(
            "site missing Bioconda tagged-release/sha256/lint disclosure",
            verify_site_disclosures.validate_site_disclosures(html),
        )

    def test_site_disclosure_requires_citation_boundary(self) -> None:
        html = """
<head>
  <meta name="description" content="command-by-command evaluation with fallback for unsupported Picard surfaces">
</head>
<section>
  <h2>Current boundaries</h2>
  <p>turbo-picard is not a full Picard suite yet.</p>
  <p>Chart outputs are lightweight PDF summaries; metrics text is the parity target.</p>
  <p>Switch only the workflow surfaces where the evidence supports a narrow change.</p>
  <p>The benchmark threshold gate is python3 tools/verify_benchmark_thresholds.py with 5.00x floor speedup, 20.00x geometric mean speedup, and 50.00x top speedup.</p>
  <p>Bioconda recipes are prepared, but submission still needs a tagged release, python3 tools/bioconda_release_preflight.py, sha256, release-ready verifier, cp -R packaging/bioconda/turbo-picard recipes/turbo-picard, cp -R packaging/bioconda/turbo-picard-picard-shim recipes/turbo-picard-picard-shim, bioconda-utils lint recipes config.yml --packages turbo-picard turbo-picard-picard-shim, and mulled-test.</p>
</section>
"""

        self.assertIn(
            "site missing software-vs-input citation disclosure",
            verify_site_disclosures.validate_site_disclosures(html),
        )


if __name__ == "__main__":
    unittest.main()
