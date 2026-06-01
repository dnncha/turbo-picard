#!/usr/bin/env python3
"""Tests for release-critical CI coverage verifier."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_real_data_ci_coverage.py")
SPEC = importlib.util.spec_from_file_location("verify_real_data_ci_coverage", MODULE_PATH)
assert SPEC is not None
verify_real_data_ci_coverage = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_real_data_ci_coverage"] = verify_real_data_ci_coverage
SPEC.loader.exec_module(verify_real_data_ci_coverage)


class RealDataCiCoverageTests(unittest.TestCase):
    def test_accepts_all_required_snippets(self) -> None:
        ci_text = "\n".join(verify_real_data_ci_coverage.REQUIRED_SNIPPETS)

        self.assertEqual(verify_real_data_ci_coverage.validate_ci_coverage(ci_text), [])

    def test_reports_missing_snippet(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "tools/update_real_data_manifest.py"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: tools/update_real_data_manifest.py",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_release_prep_coverage(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "tools/prepare_bioconda_release.py \\"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: tools/prepare_bioconda_release.py \\",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_release_prep_dry_run(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "python3 tools/prepare_bioconda_release.py --sha256"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: python3 tools/prepare_bioconda_release.py --sha256",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_release_prep_dry_run_exit_assertion(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != 'test "$release_helper_check_status" -eq 1'
        )

        self.assertIn(
            'CI missing release-critical helper coverage: test "$release_helper_check_status" -eq 1',
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_package_install_smoke(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "./tools/verify_package_install.sh"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: ./tools/verify_package_install.sh",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_bioconda_recipe_verifier(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "python3 tools/verify_bioconda_recipes.py"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: python3 tools/verify_bioconda_recipes.py",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_benchmark_threshold_verifier(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "python3 tools/verify_benchmark_thresholds.py"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: python3 tools/verify_benchmark_thresholds.py",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_benchmark_threshold_verifier_tests(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "python3 -m unittest tools/test_verify_benchmark_thresholds.py"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: python3 -m unittest tools/test_verify_benchmark_thresholds.py",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_readme_benchmark_evidence_verifier(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "python3 tools/verify_readme_benchmark_evidence.py"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: python3 tools/verify_readme_benchmark_evidence.py",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_site_benchmark_evidence_verifier(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "python3 tools/verify_site_benchmark_evidence.py"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: python3 tools/verify_site_benchmark_evidence.py",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_site_benchmark_evidence_verifier_tests(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "python3 -m unittest tools/test_verify_site_benchmark_evidence.py"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: python3 -m unittest tools/test_verify_site_benchmark_evidence.py",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_bioconda_recipe_verifier_tests(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "python3 -m unittest tools/test_verify_bioconda_recipes.py"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: python3 -m unittest tools/test_verify_bioconda_recipes.py",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_site_link_verifier(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "python3 tools/verify_site_links.py"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: python3 tools/verify_site_links.py",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_site_link_verifier_tests(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "python3 -m unittest tools/test_verify_site_links.py"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: python3 -m unittest tools/test_verify_site_links.py",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_site_disclosure_verifier(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "python3 tools/verify_site_disclosures.py"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: python3 tools/verify_site_disclosures.py",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_reports_missing_validatesamfile_parity_script(self) -> None:
        ci_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.REQUIRED_SNIPPETS
            if snippet != "./tools/verify_basic_validatesamfile_parity.sh"
        )

        self.assertIn(
            "CI missing release-critical helper coverage: ./tools/verify_basic_validatesamfile_parity.sh",
            verify_real_data_ci_coverage.validate_ci_coverage(ci_text),
        )

    def test_accepts_package_install_release_smoke_coverage(self) -> None:
        package_install_text = "\n".join(
            verify_real_data_ci_coverage.PACKAGE_INSTALL_SNIPPETS
        )

        self.assertEqual(
            verify_real_data_ci_coverage.validate_package_install_coverage(
                package_install_text
            ),
            [],
        )

    def test_reports_missing_package_install_recipe_smoke(self) -> None:
        package_install_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.PACKAGE_INSTALL_SNIPPETS
            if snippet != "packaging/bioconda/turbo-picard-picard-shim/run_test.sh"
        )

        self.assertIn(
            "package install smoke missing release-critical behavior: packaging/bioconda/turbo-picard-picard-shim/run_test.sh",
            verify_real_data_ci_coverage.validate_package_install_coverage(
                package_install_text
            ),
        )

    def test_reports_missing_package_install_citation_metadata_smoke(self) -> None:
        package_install_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.PACKAGE_INSTALL_SNIPPETS
            if snippet != "^cff-version: 1.2.0$"
        )

        self.assertIn(
            "package install smoke missing release-critical behavior: ^cff-version: 1.2.0$",
            verify_real_data_ci_coverage.validate_package_install_coverage(
                package_install_text
            ),
        )

    def test_reports_missing_package_install_citation_software_type_smoke(self) -> None:
        package_install_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.PACKAGE_INSTALL_SNIPPETS
            if snippet != "^type: software$"
        )

        self.assertIn(
            "package install smoke missing release-critical behavior: ^type: software$",
            verify_real_data_ci_coverage.validate_package_install_coverage(
                package_install_text
            ),
        )

    def test_reports_missing_package_install_real_data_release_candidate_smoke(self) -> None:
        package_install_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.PACKAGE_INSTALL_SNIPPETS
            if snippet != '"release_tier": "release_candidate"'
        )

        self.assertIn(
            'package install smoke missing release-critical behavior: "release_tier": "release_candidate"',
            verify_real_data_ci_coverage.validate_package_install_coverage(
                package_install_text
            ),
        )

    def test_reports_missing_package_install_benchmark_parity_smoke(self) -> None:
        package_install_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.PACKAGE_INSTALL_SNIPPETS
            if snippet != '"parity": "32/32 PASS"'
        )

        self.assertIn(
            'package install smoke missing release-critical behavior: "parity": "32/32 PASS"',
            verify_real_data_ci_coverage.validate_package_install_coverage(
                package_install_text
            ),
        )

    def test_reports_missing_package_install_structured_evidence_smoke(self) -> None:
        package_install_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.PACKAGE_INSTALL_SNIPPETS
            if snippet != "release_candidate manifest missing package-smoke command evidence"
        )

        self.assertIn(
            "package install smoke missing release-critical behavior: "
            "release_candidate manifest missing package-smoke command evidence",
            verify_real_data_ci_coverage.validate_package_install_coverage(
                package_install_text
            ),
        )

    def test_reports_missing_package_install_shim_data_smoke(self) -> None:
        package_install_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.PACKAGE_INSTALL_SNIPPETS
            if snippet != "picard MarkDuplicates \\"
        )

        self.assertIn(
            "package install smoke missing release-critical behavior: picard MarkDuplicates \\",
            verify_real_data_ci_coverage.validate_package_install_coverage(
                package_install_text
            ),
        )

    def test_reports_missing_installed_help_matrix_smoke(self) -> None:
        package_install_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.PACKAGE_INSTALL_SNIPPETS
            if snippet != "installed turbo-picard help missing commands"
        )

        self.assertIn(
            "package install smoke missing release-critical behavior: installed turbo-picard help missing commands",
            verify_real_data_ci_coverage.validate_package_install_coverage(
                package_install_text
            ),
        )

    def test_reports_missing_installed_shim_help_matrix_smoke(self) -> None:
        package_install_text = "\n".join(
            snippet
            for snippet in verify_real_data_ci_coverage.PACKAGE_INSTALL_SNIPPETS
            if snippet != "installed picard shim help missing commands"
        )

        self.assertIn(
            "package install smoke missing release-critical behavior: installed picard shim help missing commands",
            verify_real_data_ci_coverage.validate_package_install_coverage(
                package_install_text
            ),
        )

    def test_python_tool_compile_coverage_tracks_all_tool_scripts(self) -> None:
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            tools_dir = Path(tmp) / "tools"
            tools_dir.mkdir()
            (tools_dir / "kept.py").write_text("print('ok')\n", encoding="utf-8")
            (tools_dir / "missing.py").write_text("print('missing')\n", encoding="utf-8")

            errors = verify_real_data_ci_coverage.validate_python_tool_compile_coverage(
                "tools/kept.py\n",
                tools_dir,
            )

        self.assertEqual(errors, ["CI missing Python compile coverage: tools/missing.py"])

    def test_command_matrix_parity_scripts_are_parsed(self) -> None:
        matrix_text = """
commands:
  - name: MarkDuplicates
    status: partial-native
    parity_script: tools/verify_basic_picard_parity.sh
  - name: ViewSam
    status: native
    parity_script: tools/verify_basic_viewsam_parity.sh
"""

        self.assertEqual(
            verify_real_data_ci_coverage.command_matrix_parity_scripts(matrix_text),
            [
                "tools/verify_basic_picard_parity.sh",
                "tools/verify_basic_viewsam_parity.sh",
            ],
        )

    def test_reports_command_matrix_parity_scripts_missing_from_ci(self) -> None:
        matrix_text = """
commands:
  - name: MarkDuplicates
    status: partial-native
    parity_script: tools/verify_basic_picard_parity.sh
  - name: ViewSam
    status: native
    parity_script: tools/verify_basic_viewsam_parity.sh
"""

        self.assertEqual(
            verify_real_data_ci_coverage.validate_parity_script_ci_coverage(
                'TURBO_PICARD_CONDA_PREFIX="env" ./tools/verify_basic_picard_parity.sh\n',
                matrix_text,
            ),
            [
                "CI missing command-matrix parity script: ./tools/verify_basic_viewsam_parity.sh"
            ],
        )


if __name__ == "__main__":
    unittest.main()
