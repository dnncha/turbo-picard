#!/usr/bin/env python3
"""Tests for command matrix consistency checks."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify_command_matrix.py")
SPEC = importlib.util.spec_from_file_location("verify_command_matrix", MODULE_PATH)
assert SPEC is not None
verify_command_matrix = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_command_matrix"] = verify_command_matrix
SPEC.loader.exec_module(verify_command_matrix)


class CommandMatrixTests(unittest.TestCase):
    def test_scope_validation_accepts_complete_entries(self) -> None:
        errors = verify_command_matrix.validate_scope_notes(
            [
                {
                    "name": "SortSam",
                    "native_scope": "Coordinate and queryname sorting for SAM/BAM.",
                    "fallback_scope": "Unsupported sort orders should use upstream Picard.",
                }
            ]
        )

        self.assertEqual(errors, [])

    def test_scope_validation_reports_missing_and_vague_entries(self) -> None:
        errors = verify_command_matrix.validate_scope_notes(
            [
                {
                    "name": "MarkDuplicates",
                    "native_scope": "",
                    "fallback_scope": "TBD",
                }
            ]
        )

        self.assertIn("MarkDuplicates missing native_scope", errors)
        self.assertIn("MarkDuplicates has vague fallback_scope: TBD", errors)

    def test_matrix_entry_structure_reports_duplicates_and_invalid_statuses(self) -> None:
        errors = verify_command_matrix.validate_matrix_entry_structure(
            [
                {
                    "name": "SortSam",
                    "status": "partial-native",
                    "parity_script": "tools/verify_basic_sortsam_parity.sh",
                },
                {
                    "name": "SortSam",
                    "status": "native-ish",
                    "parity_script": "tools/verify_basic_sortsam_parity.sh",
                },
                {
                    "name": "FutureCommand",
                    "status": "native",
                },
                {
                    "name": "FallbackOnlyCommand",
                    "status": "fallback-only",
                    "parity_script": "tools/should-not-run.sh",
                },
            ]
        )

        self.assertIn("command matrix has duplicate command entry: SortSam", errors)
        self.assertIn("SortSam has invalid status: native-ish", errors)
        self.assertIn("FutureCommand missing parity_script", errors)
        self.assertIn(
            "FallbackOnlyCommand fallback-only entry should not declare parity_script",
            errors,
        )

    def test_matrix_entry_structure_accepts_valid_native_and_fallback_entries(self) -> None:
        errors = verify_command_matrix.validate_matrix_entry_structure(
            [
                {
                    "name": "ViewSam",
                    "status": "partial-native",
                    "parity_script": "tools/verify_basic_viewsam_parity.sh",
                },
                {
                    "name": "UnsupportedCommand",
                    "status": "fallback-only",
                    "parity_script": "null",
                },
            ]
        )

        self.assertEqual(errors, [])

    def test_scope_validation_allows_explicit_lightweight_chart_disclosure(self) -> None:
        errors = verify_command_matrix.validate_scope_notes(
            [
                {
                    "name": "QualityScoreDistribution",
                    "native_scope": "Quality histogram metrics and lightweight PDF chart artifact.",
                    "fallback_scope": "Rendered plots should use upstream Picard.",
                }
            ]
        )

        self.assertEqual(errors, [])

    def test_picard_reference_must_match_evidence_version(self) -> None:
        self.assertEqual(
            verify_command_matrix.matrix_picard_reference('picard_reference: "3.4.0"\ncommands: []\n'),
            "3.4.0",
        )
        self.assertEqual(
            verify_command_matrix.validate_picard_reference("3.4.0"),
            [],
        )
        self.assertEqual(
            verify_command_matrix.validate_picard_reference(None),
            ["command matrix missing picard_reference"],
        )
        self.assertEqual(
            verify_command_matrix.validate_picard_reference("3.4.x"),
            ["command matrix picard_reference must be 3.4.0, got 3.4.x"],
        )

    def test_real_data_scope_claims_require_release_candidate_manifest_evidence(self) -> None:
        entries = [
            {
                "name": "CollectInsertSizeMetrics",
                "native_scope": "Insert-size metrics with GATK NA12878 mitochondrial real-data parity.",
            },
            {
                "name": "SortSam",
                "native_scope": "Coordinate and queryname sorting for SAM/BAM.",
            },
        ]

        self.assertEqual(
            verify_command_matrix.validate_real_data_scope_claims(
                entries,
                {"CollectInsertSizeMetrics"},
            ),
            [],
        )
        self.assertEqual(
            verify_command_matrix.validate_real_data_scope_claims(entries, {"SortSam"}),
            [
                "CollectInsertSizeMetrics command matrix claims real-data parity but has no release_candidate manifest evidence"
            ],
        )

    def test_release_candidate_manifest_evidence_requires_matrix_scope_mention(self) -> None:
        entries = [
            {
                "name": "MarkDuplicates",
                "native_scope": "Duplicate marking with release-candidate real-data parity.",
            },
            {
                "name": "SortSam",
                "native_scope": "Coordinate and queryname sorting for SAM/BAM.",
            },
        ]

        self.assertEqual(
            verify_command_matrix.validate_release_candidate_scope_mentions(
                entries,
                {"MarkDuplicates"},
            ),
            [],
        )
        self.assertEqual(
            verify_command_matrix.validate_release_candidate_scope_mentions(
                entries,
                {"SortSam", "ViewSam"},
            ),
            [
                "SortSam has release_candidate manifest evidence but command matrix native_scope does not mention real-data parity",
                "ViewSam has release_candidate manifest evidence but is missing from command matrix",
            ],
        )

    def test_release_candidate_real_data_commands_reads_manifest(self) -> None:
        manifest_text = """
{
  "datasets": [
    {
      "release_tier": "public_smoke",
      "expected_commands": {"ViewSam": "SAM record digest"}
    },
    {
      "release_tier": "release_candidate",
      "expected_commands": {
        "MarkDuplicates": "duplicate-marking semantic digest plus stable metrics digest",
        "CollectInsertSizeMetrics": "stable metrics digest with insert-size histogram"
      }
    }
  ]
}
"""

        self.assertEqual(
            verify_command_matrix.release_candidate_real_data_commands(manifest_text),
            {"MarkDuplicates", "CollectInsertSizeMetrics"},
        )
        self.assertEqual(
            verify_command_matrix.release_candidate_real_data_commands("not json"),
            set(),
        )

    def test_parity_script_file_validation_reports_missing_scripts(self) -> None:
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            script = root / "tools" / "present.sh"
            script.parent.mkdir()
            script.write_text("#!/usr/bin/env sh\n", encoding="utf-8")

            self.assertEqual(
                verify_command_matrix.validate_parity_script_files(
                    ["tools/present.sh"],
                    root,
                ),
                [],
            )
            self.assertEqual(
                verify_command_matrix.validate_parity_script_files(
                    ["tools/present.sh", "tools/missing.sh"],
                    root,
                ),
                [
                    "matrix parity scripts missing from repository: tools/missing.sh"
                ],
            )

    def test_parity_script_ci_validation_reports_missing_references(self) -> None:
        self.assertEqual(
            verify_command_matrix.validate_parity_script_ci_coverage(
                ["tools/a.sh", "tools/b.sh"],
                "tools/a.sh\n",
            ),
            ["matrix parity scripts missing from CI: tools/b.sh"],
        )
        self.assertEqual(
            verify_command_matrix.validate_parity_script_ci_coverage(
                ["tools/a.sh"],
                "run tools/a.sh here\n",
            ),
            [],
        )

    def test_command_docs_scope_language_rejects_mixed_examples_as_native(self) -> None:
        self.assertEqual(
            verify_command_matrix.validate_command_docs_scope_language(
                "Common command examples\n"
                "These examples include partial-native surfaces. Check the "
                "machine-readable matrix before treating any command as fully native.\n"
            ),
            [],
        )
        errors = verify_command_matrix.validate_command_docs_scope_language(
            "Common native command examples\n"
        )

        self.assertIn(
            "commands docs must not label mixed native/partial-native examples as native",
            errors,
        )
        self.assertIn("commands docs missing partial-native scope wording", errors)
        self.assertIn("commands docs missing matrix pointer", errors)
        self.assertIn("commands docs missing fully native caution", errors)

    def test_command_docs_examples_track_matrix_commands(self) -> None:
        entries = [
            {"name": "MarkDuplicates", "status": "partial-native"},
            {"name": "SortSam", "status": "native"},
            {"name": "UnsupportedFutureCommand", "status": "fallback-only"},
        ]

        self.assertEqual(
            verify_command_matrix.validate_command_docs_examples(
                entries,
                "picard MarkDuplicates I=input.bam\npicard SortSam I=input.bam\n",
            ),
            [],
        )
        self.assertEqual(
            verify_command_matrix.validate_command_docs_examples(
                entries,
                "picard MarkDuplicates I=input.bam\n",
            ),
            ["commands docs missing example for matrix command: SortSam"],
        )

    def test_command_docs_status_summary_tracks_matrix_statuses(self) -> None:
        entries = [
            {"name": "MarkDuplicates", "status": "partial-native"},
            {"name": "SortSam", "status": "native"},
            {"name": "FutureCommand", "status": "fallback-only"},
        ]

        self.assertEqual(
            verify_command_matrix.validate_command_docs_status_summary(
                entries,
                "* ``MarkDuplicates``: ``partial-native``\n"
                "* ``SortSam``: ``native``\n"
                "* ``FutureCommand``: ``fallback-only``\n",
            ),
            [],
        )
        self.assertEqual(
            verify_command_matrix.validate_command_docs_status_summary(
                entries,
                "* ``MarkDuplicates``: ``native``\n"
                "* ``SortSam``: ``native``\n",
            ),
            [
                "commands docs missing matrix status summary for MarkDuplicates: partial-native",
                "commands docs missing matrix status summary for FutureCommand: fallback-only",
            ],
        )


if __name__ == "__main__":
    unittest.main()
