#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import sys
import tempfile
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

        assert errors == []

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

        assert "MarkDuplicates missing native_scope" in errors
        assert "MarkDuplicates has vague fallback_scope: TBD" in errors

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

        assert "command matrix has duplicate command entry: SortSam" in errors
        assert "SortSam has invalid status: native-ish" in errors
        assert "FutureCommand missing parity_script" in errors
        assert "FallbackOnlyCommand fallback-only entry should not declare parity_script" in errors

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

        assert errors == []

    def test_command_docs_examples_reject_upstream_viewsam_output_argument(self) -> None:
        errors = verify_command_matrix.validate_command_docs_examples(
            [],
            "picard ViewSam I=input.bam O=view.sam\n",
        )

        assert "commands docs must show upstream ViewSam writing to stdout, not O= output" in errors

    def test_command_docs_examples_accept_upstream_viewsam_stdout_redirect(self) -> None:
        errors = verify_command_matrix.validate_command_docs_examples(
            [],
            "picard ViewSam I=input.bam > view.sam\n",
        )

        assert errors == []

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

        assert errors == []

    def test_picard_reference_must_match_evidence_version(self) -> None:
        assert verify_command_matrix.matrix_picard_reference('picard_reference: "3.4.0"\ncommands: []\n') == "3.4.0"
        assert verify_command_matrix.validate_picard_reference("3.4.0") == []
        assert verify_command_matrix.validate_picard_reference(None) == ["command matrix missing picard_reference"]
        assert verify_command_matrix.validate_picard_reference("3.4.x") == [
            "command matrix picard_reference must be 3.4.0, got 3.4.x"
        ]

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

        assert verify_command_matrix.validate_real_data_scope_claims(entries, {"CollectInsertSizeMetrics"}) == []
        assert verify_command_matrix.validate_real_data_scope_claims(entries, {"SortSam"}) == [
            "CollectInsertSizeMetrics command matrix claims real-data parity but has no release_candidate manifest evidence"
        ]

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

        assert verify_command_matrix.validate_release_candidate_scope_mentions(entries, {"MarkDuplicates"}) == []
        assert verify_command_matrix.validate_release_candidate_scope_mentions(entries, {"SortSam", "ViewSam"}) == [
            "SortSam has release_candidate manifest evidence but command matrix native_scope does not mention real-data parity",
            "ViewSam has release_candidate manifest evidence but is missing from command matrix",
        ]

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

        assert verify_command_matrix.release_candidate_real_data_commands(manifest_text) == {
            "MarkDuplicates",
            "CollectInsertSizeMetrics",
        }
        assert verify_command_matrix.release_candidate_real_data_commands("not json") == set()

    def test_cram_parity_commands_reads_script_summary(self) -> None:
        script_text = 'echo "CRAM hot-path parity passed for: SortSam, ViewSam, CleanSam"\n'

        assert verify_command_matrix.cram_parity_commands(script_text) == {"SortSam", "ViewSam", "CleanSam"}
        assert verify_command_matrix.cram_parity_commands("no summary") == set()

    def test_cram_scope_mentions_require_matrix_cram_wording(self) -> None:
        entries = [
            {
                "name": "SortSam",
                "native_scope": "Coordinate and queryname sorting for SAM/BAM/CRAM.",
            },
            {
                "name": "CleanSam",
                "native_scope": "Common SAM/BAM cleanup.",
            },
        ]

        assert verify_command_matrix.validate_cram_scope_mentions(entries, {"SortSam"}) == []
        assert verify_command_matrix.validate_cram_scope_mentions(entries, {"CleanSam", "ViewSam"}) == [
            "CleanSam has CRAM parity coverage but command matrix native_scope does not mention CRAM",
            "ViewSam has CRAM parity coverage but is missing from command matrix",
        ]

    def test_parity_script_file_validation_reports_missing_scripts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            script = root / "tools" / "present.sh"
            script.parent.mkdir()
            script.write_text("#!/usr/bin/env sh\nset -eu\n", encoding="utf-8")

            assert verify_command_matrix.validate_parity_script_files(["tools/present.sh"], root) == []
            assert verify_command_matrix.validate_parity_script_files(
                ["tools/present.sh", "tools/missing.sh"], root
            ) == ["matrix parity scripts missing from repository: tools/missing.sh"]

    def test_parity_script_file_validation_requires_errexit(self) -> None:
        accepted = [
            "set -euo pipefail\n",
            "set -e\n",
            "  set -o errexit -o nounset\n",
            "set -u -e; run\n",
        ]
        rejected = [
            "set -u\n",
            "set -x\n",
            "set -o pipefail\n",
            "# set -e\n",
            "set +e\n",
            "echo set -e\n",
            "set -u # -e\n",
            "",
        ]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            script = root / "tools" / "parity.sh"
            script.parent.mkdir()
            for body in accepted:
                script.write_text("#!/usr/bin/env bash\n" + body, encoding="utf-8")
                assert verify_command_matrix.validate_parity_script_files(["tools/parity.sh"], root) == [], body
            for body in rejected:
                script.write_text("#!/usr/bin/env bash\n" + body, encoding="utf-8")
                assert verify_command_matrix.validate_parity_script_files(["tools/parity.sh"], root) == [
                    "matrix parity script does not fail closed: tools/parity.sh"
                ], body

    def test_parity_gate_must_fail_closed_and_run_every_matrix_script(self) -> None:
        gate = "#!/usr/bin/env bash\nset -euo pipefail\n./tools/a.sh\nENV=1 ./tools/c.sh\n"
        assert verify_command_matrix.validate_parity_gate_coverage(["tools/a.sh", "tools/b.sh"], gate) == [
            "matrix parity scripts missing from tools/parity_gate.sh: tools/b.sh"
        ]
        assert verify_command_matrix.validate_parity_gate_coverage(["tools/a.sh", "tools/c.sh"], gate) == []
        assert verify_command_matrix.validate_parity_gate_coverage(
            ["tools/a.sh"], "echo ./tools/a.sh is skipped\n"
        ) == [
            "tools/parity_gate.sh does not fail closed",
            "matrix parity scripts missing from tools/parity_gate.sh: tools/a.sh",
        ]

    def test_repository_parity_gate_runs_every_matrix_script(self) -> None:
        gate = verify_command_matrix.PARITY_GATE.read_text(encoding="utf-8")
        assert (
            verify_command_matrix.validate_parity_gate_coverage(verify_command_matrix.matrix_parity_scripts(), gate)
            == []
        )

    def test_command_docs_scope_language_requires_full_picard_surface_wording(self) -> None:
        assert (
            verify_command_matrix.validate_command_docs_scope_language(
                "Picard 3.4.0 command surface\n"
                "turbo-picard --list-commands\n"
                "machine-readable matrix\n"
                "33 accelerated commands\n"
                "89 delegated commands\n"
            )
            == []
        )
        errors = verify_command_matrix.validate_command_docs_scope_language("Common native command examples\n")

        assert "commands docs must not label mixed native/partial-native examples as native" in errors
        assert "commands docs missing Picard 3.4.0 compatibility wording" in errors
        assert "commands docs missing list-commands pointer" in errors
        assert "commands docs missing matrix pointer" in errors
        assert "commands docs missing accelerated command wording" in errors
        assert "commands docs missing delegated command wording" in errors

    def test_command_docs_examples_track_matrix_commands(self) -> None:
        entries = [
            {"name": "MarkDuplicates", "status": "partial-native"},
            {"name": "SortSam", "status": "native"},
            {"name": "UnsupportedFutureCommand", "status": "fallback-only"},
        ]

        assert (
            verify_command_matrix.validate_command_docs_examples(
                entries, "picard MarkDuplicates I=input.bam\npicard SortSam I=input.bam\n"
            )
            == []
        )
        assert verify_command_matrix.validate_command_docs_examples(entries, "picard MarkDuplicates I=input.bam\n") == [
            "commands docs missing example for matrix command: SortSam"
        ]

    def test_command_docs_status_summary_tracks_matrix_statuses(self) -> None:
        entries = [
            {"name": "MarkDuplicates", "status": "partial-native"},
            {"name": "SortSam", "status": "native"},
            {"name": "FutureCommand", "status": "fallback-only"},
        ]

        assert (
            verify_command_matrix.validate_command_docs_status_summary(
                entries,
                "* ``MarkDuplicates``: ``partial-native``\n"
                "* ``SortSam``: ``native``\n"
                "2 accelerated commands\n"
                "1 delegated commands\n",
            )
            == []
        )
        assert verify_command_matrix.validate_command_docs_status_summary(
            entries, "* ``MarkDuplicates``: ``native``\n* ``SortSam``: ``native``\n"
        ) == [
            "commands docs missing matrix status summary for MarkDuplicates: partial-native",
            "commands docs missing accelerated command count summary",
            "commands docs missing delegated command count summary",
        ]


if __name__ == "__main__":
    unittest.main()
