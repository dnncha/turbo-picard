#!/usr/bin/env python3
"""Tests for real-data evidence validation."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


MODULE_PATH = Path(__file__).with_name("verify_real_data_evidence.py")
SPEC = importlib.util.spec_from_file_location("verify_real_data_evidence", MODULE_PATH)
assert SPEC is not None
verify_real_data_evidence = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["verify_real_data_evidence"] = verify_real_data_evidence
SPEC.loader.exec_module(verify_real_data_evidence)


class RealDataEvidenceTests(unittest.TestCase):
    def test_validation_accepts_manifest_dataset(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.bam"
            evidence_json = root / "evidence.json"
            evidence_md = root / "evidence.md"
            input_path.write_bytes(b"bam")
            sha256 = verify_real_data_evidence.digest_file(input_path)
            source_url = "https://github.com/example/repo/blob/abc123/test/input.bam"
            command = "ViewSam"
            comparison = "SAM record digest"

            evidence_json.write_text(
                json.dumps(
                    {
                        "parity": "PASS",
                        "input": {
                            "sha256": sha256,
                            "source_url": source_url,
                            "source_commit": "abc123",
                            "size_bytes": len(b"bam"),
                        },
                        "commands": [
                            {
                                "command": command,
                                "status": "PASS",
                                "comparison": comparison,
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            evidence_md.write_text(
                "Picard: `Version:3.4.0`\n"
                f"| {command} | PASS | {comparison} | 0.001s | 0.001s | 1.00x |\n",
                encoding="utf-8",
            )
            manifest = {
                "datasets": [
                    {
                        "id": "fixture",
                        "input_path": "input.bam",
                        "evidence_json": "evidence.json",
                        "evidence_markdown": "evidence.md",
                        "source_url": source_url,
                        "source_commit": "abc123",
                        "sha256": sha256,
                        "scope_caveat": "small public fixture",
                        "release_tier": "public_smoke",
                        "expected_commands": {command: comparison},
                    }
                ]
            }
            readme = (
                f"{source_url}\nabc123\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"| {command} | PASS | {comparison} |\n"
                "python3 tools/compare_real_data.py\n"
                "python3 tools/update_real_data_manifest.py\n"
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "release_candidate\n"
                "manifest-entry.json\n"
            )
            site = (
                f"{source_url}\nabc123\n{sha256}\nevidence.md\nsmall public fixture\n{command}\n"
                "python3 tools/compare_real_data.py\n"
                "python3 tools/update_real_data_manifest.py\n"
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "release_candidate\n"
                "manifest-entry.json\n"
            )

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                self.assertEqual(
                    verify_real_data_evidence.validate_real_data_evidence(
                        manifest, readme, site
                    ),
                    [],
                )
                self.assertIn(
                    "real-data manifest has no release_candidate dataset for scientist-facing release",
                    verify_real_data_evidence.validate_real_data_evidence(
                        manifest, readme, site, release_ready=True
                    ),
                )

                manifest["datasets"][0]["release_tier"] = "release_candidate"
                manifest["datasets"][0]["minimum_input_bytes"] = 1
                manifest["datasets"][0]["expected_commands"] = {
                    "ViewSam": comparison,
                    "CleanSam": "post-command SAM record digest",
                    "CollectQualityYieldMetrics": "stable metrics digest",
                    "CollectAlignmentSummaryMetrics": "stable metrics digest",
                    "MarkDuplicates": "duplicate-marking semantic digest plus stable metrics digest",
                }
                evidence = json.loads(evidence_json.read_text(encoding="utf-8"))
                evidence["commands"].extend(
                    [
                        {
                            "command": "CleanSam",
                            "status": "PASS",
                            "comparison": "post-command SAM record digest",
                        },
                        {
                            "command": "CollectQualityYieldMetrics",
                            "status": "PASS",
                            "comparison": "stable metrics digest",
                        },
                        {
                            "command": "CollectAlignmentSummaryMetrics",
                            "status": "PASS",
                            "comparison": "stable metrics digest",
                        },
                        {
                            "command": "MarkDuplicates",
                            "status": "PASS",
                            "comparison": "duplicate-marking semantic digest plus stable metrics digest",
                        },
                    ]
                )
                evidence_json.write_text(json.dumps(evidence), encoding="utf-8")
                for command_name, command_comparison in manifest["datasets"][0][
                    "expected_commands"
                ].items():
                    row = f"| {command_name} | PASS | {command_comparison} |"
                    if row not in readme:
                        readme += row + "\n"
                    if command_name not in site:
                        site += command_name + "\n"
                    if row not in evidence_md.read_text(encoding="utf-8"):
                        with evidence_md.open("a", encoding="utf-8") as handle:
                            handle.write(row + "\n")
                self.assertEqual(
                    verify_real_data_evidence.validate_real_data_evidence(
                        manifest, readme, site, release_ready=True
                    ),
                    [],
                )

    def test_validation_rejects_unpinned_source(self) -> None:
        errors = verify_real_data_evidence.validate_manifest(
            {
                "datasets": [
                    {
                        "id": "fixture",
                        "input_path": "input.bam",
                        "evidence_json": "evidence.json",
                        "evidence_markdown": "evidence.md",
                        "source_url": "https://raw.githubusercontent.com/samtools/htslib/develop/test/range.bam",
                        "source_commit": "develop",
                        "sha256": "abc",
                        "scope_caveat": "small public fixture",
                        "release_tier": "public_smoke",
                        "expected_commands": {"ViewSam": "SAM record digest"},
                    }
                ]
            }
        )

        self.assertIn("fixture source_commit is not pinned", errors)
        self.assertIn(
            "fixture source_url must not use raw.githubusercontent.com moving branch URLs",
            errors,
        )

    def test_validation_accepts_https_accession_style_source(self) -> None:
        errors = verify_real_data_evidence.validate_manifest(
            {
                "datasets": [
                    {
                        "id": "giab-shard",
                        "input_path": "input.bam",
                        "evidence_json": "evidence.json",
                        "evidence_markdown": "evidence.md",
                        "source_url": "https://example.org/datasets/GIAB-HG001-v4.2.1/input.bam",
                        "source_commit": "GIAB-HG001-v4.2.1",
                        "sha256": "abc",
                        "scope_caveat": "representative GIAB shard",
                        "release_tier": "release_candidate",
                        "expected_commands": {"ViewSam": "SAM record digest"},
                    }
                ]
            }
        )

        self.assertEqual(errors, [])

    def test_validation_rejects_accession_source_without_identifier_in_url(self) -> None:
        errors = verify_real_data_evidence.validate_manifest(
            {
                "datasets": [
                    {
                        "id": "giab-shard",
                        "input_path": "input.bam",
                        "evidence_json": "evidence.json",
                        "evidence_markdown": "evidence.md",
                        "source_url": "https://example.org/datasets/input.bam",
                        "source_commit": "GIAB-HG001-v4.2.1",
                        "sha256": "abc",
                        "scope_caveat": "representative GIAB shard",
                        "release_tier": "release_candidate",
                        "expected_commands": {"ViewSam": "SAM record digest"},
                    }
                ]
            }
        )

        self.assertIn(
            "giab-shard non-GitHub source_url must include source_commit/accession identifier",
            errors,
        )

    def test_validation_rejects_github_url_without_matching_blob_commit(self) -> None:
        errors = verify_real_data_evidence.validate_manifest(
            {
                "datasets": [
                    {
                        "id": "github-fixture",
                        "input_path": "input.bam",
                        "evidence_json": "evidence.json",
                        "evidence_markdown": "evidence.md",
                        "source_url": "https://github.com/example/repo/blob/other/input.bam",
                        "source_commit": "abc123",
                        "sha256": "abc",
                        "scope_caveat": "fixture",
                        "release_tier": "public_smoke",
                        "expected_commands": {"ViewSam": "SAM record digest"},
                    }
                ]
            }
        )

        self.assertIn(
            "github-fixture GitHub source_url must include /blob/abc123/",
            errors,
        )

    def test_validation_requires_workflow_documentation(self) -> None:
        errors = verify_real_data_evidence.validate_workflow_docs(
            "python3 tools/verify_real_data_evidence.py",
            "python3 tools/verify_real_data_evidence.py",
        )

        self.assertIn(
            "benchmarks README missing manifest update command",
            errors,
        )
        self.assertIn(
            "site missing release-ready real-data verifier command",
            errors,
        )

    def test_release_candidate_requires_broad_commands_and_size(self) -> None:
        dataset = {
            "id": "tiny-release",
            "release_tier": "release_candidate",
            "expected_commands": {"ViewSam": "SAM record digest"},
        }
        errors = verify_real_data_evidence.validate_release_candidate_dataset(
            dataset,
            {"size_bytes": 10},
        )

        self.assertIn(
            "tiny-release release_candidate missing required commands: CleanSam, CollectAlignmentSummaryMetrics, CollectQualityYieldMetrics, MarkDuplicates",
            errors,
        )
        self.assertIn(
            "tiny-release release_candidate input too small: 10 bytes < 1000000",
            errors,
        )

    def test_release_candidate_minimum_size_can_be_manifest_explicit(self) -> None:
        dataset = {
            "id": "reviewed-small-release",
            "release_tier": "release_candidate",
            "minimum_input_bytes": 10,
            "expected_commands": {
                "ViewSam": "SAM record digest",
                "CleanSam": "post-command SAM record digest",
                "CollectQualityYieldMetrics": "stable metrics digest",
                "CollectAlignmentSummaryMetrics": "stable metrics digest",
                "MarkDuplicates": "duplicate-marking semantic digest plus stable metrics digest",
            },
        }

        self.assertEqual(
            verify_real_data_evidence.validate_release_candidate_dataset(
                dataset,
                {"size_bytes": 10},
            ),
            [],
        )


if __name__ == "__main__":
    unittest.main()
