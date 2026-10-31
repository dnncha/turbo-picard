#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("verify_parity_docs.py")
SPEC = importlib.util.spec_from_file_location("verify_parity_docs", MODULE_PATH)
assert SPEC is not None
verify_parity_docs = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_parity_docs"] = verify_parity_docs
SPEC.loader.exec_module(verify_parity_docs)


class ParityDocsTests(unittest.TestCase):
    def test_accepts_complete_parity_docs(self) -> None:
        parity = """
        What Parity Means
        A specific command with a specific input shape names a comparison method.
        It does not mean every Picard behavior has been reimplemented.
        Passing parity evidence does not prove broad switching safety.
        Use representative inputs, input SHA-256, Picard version, turbo-picard version,
        tools/compare_real_data.py, verify_basic_cram_parity, verify_markdup_cram_parity,
        verify_gatk_preprocessing_combo_parity,
        verify_gatk_mito_bam_parity,
        verify_gatk_mito_cram_parity,
        verify_gatk_preprocessing_combo_cram_parity,
        bootstrap_gatk_mito_cram_evidence,
        python3 tools/verify_real_data_evidence.py --release-ready,
        and fallback to upstream Picard where needed.
        MarkDuplicates SortSam BuildBamIndex SamToFastq ValidateSamFile FixMateInformation
        SetNmMdAndUqTags metrics commands.
        """
        errors = verify_parity_docs.validate_parity_docs(
            parity,
            "\n   quickstart\n   parity\n   fallback\n",
            "[What parity means](https://turbo-picard.readthedocs.io/en/latest/parity.html)",
            '<a href="https://turbo-picard.readthedocs.io/en/latest/parity.html">What parity means</a>',
            "The comparison boundary is described in :doc:`parity`.",
            "Fallback is a compatibility bridge, not proof of readiness. See :doc:`parity` so unsupported surfaces remain visible.",
        )

        assert errors == []

    def test_reports_missing_required_content(self) -> None:
        errors = verify_parity_docs.validate_parity_docs(
            "This is fast.",
            "\n   quickstart\n   fallback\n",
            "# README",
            "<html></html>",
            "Adopt it.",
            "Fallback is " + "ma" + "gic.",
        )

        assert "parity docs missing command-specific parity scope" in errors
        assert "parity docs missing broad switching caveat" in errors
        assert "parity docs missing comparison boundary for markduplicates" in errors
        assert "docs index missing parity page in user-guide toctree" in errors
        assert "README missing parity guide link" in errors
        assert "site missing parity guide link" in errors
        assert "adoption docs missing parity page cross-reference" in errors
        assert "fallback docs missing fallback compatibility-bridge wording" in errors
        assert "fallback docs missing fallback not-proof caveat" in errors
        assert "fallback docs missing fallback parity cross-reference" in errors

    def test_rejects_overclaims(self) -> None:
        errors = verify_parity_docs.validate_parity_docs(
            "This is a drop-in replacement for production genomics workflows.",
            "\n   parity\n",
            "parity.html What parity means",
            "parity.html What parity means",
            ":doc:`parity` comparison boundary",
            "compatibility bridge not proof :doc:`parity` unsupported surfaces remain visible",
        )

        assert "parity docs contain unsupported overclaim: drop-in replacement" in errors
        assert "parity docs contain unsupported overclaim: production genomics workflows" in errors


if __name__ == "__main__":
    unittest.main()
