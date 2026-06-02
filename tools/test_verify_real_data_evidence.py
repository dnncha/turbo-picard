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
            dataset_root = root / "benchmarks" / "real-data" / "fixture"
            evidence_root = dataset_root / "evidence"
            evidence_root.mkdir(parents=True)
            input_path = dataset_root / "input.bam"
            evidence_json = evidence_root / "real-data-comparison.json"
            evidence_md = evidence_root / "real-data-comparison.md"
            turbo_artifact = evidence_root / "turbo.out"
            picard_artifact = evidence_root / "picard.out"
            input_path.write_bytes(b"bam")
            turbo_artifact.write_text("same\n", encoding="utf-8")
            picard_artifact.write_text("same\n", encoding="utf-8")
            markduplicates_sam = (
                "@HD\tVN:1.6\n"
                "read-a\t1024\tchr1\t1\t60\t4M\t=\t10\t100\tACGT\tFFFF\t"
                "DT:Z:LB\tDS:i:2\tDI:i:1\tRX:Z:AAAA\n"
            )
            (evidence_root / "turbo.view.sam").write_text(
                markduplicates_sam,
                encoding="utf-8",
            )
            (evidence_root / "picard.view.sam").write_text(
                markduplicates_sam,
                encoding="utf-8",
            )
            (evidence_root / "turbo.metrics.txt").write_text(
                "# generated\nLIBRARY\tUNPAIRED_READ_DUPLICATES\nunknown\t1\n",
                encoding="utf-8",
            )
            (evidence_root / "picard.metrics.txt").write_text(
                "# generated\nLIBRARY\tUNPAIRED_READ_DUPLICATES\nunknown\t1\n",
                encoding="utf-8",
            )
            markduplicates_digest = verify_real_data_evidence.recomputable_artifact_digest(
                turbo_artifact,
                "duplicate-marking semantic digest plus stable metrics digest",
            )
            assert markduplicates_digest is not None
            sha256 = verify_real_data_evidence.digest_file(input_path)
            source_commit = "0123456789abcdef0123456789abcdef01234567"
            source_url = (
                "https://github.com/example/repo/blob/"
                f"{source_commit}/test/input.bam"
            )
            command = "ViewSam"
            comparison = "SAM record digest"

            evidence_json.write_text(
                json.dumps(
                    {
                        "parity": "PASS",
                        "picard_version": "Version:3.4.0",
                        "turbo_picard_version": "picard 0.1.0",
                        "input": {
                            "path": "benchmarks/real-data/fixture/input.bam",
                            "sha256": sha256,
                            "source_url": source_url,
                            "source_commit": source_commit,
                            "size_bytes": len(b"bam"),
                        },
                        "commands": [
                            {
                                "command": command,
                                "status": "PASS",
                                "comparison": comparison,
                                "turbo_seconds": 0.001,
                                "picard_seconds": 0.001,
                                "speedup": 1.0,
                                "turbo_artifact": "benchmarks/real-data/fixture/evidence/turbo.out",
                                "picard_artifact": "benchmarks/real-data/fixture/evidence/picard.out",
                                "turbo_digest": "abc123",
                                "picard_digest": "abc123",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            evidence_md.write_text(
                "Picard: `Version:3.4.0`\n"
                "Input BAM: `benchmarks/real-data/fixture/input.bam`\n"
                f"| {command} | PASS | {comparison} | 0.001s | 0.001s | 1.00x |\n"
                "## Comparison details\n"
                "ignores headers\n"
                "## Artifact digests\n"
                "benchmarks/real-data/fixture/evidence/turbo.out\n"
                "benchmarks/real-data/fixture/evidence/picard.out\n",
                encoding="utf-8",
            )
            manifest = {
                "datasets": [
                    {
                        "id": "fixture",
                        "input_path": "benchmarks/real-data/fixture/input.bam",
                        "evidence_json": "benchmarks/real-data/fixture/evidence/real-data-comparison.json",
                        "evidence_markdown": "benchmarks/real-data/fixture/evidence/real-data-comparison.md",
                        "source_url": source_url,
                        "source_commit": source_commit,
                        "sha256": sha256,
                        "scope_caveat": "small public fixture",
                        "release_tier": "public_smoke",
                        "expected_commands": {command: comparison},
                    }
                ]
            }
            portfolio_text = (
                verify_real_data_evidence.RELEASE_CANDIDATE_PORTFOLIO_COMMAND_TEXT
            )
            readme = (
                f"{source_url}\n{source_commit}\n{sha256}\nbenchmarks/real-data/fixture/evidence/real-data-comparison.md\nsmall public fixture\n"
                f"| {command} | PASS | {comparison} |\n"
                "python3 tools/compare_real_data.py\n"
                "python3 tools/update_real_data_manifest.py\n"
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "release_candidate\n"
                "manifest-entry.json\n"
                "/evidence/manifest-entry.json\n"
                "scientific release\n"
                "not proof for every dataset\n"
                f"{portfolio_text}\n"
                "full 40-character Git commit SHA\none tiny fixture\n"
            )
            site = (
                f"{source_url}\n{source_commit}\n{sha256}\nbenchmarks/real-data/fixture/evidence/real-data-comparison.md\nsmall public fixture\n{command}\n"
                "python3 tools/compare_real_data.py\n"
                "python3 tools/update_real_data_manifest.py\n"
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "release_candidate\n"
                "manifest-entry.json\n"
                "/evidence/manifest-entry.json\n"
                "scientific release\n"
                "not proof for every dataset\n"
                f"{portfolio_text}\n"
                "full 40-character Git commit SHA\none tiny fixture\n"
            )
            benchmark_docs = (
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "python3 tools/verify_benchmark_thresholds.py\n"
                "release evidence\n"
                "not proof for every dataset\n"
                f"{portfolio_text}\n"
                "full 40-character Git commit SHA\none tiny fixture\n"
            )

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                self.assertEqual(
                    verify_real_data_evidence.validate_real_data_evidence(
                        manifest, readme, site, benchmark_docs
                    ),
                    [],
                )
                evidence = json.loads(evidence_json.read_text(encoding="utf-8"))
                evidence["input"]["size_bytes"] = 999
                evidence_json.write_text(json.dumps(evidence), encoding="utf-8")
                self.assertIn(
                    "fixture evidence input size changed",
                    verify_real_data_evidence.validate_real_data_evidence(
                        manifest, readme, site, benchmark_docs
                    ),
                )
                evidence["input"]["size_bytes"] = len(b"bam")
                evidence_json.write_text(json.dumps(evidence), encoding="utf-8")
                self.assertIn(
                    "real-data manifest has no release_candidate dataset for scientist-facing release",
                    verify_real_data_evidence.validate_real_data_evidence(
                        manifest, readme, site, benchmark_docs, release_ready=True
                    ),
                )

                manifest["datasets"][0]["release_tier"] = "release_candidate"
                manifest["datasets"][0]["minimum_input_bytes"] = 1
                manifest["datasets"][0]["expected_commands"] = {
                    "AddOrReplaceReadGroups": "SAM record digest",
                    "BuildBamIndex": "SAM record digest",
                    "ViewSam": comparison,
                    "CleanSam": "post-command SAM record digest",
                    "CollectQualityYieldMetrics": "stable metrics digest",
                    "CollectAlignmentSummaryMetrics": "stable metrics digest",
                    "MarkDuplicates": "duplicate-marking semantic digest plus stable metrics digest",
                    "CollectInsertSizeMetrics": "stable metrics digest with insert-size histogram",
                    "RevertSam": "SAM record digest",
                    "SamToFastq": "SAM record digest",
                    "SortSam": "SAM record digest",
                    "ValidateSamFile": "SAM record digest",
                }
                evidence = json.loads(evidence_json.read_text(encoding="utf-8"))
                evidence["commands"].extend(
                    [
                        {
                            "command": "CleanSam",
                            "status": "PASS",
                            "comparison": "post-command SAM record digest",
                            "turbo_seconds": 0.001,
                            "picard_seconds": 0.001,
                            "speedup": 1.0,
                            "turbo_artifact": "benchmarks/real-data/fixture/evidence/turbo.out",
                            "picard_artifact": "benchmarks/real-data/fixture/evidence/picard.out",
                            "turbo_digest": "abc123",
                            "picard_digest": "abc123",
                        },
                        {
                            "command": "CollectQualityYieldMetrics",
                            "status": "PASS",
                            "comparison": "stable metrics digest",
                            "turbo_seconds": 0.001,
                            "picard_seconds": 0.001,
                            "speedup": 1.0,
                            "turbo_artifact": "benchmarks/real-data/fixture/evidence/turbo.out",
                            "picard_artifact": "benchmarks/real-data/fixture/evidence/picard.out",
                            "turbo_digest": "abc123",
                            "picard_digest": "abc123",
                        },
                        {
                            "command": "CollectAlignmentSummaryMetrics",
                            "status": "PASS",
                            "comparison": "stable metrics digest",
                            "turbo_seconds": 0.001,
                            "picard_seconds": 0.001,
                            "speedup": 1.0,
                            "turbo_artifact": "benchmarks/real-data/fixture/evidence/turbo.out",
                            "picard_artifact": "benchmarks/real-data/fixture/evidence/picard.out",
                            "turbo_digest": "abc123",
                            "picard_digest": "abc123",
                        },
                        {
                            "command": "MarkDuplicates",
                            "status": "PASS",
                            "comparison": "duplicate-marking semantic digest plus stable metrics digest",
                            "turbo_seconds": 0.001,
                            "picard_seconds": 0.001,
                            "speedup": 1.0,
                            "turbo_artifact": "benchmarks/real-data/fixture/evidence/turbo.out",
                            "picard_artifact": "benchmarks/real-data/fixture/evidence/picard.out",
                            "turbo_digest": markduplicates_digest,
                            "picard_digest": markduplicates_digest,
                        },
                        {
                            "command": "CollectInsertSizeMetrics",
                            "status": "PASS",
                            "comparison": "stable metrics digest with insert-size histogram",
                            "turbo_seconds": 0.001,
                            "picard_seconds": 0.001,
                            "speedup": 1.0,
                            "turbo_artifact": "benchmarks/real-data/fixture/evidence/turbo.out",
                            "picard_artifact": "benchmarks/real-data/fixture/evidence/picard.out",
                            "turbo_digest": "abc123",
                            "picard_digest": "abc123",
                        },
                    ]
                )
                for command_name in [
                    "AddOrReplaceReadGroups",
                    "BuildBamIndex",
                    "RevertSam",
                    "SamToFastq",
                    "SortSam",
                    "ValidateSamFile",
                ]:
                    evidence["commands"].append(
                        {
                            "command": command_name,
                            "status": "PASS",
                            "comparison": "SAM record digest",
                            "turbo_seconds": 0.001,
                            "picard_seconds": 0.001,
                            "speedup": 1.0,
                            "turbo_artifact": "benchmarks/real-data/fixture/evidence/turbo.out",
                            "picard_artifact": "benchmarks/real-data/fixture/evidence/picard.out",
                            "turbo_digest": "abc123",
                            "picard_digest": "abc123",
                        }
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
                            handle.write(row + " 0.001s | 0.001s | 1.00x |\n")
                with evidence_md.open("a", encoding="utf-8") as handle:
                    handle.write(
                        "## Comparison details\n"
                        "ignores headers\n"
                        "after a BAM-writing command\n"
                        "generated headers do not affect parity\n"
                        "duplicate flags\n"
                        "## Artifact digests\n"
                        "benchmarks/real-data/fixture/evidence/turbo.out\n"
                        "benchmarks/real-data/fixture/evidence/picard.out\n"
                    )
                benchmark_docs += (
                    f"{source_url}\n{source_commit}\n{sha256}\n"
                    "benchmarks/real-data/fixture/evidence/real-data-comparison.md\n"
                    "small public fixture\n"
                    "ViewSam CleanSam CollectQualityYieldMetrics "
                    "CollectAlignmentSummaryMetrics MarkDuplicates "
                    "CollectInsertSizeMetrics\n"
                    f"{portfolio_text}\n"
                )
                project_readme = (
                    "fixture\n"
                    "benchmarks/real-data/\n"
                    "https://turbo-picard.readthedocs.io/en/latest/benchmarks.html\n"
                    "SHA-256\n"
                    "python3 tools/update_real_data_manifest.py\n"
                    "python3 tools/verify_real_data_evidence.py\n"
                    "python3 tools/verify_real_data_evidence.py --release-ready\n"
                )
                (evidence_root / "manifest-entry.json").write_text(
                    json.dumps(manifest["datasets"][0]),
                    encoding="utf-8",
                )
                with mock.patch.object(
                    verify_real_data_evidence,
                    "RELEASE_CANDIDATE_PORTFOLIO_MIN_BYTES",
                    1,
                ):
                    self.assertEqual(
                        verify_real_data_evidence.validate_real_data_evidence(
                            manifest,
                            readme,
                            site,
                            benchmark_docs,
                            project_readme,
                            release_ready=True,
                        ),
                        [],
                    )

                self.assertIn(
                    "fixture benchmark docs missing input SHA-256",
                    verify_real_data_evidence.validate_real_data_evidence(
                        manifest,
                        readme,
                        site,
                        "python3 tools/verify_real_data_evidence.py\n"
                        "python3 tools/verify_real_data_evidence.py --release-ready\n"
                        "python3 tools/verify_benchmark_thresholds.py\n"
                        "release-candidate\n",
                        project_readme,
                        release_ready=True,
                    ),
                )
                self.assertIn(
                    "fixture benchmark docs missing command: MarkDuplicates",
                    verify_real_data_evidence.validate_real_data_evidence(
                        manifest,
                        readme,
                        site,
                        f"{source_url}\n{source_commit}\n{sha256}\n"
                        "benchmarks/real-data/fixture/evidence/real-data-comparison.md\n"
                        "small public fixture\n"
                        "1000000\n"
                        "ViewSam CleanSam CollectQualityYieldMetrics "
                        "CollectAlignmentSummaryMetrics CollectInsertSizeMetrics\n",
                        project_readme,
                        release_ready=True,
                    ),
                )

    def test_project_readme_requires_release_candidate_citations(self) -> None:
        manifest = {
            "datasets": [
                {
                    "id": "release-dataset",
                    "release_tier": "release_candidate",
                    "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                    "source_commit": "0123456789abcdef0123456789abcdef01234567",
                    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    "evidence_markdown": "benchmarks/real-data/release-dataset/evidence/real-data-comparison.md",
                    "scope_caveat": "representative public fixture",
                    "expected_commands": {
                        "ViewSam": "SAM record digest",
                        "MarkDuplicates": "duplicate-marking semantic digest plus stable metrics digest",
                    },
                },
                {
                    "id": "smoke-dataset",
                    "release_tier": "public_smoke",
                    "source_url": "https://example.org/smoke",
                    "source_commit": "smoke-v1",
                    "sha256": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    "evidence_markdown": "smoke.md",
                    "scope_caveat": "smoke",
                    "expected_commands": {"ViewSam": "SAM record digest"},
                },
            ]
        }
        complete_readme = (
            "release-dataset\n"
            "benchmarks/real-data/\n"
            "https://turbo-picard.readthedocs.io/en/latest/benchmarks.html\n"
            "SHA-256\n"
            "python3 tools/update_real_data_manifest.py\n"
            "python3 tools/verify_real_data_evidence.py\n"
            "python3 tools/verify_real_data_evidence.py --release-ready\n"
        )

        self.assertEqual(
            verify_real_data_evidence.validate_project_readme_real_data_summary(
                manifest, complete_readme
            ),
            [],
        )

        errors = verify_real_data_evidence.validate_project_readme_real_data_summary(
            manifest,
            "release-dataset\nViewSam\n",
        )
        self.assertIn(
            "project README missing real-data evidence directory",
            errors,
        )
        self.assertIn(
            "project README missing release-ready real-data verifier command",
            errors,
        )
        self.assertIn(
            "project README missing input SHA-256 guidance",
            errors,
        )

    def test_validation_rejects_unpinned_source(self) -> None:
        errors = verify_real_data_evidence.validate_manifest(
            {
                "datasets": [
                    {
                        "id": "fixture",
                        "input_path": "benchmarks/real-data/giab-shard/input.bam",
                        "evidence_json": "benchmarks/real-data/giab-shard/evidence/real-data-comparison.json",
                        "evidence_markdown": "benchmarks/real-data/giab-shard/evidence/real-data-comparison.md",
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
        self.assertIn(
            "fixture sha256 must be a lowercase 64-character hex digest",
            errors,
        )

    def test_validation_rejects_manifest_paths_outside_real_data_tree(self) -> None:
        errors = verify_real_data_evidence.validate_manifest(
            {
                "datasets": [
                    {
                        "id": "bad-paths",
                        "input_path": "/tmp/input.bam",
                        "evidence_json": "benchmarks/real-data/bad/../other/evidence.json",
                        "evidence_markdown": "evidence.md",
                        "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                        "source_commit": "0123456789abcdef0123456789abcdef01234567",
                        "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                        "scope_caveat": "small public fixture",
                        "release_tier": "public_smoke",
                        "expected_commands": {"ViewSam": "SAM record digest"},
                    }
                ]
            }
        )

        self.assertIn(
            "bad-paths input_path must be repository-relative: /tmp/input.bam",
            errors,
        )
        self.assertIn(
            "bad-paths evidence_json must not contain path traversal: benchmarks/real-data/bad/../other/evidence.json",
            errors,
        )
        self.assertIn(
            "bad-paths evidence_markdown must stay under benchmarks/real-data: evidence.md",
            errors,
        )

    def test_validation_rejects_manifest_evidence_paths_outside_evidence_layout(self) -> None:
        errors = verify_real_data_evidence.validate_manifest(
            {
                "datasets": [
                    {
                        "id": "bad-layout",
                        "input_path": "benchmarks/real-data/bad-layout/input.bam",
                        "evidence_json": "benchmarks/real-data/bad-layout/real-data-comparison.json",
                        "evidence_markdown": "benchmarks/real-data/bad-layout/evidence/report.md",
                        "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                        "source_commit": "0123456789abcdef0123456789abcdef01234567",
                        "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                        "scope_caveat": "small public fixture",
                        "release_tier": "public_smoke",
                        "expected_commands": {"ViewSam": "SAM record digest"},
                    }
                ]
            }
        )

        self.assertIn(
            "bad-layout evidence_json must use evidence/real-data-comparison.json: "
            "benchmarks/real-data/bad-layout/real-data-comparison.json",
            errors,
        )
        self.assertIn(
            "bad-layout evidence_markdown must use evidence/real-data-comparison.md: "
            "benchmarks/real-data/bad-layout/evidence/report.md",
            errors,
        )

    def test_validation_rejects_malformed_expected_commands(self) -> None:
        errors = verify_real_data_evidence.validate_manifest(
            {
                "datasets": [
                    {
                        "id": "bad-commands",
                        "input_path": "benchmarks/real-data/bad-commands/input.bam",
                        "evidence_json": "benchmarks/real-data/bad-commands/evidence/real-data-comparison.json",
                        "evidence_markdown": "benchmarks/real-data/bad-commands/evidence/real-data-comparison.md",
                        "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                        "source_commit": "0123456789abcdef0123456789abcdef01234567",
                        "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                        "scope_caveat": "small public fixture",
                        "release_tier": "public_smoke",
                        "expected_commands": {"ViewSam": ""},
                    }
                ]
            }
        )

        self.assertIn(
            "bad-commands expected_commands must map non-empty command names to non-empty comparison labels",
            errors,
        )

    def test_validation_rejects_unknown_comparison_labels(self) -> None:
        errors = verify_real_data_evidence.validate_manifest(
            {
                "datasets": [
                    {
                        "id": "bad-comparison",
                        "input_path": "benchmarks/real-data/bad-comparison/input.bam",
                        "evidence_json": "benchmarks/real-data/bad-comparison/evidence/real-data-comparison.json",
                        "evidence_markdown": "benchmarks/real-data/bad-comparison/evidence/real-data-comparison.md",
                        "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                        "source_commit": "0123456789abcdef0123456789abcdef01234567",
                        "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                        "scope_caveat": "small public fixture",
                        "release_tier": "public_smoke",
                        "expected_commands": {"ViewSam": "roughly similar output"},
                    }
                ]
            }
        )

        self.assertIn(
            "bad-comparison expected_commands use unknown comparison labels: roughly similar output",
            errors,
        )

    def test_validation_accepts_https_accession_style_source(self) -> None:
        errors = verify_real_data_evidence.validate_manifest(
            {
                "datasets": [
                    {
                        "id": "giab-shard",
                        "input_path": "benchmarks/real-data/giab-shard/input.bam",
                        "evidence_json": "benchmarks/real-data/giab-shard/evidence/real-data-comparison.json",
                        "evidence_markdown": "benchmarks/real-data/giab-shard/evidence/real-data-comparison.md",
                        "source_url": "https://example.org/datasets/GIAB-HG001-v4.2.1/input.bam",
                        "source_commit": "GIAB-HG001-v4.2.1",
                        "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
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
                        "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
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
                        "source_commit": "0123456789abcdef0123456789abcdef01234567",
                        "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                        "scope_caveat": "fixture",
                        "release_tier": "public_smoke",
                        "expected_commands": {"ViewSam": "SAM record digest"},
                    }
                ]
            }
        )

        self.assertIn(
            "github-fixture GitHub source_url must include /blob/0123456789abcdef0123456789abcdef01234567/",
            errors,
        )

    def test_validation_rejects_short_github_commit_for_all_tiers(self) -> None:
        errors = verify_real_data_evidence.validate_manifest(
            {
                "datasets": [
                    {
                        "id": "short-github-commit",
                        "input_path": "benchmarks/real-data/fixture/input.bam",
                        "evidence_json": "benchmarks/real-data/fixture/evidence/real-data-comparison.json",
                        "evidence_markdown": "benchmarks/real-data/fixture/evidence/real-data-comparison.md",
                        "source_url": "https://github.com/example/repo/blob/abc123/input.bam",
                        "source_commit": "abc123",
                        "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                        "scope_caveat": "fixture",
                        "release_tier": "public_smoke",
                        "expected_commands": {"ViewSam": "SAM record digest"},
                    }
                ]
            }
        )

        self.assertIn(
            "short-github-commit GitHub source_commit must be a full 40-character SHA",
            errors,
        )

    def test_validation_requires_workflow_documentation(self) -> None:
        errors = verify_real_data_evidence.validate_workflow_docs(
            "python3 tools/verify_real_data_evidence.py",
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
        self.assertIn(
            "benchmark docs missing release-ready real-data verifier command",
            errors,
        )
        self.assertIn(
            "benchmarks README missing broad-dataset scope caveat",
            errors,
        )
        self.assertIn(
            "site missing scientific release wording",
            errors,
        )
        self.assertIn(
            "benchmark docs missing broad-dataset scope caveat",
            errors,
        )

        complete_workflow_doc = (
            "python3 tools/compare_real_data.py\n"
            "python3 tools/update_real_data_manifest.py\n"
            "python3 tools/verify_real_data_evidence.py\n"
            "python3 tools/verify_real_data_evidence.py --release-ready\n"
            "python3 tools/verify_benchmark_thresholds.py\n"
            "release_candidate\n"
            "manifest-entry.json\n"
            "/evidence/manifest-entry.json\n"
            "scientific release\n"
            "not proof for every dataset\n"
            "release evidence\n"
            "full 40-character Git commit SHA\none tiny fixture\n"
        )
        adoption_errors = verify_real_data_evidence.validate_workflow_docs(
            complete_workflow_doc,
            complete_workflow_doc,
            complete_workflow_doc,
            "Use it quickly.",
        )
        self.assertIn(
            "adoption docs missing one-command-at-a-time caveat",
            adoption_errors,
        )
        self.assertIn(
            "adoption docs missing side-by-side comparison caveat",
            adoption_errors,
        )

    def test_workflow_documentation_rejects_unsupported_replacement_overclaims(self) -> None:
        baseline = (
            "python3 tools/compare_real_data.py\n"
            "python3 tools/update_real_data_manifest.py\n"
            "python3 tools/verify_real_data_evidence.py\n"
            "python3 tools/verify_real_data_evidence.py --release-ready\n"
            "python3 tools/verify_benchmark_thresholds.py\n"
            "release_candidate\n"
            "manifest-entry.json\n"
            "/evidence/manifest-entry.json\n"
            "scientific release\n"
            "not proof for every dataset\n"
            "release evidence\n"
            "full 40-character Git commit SHA\none tiny fixture\n"
        )

        errors = verify_real_data_evidence.validate_workflow_docs(
            baseline + "This is a drop-in replacement.\n",
            baseline + "Production genomics workflows.\n",
            baseline + "Complete cohort-scale validation.\n",
            baseline + "Safe for all production.\n",
        )

        self.assertIn(
            "benchmarks README contains unsupported overclaim: drop-in replacement",
            errors,
        )
        self.assertIn(
            "site contains unsupported overclaim: production genomics workflows",
            errors,
        )
        self.assertIn(
            "benchmark docs contains unsupported overclaim: complete cohort-scale validation",
            errors,
        )
        self.assertIn(
            "adoption docs contains unsupported overclaim: safe for all production",
            errors,
        )

    def test_project_readme_rejects_unsupported_replacement_overclaims(self) -> None:
        manifest = {"datasets": []}

        self.assertIn(
            "project README contains unsupported overclaim: production genomics workflows",
            verify_real_data_evidence.validate_project_readme_real_data_summary(
                manifest,
                "Ready for production genomics workflows.",
            ),
        )

    def test_release_candidate_requires_broad_commands_and_size(self) -> None:
        dataset = {
            "id": "tiny-release",
            "release_tier": "release_candidate",
            "source_url": "https://github.com/example/repo/blob/abc123/input.bam",
            "source_commit": "abc123",
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
            "tiny-release release_candidate missing minimum_input_bytes",
            errors,
        )
        self.assertIn(
            "tiny-release release_candidate input too small: 10 bytes < 1000000",
            errors,
        )
        self.assertIn(
            "tiny-release release_candidate GitHub source_commit must be a full 40-character SHA",
            errors,
        )

    def test_release_candidate_minimum_size_can_be_manifest_explicit(self) -> None:
        dataset = {
            "id": "reviewed-small-release",
            "release_tier": "release_candidate",
            "minimum_input_bytes": 10,
            "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
            "source_commit": "0123456789abcdef0123456789abcdef01234567",
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

    def test_release_ready_requires_portfolio_insert_size_evidence(self) -> None:
        manifest = {
            "datasets": [
                {
                    "id": "fixture",
                    "input_path": "benchmarks/real-data/fixture/input.bam",
                    "evidence_json": "benchmarks/real-data/fixture/evidence/real-data-comparison.json",
                    "evidence_markdown": "benchmarks/real-data/fixture/evidence/real-data-comparison.md",
                    "source_url": (
                        "https://github.com/example/repo/blob/"
                        "0123456789abcdef0123456789abcdef01234567/input.bam"
                    ),
                    "source_commit": "0123456789abcdef0123456789abcdef01234567",
                    "sha256": (
                        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    ),
                    "scope_caveat": "reviewed fixture",
                    "release_tier": "release_candidate",
                    "minimum_input_bytes": 1,
                    "expected_commands": {
                        "ViewSam": "SAM record digest",
                        "CleanSam": "post-command SAM record digest",
                        "CollectQualityYieldMetrics": "stable metrics digest",
                        "CollectAlignmentSummaryMetrics": "stable metrics digest",
                        "MarkDuplicates": (
                            "duplicate-marking semantic digest plus stable metrics digest"
                        ),
                    },
                }
            ]
        }
        docs = (
            "python3 tools/compare_real_data.py\n"
            "python3 tools/update_real_data_manifest.py\n"
            "python3 tools/verify_real_data_evidence.py\n"
            "python3 tools/verify_real_data_evidence.py --release-ready\n"
            "python3 tools/verify_benchmark_thresholds.py\n"
            "release_candidate\n"
            "manifest-entry.json\n"
            "/evidence/manifest-entry.json\n"
            "scientific release\n"
            "not proof for every dataset\n"
            f"{verify_real_data_evidence.RELEASE_CANDIDATE_PORTFOLIO_COMMAND_TEXT}\n"
            "full 40-character Git commit SHA\n"
            "one tiny fixture\n"
        )
        benchmark_docs = (
            "python3 tools/verify_real_data_evidence.py\n"
            "python3 tools/verify_real_data_evidence.py --release-ready\n"
            "python3 tools/verify_benchmark_thresholds.py\n"
            "release evidence\n"
            "not proof for every dataset\n"
            f"{verify_real_data_evidence.RELEASE_CANDIDATE_PORTFOLIO_COMMAND_TEXT}\n"
            "full 40-character Git commit SHA\n"
            "one tiny fixture\n"
        )

        with mock.patch.object(verify_real_data_evidence, "validate_dataset", return_value=[]):
            errors = verify_real_data_evidence.validate_real_data_evidence(
                manifest,
                docs,
                docs,
                benchmark_docs,
                release_ready=True,
            )

        self.assertIn(
            "real-data release_candidate portfolio missing required command evidence: "
            "AddOrReplaceReadGroups, BuildBamIndex, CollectInsertSizeMetrics, "
            "RevertSam, SamToFastq, SortSam, ValidateSamFile",
            errors,
        )

    def test_release_ready_requires_aggregate_candidate_input_size(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            evidence_dir = root / "benchmarks" / "real-data" / "fixture" / "evidence"
            evidence_dir.mkdir(parents=True)
            (evidence_dir / "real-data-comparison.json").write_text(
                json.dumps({"input": {"size_bytes": 9}}),
                encoding="utf-8",
            )
            manifest = {
                "datasets": [
                    {
                        "id": "fixture",
                        "input_path": "benchmarks/real-data/fixture/input.bam",
                        "evidence_json": "benchmarks/real-data/fixture/evidence/real-data-comparison.json",
                        "evidence_markdown": "benchmarks/real-data/fixture/evidence/real-data-comparison.md",
                        "source_url": (
                            "https://github.com/example/repo/blob/"
                            "0123456789abcdef0123456789abcdef01234567/input.bam"
                        ),
                        "source_commit": "0123456789abcdef0123456789abcdef01234567",
                        "sha256": (
                            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        ),
                        "scope_caveat": "reviewed fixture",
                        "release_tier": "release_candidate",
                        "minimum_input_bytes": 1,
                        "expected_commands": {
                            "ViewSam": "SAM record digest",
                            "CleanSam": "post-command SAM record digest",
                            "CollectQualityYieldMetrics": "stable metrics digest",
                            "CollectAlignmentSummaryMetrics": "stable metrics digest",
                            "MarkDuplicates": (
                                "duplicate-marking semantic digest plus stable metrics digest"
                            ),
                            "CollectInsertSizeMetrics": (
                                "stable metrics digest with insert-size histogram"
                            ),
                        },
                    }
                ]
            }
            docs = (
                "python3 tools/compare_real_data.py\n"
                "python3 tools/update_real_data_manifest.py\n"
                "python3 tools/verify_real_data_evidence.py\n"
                "python3 tools/verify_real_data_evidence.py --release-ready\n"
                "release_candidate\n"
                "manifest-entry.json\n"
                "/evidence/manifest-entry.json\n"
                "scientific release\n"
                "not proof for every dataset\n"
                "CollectInsertSizeMetrics\n"
                "full 40-character Git commit SHA\n"
                "one tiny fixture\n"
            )
            benchmark_docs = docs + "release evidence\n"

            with (
                mock.patch.object(verify_real_data_evidence, "ROOT", root),
                mock.patch.object(
                    verify_real_data_evidence,
                    "RELEASE_CANDIDATE_PORTFOLIO_MIN_BYTES",
                    10,
                ),
                mock.patch.object(verify_real_data_evidence, "validate_dataset", return_value=[]),
            ):
                errors = verify_real_data_evidence.validate_real_data_evidence(
                    manifest,
                    docs,
                    docs,
                    benchmark_docs,
                    docs,
                    release_ready=True,
                )

        self.assertIn(
            "real-data release_candidate portfolio input too small: 9 bytes < 10",
            errors,
        )

    def test_release_candidate_docs_reject_stale_release_ready_failure_text(self) -> None:
        manifest = {
            "datasets": [
                {
                    "id": "fixture",
                    "input_path": "benchmarks/real-data/fixture/input.bam",
                    "evidence_json": "benchmarks/real-data/fixture/evidence/real-data-comparison.json",
                    "evidence_markdown": "benchmarks/real-data/fixture/evidence/real-data-comparison.md",
                    "source_url": (
                        "https://github.com/example/repo/blob/"
                        "0123456789abcdef0123456789abcdef01234567/input.bam"
                    ),
                    "source_commit": "0123456789abcdef0123456789abcdef01234567",
                    "sha256": (
                        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    ),
                    "scope_caveat": "reviewed fixture",
                    "release_tier": "release_candidate",
                    "minimum_input_bytes": 1,
                    "expected_commands": {
                        "ViewSam": "SAM record digest",
                        "CleanSam": "post-command SAM record digest",
                        "CollectQualityYieldMetrics": "stable metrics digest",
                        "CollectAlignmentSummaryMetrics": "stable metrics digest",
                        "MarkDuplicates": (
                            "duplicate-marking semantic digest plus stable metrics digest"
                        ),
                    },
                }
            ]
        }
        readme = (
            "python3 tools/compare_real_data.py\n"
            "python3 tools/update_real_data_manifest.py\n"
            "python3 tools/verify_real_data_evidence.py\n"
            "python3 tools/verify_real_data_evidence.py --release-ready\n"
            "release_candidate\n"
            "manifest-entry.json\n"
            "/evidence/manifest-entry.json\n"
            "scientific release\n"
            "not proof for every dataset\n"
            "CollectInsertSizeMetrics\n"
            "fails until the manifest contains at least one pinned release-candidate dataset\n"
        )
        site = readme
        benchmark_docs = (
            "python3 tools/verify_real_data_evidence.py\n"
            "python3 tools/verify_real_data_evidence.py --release-ready\n"
            "release evidence\n"
            "not proof for every dataset\n"
            "CollectInsertSizeMetrics\n"
        )

        with mock.patch.object(verify_real_data_evidence, "validate_dataset", return_value=[]):
            errors = verify_real_data_evidence.validate_real_data_evidence(
                manifest,
                readme,
                site,
                benchmark_docs,
                release_ready=True,
            )

        self.assertIn(
            "benchmarks README still says release-ready verification fails before release candidates",
            errors,
        )

    def test_dataset_requires_timing_evidence_and_markdown_timing_row(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.bam"
            evidence_json = root / "evidence.json"
            evidence_md = root / "evidence.md"
            input_path.write_bytes(b"bam")
            sha256 = verify_real_data_evidence.digest_file(input_path)
            source_url = "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/test/input.bam"
            command = "ViewSam"
            comparison = "SAM record digest"
            dataset = {
                "id": "fixture",
                "input_path": "input.bam",
                "evidence_json": "evidence.json",
                "evidence_markdown": "evidence.md",
                "source_url": source_url,
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "sha256": sha256,
                "scope_caveat": "small public fixture",
                "release_tier": "public_smoke",
                "expected_commands": {command: comparison},
            }
            evidence_json.write_text(
                json.dumps(
                    {
                        "parity": "PASS",
                        "picard_version": "Version:3.4.0",
                        "turbo_picard_version": "picard 0.1.0",
                        "input": {
                            "sha256": sha256,
                            "source_url": source_url,
                            "source_commit": "0123456789abcdef0123456789abcdef01234567",
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
                f"| {command} | PASS | {comparison} |\n",
                encoding="utf-8",
            )
            readme = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"| {command} | PASS | {comparison} |\n"
            )
            site = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"{command}\npython3 tools/verify_real_data_evidence.py\n"
            )

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                errors = verify_real_data_evidence.validate_dataset(
                    dataset,
                    readme,
                    site,
                )

            self.assertIn(
                "fixture ViewSam missing positive turbo-picard timing",
                errors,
            )
            self.assertIn("fixture ViewSam missing positive Picard timing", errors)
            self.assertIn("fixture ViewSam missing positive speedup", errors)
            self.assertIn("fixture Markdown missing timing row: ViewSam", errors)

    def test_dataset_rejects_speedup_that_does_not_match_timing_ratio(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.bam"
            evidence_json = root / "evidence.json"
            evidence_md = root / "evidence.md"
            input_path.write_bytes(b"bam")
            sha256 = verify_real_data_evidence.digest_file(input_path)
            source_url = "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/test/input.bam"
            command = "ViewSam"
            comparison = "SAM record digest"
            dataset = {
                "id": "fixture",
                "input_path": "input.bam",
                "evidence_json": "evidence.json",
                "evidence_markdown": "evidence.md",
                "source_url": source_url,
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "sha256": sha256,
                "scope_caveat": "small public fixture",
                "release_tier": "public_smoke",
                "expected_commands": {command: comparison},
            }
            evidence_json.write_text(
                json.dumps(
                    {
                        "parity": "PASS",
                        "picard_version": "Version:3.4.0",
                        "turbo_picard_version": "picard 0.1.0",
                        "input": {
                            "sha256": sha256,
                            "source_url": source_url,
                            "source_commit": "0123456789abcdef0123456789abcdef01234567",
                            "size_bytes": len(b"bam"),
                        },
                        "commands": [
                            {
                                "command": command,
                                "status": "PASS",
                                "comparison": comparison,
                                "turbo_seconds": 2.0,
                                "picard_seconds": 10.0,
                                "speedup": 3.0,
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            evidence_md.write_text(
                "Picard: `Version:3.4.0`\n"
                f"| {command} | PASS | {comparison} | 2.000s | 10.000s | 3.00x |\n",
                encoding="utf-8",
            )
            readme = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"| {command} | PASS | {comparison} |\n"
            )
            site = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"{command}\npython3 tools/verify_real_data_evidence.py\n"
            )

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                errors = verify_real_data_evidence.validate_dataset(
                    dataset,
                    readme,
                    site,
                )

            self.assertIn(
                "fixture ViewSam speedup does not match timing ratio: 3.0000 != 5.0000",
                errors,
            )

    def test_dataset_requires_command_artifacts_and_matching_digests(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.bam"
            evidence_json = root / "evidence.json"
            evidence_md = root / "evidence.md"
            input_path.write_bytes(b"bam")
            (root / "turbo.out").write_text("turbo\n", encoding="utf-8")
            sha256 = verify_real_data_evidence.digest_file(input_path)
            source_url = "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/test/input.bam"
            command = "ViewSam"
            comparison = "SAM record digest"
            dataset = {
                "id": "fixture",
                "input_path": "input.bam",
                "evidence_json": "evidence.json",
                "evidence_markdown": "evidence.md",
                "source_url": source_url,
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "sha256": sha256,
                "scope_caveat": "small public fixture",
                "release_tier": "public_smoke",
                "expected_commands": {command: comparison},
            }
            evidence_json.write_text(
                json.dumps(
                    {
                        "parity": "PASS",
                        "picard_version": "Version:3.4.0",
                        "turbo_picard_version": "picard 0.1.0",
                        "input": {
                            "sha256": sha256,
                            "source_url": source_url,
                            "source_commit": "0123456789abcdef0123456789abcdef01234567",
                            "size_bytes": len(b"bam"),
                        },
                        "commands": [
                            {
                                "command": command,
                                "status": "PASS",
                                "comparison": comparison,
                                "turbo_seconds": 0.001,
                                "picard_seconds": 0.001,
                                "speedup": 1.0,
                                "turbo_artifact": "turbo.out",
                                "picard_artifact": "missing-picard.out",
                                "turbo_digest": "abc123",
                                "picard_digest": "def456",
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
            readme = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"| {command} | PASS | {comparison} |\n"
            )
            site = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"{command}\npython3 tools/verify_real_data_evidence.py\n"
            )

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                errors = verify_real_data_evidence.validate_dataset(
                    dataset,
                    readme,
                    site,
                )

            self.assertIn(
                "fixture ViewSam turbo-picard/Picard digests differ",
                errors,
            )
            self.assertIn(
                "fixture ViewSam missing Picard artifact file: missing-picard.out",
                errors,
            )


    def test_dataset_recomputes_sam_and_metrics_artifact_digests(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            evidence_dir = root / "benchmarks" / "real-data" / "fixture" / "evidence"
            evidence_dir.mkdir(parents=True)
            sam_path = evidence_dir / "turbo.sam"
            metrics_path = evidence_dir / "picard.metrics.txt"
            sam_path.write_text(
                "@HD\tVN:1.6\n"
                "r1\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\tZF:f:1.0\n",
                encoding="utf-8",
            )
            metrics_path.write_text("# comment\nPF_BASES\n10\n", encoding="utf-8")

            self.assertEqual(
                verify_real_data_evidence.recomputable_artifact_digest(
                    sam_path, "SAM record digest"
                ),
                verify_real_data_evidence.digest_sam_records(sam_path),
            )
            self.assertEqual(
                verify_real_data_evidence.recomputable_artifact_digest(
                    metrics_path, "stable metrics digest"
                ),
                verify_real_data_evidence.digest_stable_text(metrics_path),
            )
            self.assertEqual(
                verify_real_data_evidence.recomputable_artifact_digest(
                    metrics_path, "stable metrics digest with insert-size histogram"
                ),
                verify_real_data_evidence.digest_stable_text(metrics_path),
            )

            self.assertNotEqual(
                verify_real_data_evidence.digest_sam_records(sam_path),
                "not-the-digest",
            )

    def test_dataset_rejects_command_artifact_outside_evidence_directory(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            evidence_dir = root / "evidence"
            evidence_dir.mkdir()
            input_path = root / "input.bam"
            evidence_json = evidence_dir / "real-data-comparison.json"
            evidence_md = evidence_dir / "real-data-comparison.md"
            input_path.write_bytes(b"bam")
            (root / "outside.out").write_text("same\n", encoding="utf-8")
            absolute_artifact = root / "absolute.out"
            absolute_artifact.write_text("same\n", encoding="utf-8")
            (evidence_dir / "picard.out").write_text("same\n", encoding="utf-8")
            sha256 = verify_real_data_evidence.digest_file(input_path)
            source_url = "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/test/input.bam"
            command = "ViewSam"
            comparison = "SAM record digest"
            dataset = {
                "id": "fixture",
                "input_path": "input.bam",
                "evidence_json": "evidence/real-data-comparison.json",
                "evidence_markdown": "evidence/real-data-comparison.md",
                "source_url": source_url,
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "sha256": sha256,
                "scope_caveat": "small public fixture",
                "release_tier": "public_smoke",
                "expected_commands": {
                    command: comparison,
                    "CleanSam": "post-command SAM record digest",
                },
            }
            evidence_json.write_text(
                json.dumps(
                    {
                        "parity": "PASS",
                        "picard_version": "Version:3.4.0",
                        "turbo_picard_version": "picard 0.1.0",
                        "input": {
                            "sha256": sha256,
                            "source_url": source_url,
                            "source_commit": "0123456789abcdef0123456789abcdef01234567",
                            "size_bytes": len(b"bam"),
                        },
                        "commands": [
                            {
                                "command": command,
                                "status": "PASS",
                                "comparison": comparison,
                                "turbo_seconds": 0.001,
                                "picard_seconds": 0.001,
                                "speedup": 1.0,
                                "turbo_artifact": "outside.out",
                                "picard_artifact": "evidence/picard.out",
                                "turbo_digest": "abc123",
                                "picard_digest": "abc123",
                            },
                            {
                                "command": "CleanSam",
                                "status": "PASS",
                                "comparison": "post-command SAM record digest",
                                "turbo_seconds": 0.001,
                                "picard_seconds": 0.001,
                                "speedup": 1.0,
                                "turbo_artifact": str(absolute_artifact),
                                "picard_artifact": "evidence/picard.out",
                                "turbo_digest": "abc123",
                                "picard_digest": "abc123",
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
            readme = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence/real-data-comparison.md\nsmall public fixture\n"
                f"| {command} | PASS | {comparison} |\n"
            )
            site = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence/real-data-comparison.md\nsmall public fixture\n"
                f"{command}\npython3 tools/verify_real_data_evidence.py\n"
            )

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                errors = verify_real_data_evidence.validate_dataset(
                    dataset,
                    readme,
                    site,
                )

            self.assertIn(
                "fixture ViewSam turbo-picard artifact must stay under evidence directory: outside.out",
                errors,
            )
            self.assertIn(
                f"fixture CleanSam turbo-picard artifact must be repository-relative: {absolute_artifact}",
                errors,
            )

    def test_dataset_rejects_unreviewed_or_duplicate_command_rows(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.bam"
            evidence_json = root / "evidence.json"
            evidence_md = root / "evidence.md"
            input_path.write_bytes(b"bam")
            sha256 = verify_real_data_evidence.digest_file(input_path)
            source_url = "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/test/input.bam"
            command = "ViewSam"
            comparison = "SAM record digest"
            dataset = {
                "id": "fixture",
                "input_path": "input.bam",
                "evidence_json": "evidence.json",
                "evidence_markdown": "evidence.md",
                "source_url": source_url,
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "sha256": sha256,
                "scope_caveat": "small public fixture",
                "release_tier": "public_smoke",
                "expected_commands": {command: comparison},
            }
            row = {
                "command": command,
                "status": "PASS",
                "comparison": comparison,
                "turbo_seconds": 0.001,
                "picard_seconds": 0.001,
                "speedup": 1.0,
            }
            evidence_json.write_text(
                json.dumps(
                    {
                        "parity": "PASS",
                        "picard_version": "Version:3.4.0",
                        "turbo_picard_version": "picard 0.1.0",
                        "input": {
                            "sha256": sha256,
                            "source_url": source_url,
                            "source_commit": "0123456789abcdef0123456789abcdef01234567",
                            "size_bytes": len(b"bam"),
                        },
                        "commands": [
                            row,
                            dict(row),
                            {
                                "command": "CleanSam",
                                "status": "PASS",
                                "comparison": "post-command SAM record digest",
                                "turbo_seconds": 0.001,
                                "picard_seconds": 0.001,
                                "speedup": 1.0,
                            },
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
            readme = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"| {command} | PASS | {comparison} |\n"
            )
            site = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"{command}\npython3 tools/verify_real_data_evidence.py\n"
            )

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                errors = verify_real_data_evidence.validate_dataset(
                    dataset,
                    readme,
                    site,
                )

            self.assertIn("fixture duplicate command evidence: ViewSam", errors)
            self.assertIn("fixture unreviewed extra command evidence: CleanSam", errors)

    def test_dataset_rejects_malformed_command_rows_without_crashing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.bam"
            evidence_json = root / "evidence.json"
            evidence_md = root / "evidence.md"
            input_path.write_bytes(b"bam")
            sha256 = verify_real_data_evidence.digest_file(input_path)
            source_url = "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/test/input.bam"
            command = "ViewSam"
            comparison = "SAM record digest"
            dataset = {
                "id": "fixture",
                "input_path": "input.bam",
                "evidence_json": "evidence.json",
                "evidence_markdown": "evidence.md",
                "source_url": source_url,
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "sha256": sha256,
                "scope_caveat": "small public fixture",
                "release_tier": "public_smoke",
                "expected_commands": {command: comparison},
            }
            base_evidence = {
                "parity": "PASS",
                "picard_version": "Version:3.4.0",
                "turbo_picard_version": "picard 0.1.0",
                "input": {
                    "sha256": sha256,
                    "source_url": source_url,
                    "source_commit": "0123456789abcdef0123456789abcdef01234567",
                    "size_bytes": len(b"bam"),
                },
                "commands": {"command": command},
            }
            evidence_json.write_text(json.dumps(base_evidence), encoding="utf-8")
            evidence_md.write_text(
                "Picard: `Version:3.4.0`\n"
                f"| {command} | PASS | {comparison} | 0.001s | 0.001s | 1.00x |\n",
                encoding="utf-8",
            )
            readme = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"| {command} | PASS | {comparison} |\n"
            )
            site = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"{command}\npython3 tools/verify_real_data_evidence.py\n"
            )

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                errors = verify_real_data_evidence.validate_dataset(
                    dataset,
                    readme,
                    site,
                )

            self.assertIn("fixture evidence commands must be a list", errors)

            base_evidence["commands"] = ["not-an-object"]
            evidence_json.write_text(json.dumps(base_evidence), encoding="utf-8")
            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                errors = verify_real_data_evidence.validate_dataset(
                    dataset,
                    readme,
                    site,
                )

            self.assertIn("fixture evidence command row 0 must be an object", errors)

    def test_dataset_requires_machine_readable_tool_versions(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.bam"
            evidence_json = root / "evidence.json"
            evidence_md = root / "evidence.md"
            input_path.write_bytes(b"bam")
            sha256 = verify_real_data_evidence.digest_file(input_path)
            source_url = "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/test/input.bam"
            command = "ViewSam"
            comparison = "SAM record digest"
            dataset = {
                "id": "fixture",
                "input_path": "input.bam",
                "evidence_json": "evidence.json",
                "evidence_markdown": "evidence.md",
                "source_url": source_url,
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "sha256": sha256,
                "scope_caveat": "small public fixture",
                "release_tier": "public_smoke",
                "expected_commands": {command: comparison},
            }
            evidence_json.write_text(
                json.dumps(
                    {
                        "parity": "PASS",
                        "picard_version": "Version:3.3.0",
                        "turbo_picard_version": "",
                        "input": {
                            "sha256": sha256,
                            "source_url": source_url,
                            "source_commit": "0123456789abcdef0123456789abcdef01234567",
                            "size_bytes": len(b"bam"),
                        },
                        "commands": [
                            {
                                "command": command,
                                "status": "PASS",
                                "comparison": comparison,
                                "turbo_seconds": 0.001,
                                "picard_seconds": 0.001,
                                "speedup": 1.0,
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
            readme = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"| {command} | PASS | {comparison} |\n"
            )
            site = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"{command}\npython3 tools/verify_real_data_evidence.py\n"
            )

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                errors = verify_real_data_evidence.validate_dataset(
                    dataset,
                    readme,
                    site,
                )

            self.assertIn("fixture evidence Picard version changed", errors)
            self.assertIn("fixture evidence missing turbo-picard version", errors)

    def test_dataset_rejects_malformed_input_summary_without_crashing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.bam"
            evidence_json = root / "evidence.json"
            evidence_md = root / "evidence.md"
            input_path.write_bytes(b"bam")
            sha256 = verify_real_data_evidence.digest_file(input_path)
            source_url = "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/test/input.bam"
            command = "ViewSam"
            comparison = "SAM record digest"
            dataset = {
                "id": "fixture",
                "input_path": "input.bam",
                "evidence_json": "evidence.json",
                "evidence_markdown": "evidence.md",
                "source_url": source_url,
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "sha256": sha256,
                "scope_caveat": "small public fixture",
                "release_tier": "public_smoke",
                "expected_commands": {command: comparison},
            }
            evidence_json.write_text(
                json.dumps(
                    {
                        "parity": "PASS",
                        "picard_version": "Version:3.4.0",
                        "turbo_picard_version": "picard 0.1.0",
                        "input": "not-an-object",
                        "commands": [
                            {
                                "command": command,
                                "status": "PASS",
                                "comparison": comparison,
                                "turbo_seconds": 0.001,
                                "picard_seconds": 0.001,
                                "speedup": 1.0,
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
            readme = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"| {command} | PASS | {comparison} |\n"
            )
            site = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"{command}\npython3 tools/verify_real_data_evidence.py\n"
            )

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                errors = verify_real_data_evidence.validate_dataset(
                    dataset,
                    readme,
                    site,
                )

            self.assertIn("fixture evidence input must be an object", errors)
            self.assertIn("fixture evidence SHA-256 changed", errors)

    def test_dataset_rejects_malformed_top_level_evidence_without_crashing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "input.bam"
            evidence_json = root / "evidence.json"
            evidence_md = root / "evidence.md"
            input_path.write_bytes(b"bam")
            sha256 = verify_real_data_evidence.digest_file(input_path)
            source_url = "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/test/input.bam"
            command = "ViewSam"
            comparison = "SAM record digest"
            dataset = {
                "id": "fixture",
                "input_path": "input.bam",
                "evidence_json": "evidence.json",
                "evidence_markdown": "evidence.md",
                "source_url": source_url,
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "sha256": sha256,
                "scope_caveat": "small public fixture",
                "release_tier": "public_smoke",
                "expected_commands": {command: comparison},
            }
            evidence_json.write_text(json.dumps(["not", "an", "object"]), encoding="utf-8")
            evidence_md.write_text(
                "Picard: `Version:3.4.0`\n"
                f"| {command} | PASS | {comparison} | 0.001s | 0.001s | 1.00x |\n",
                encoding="utf-8",
            )
            readme = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"| {command} | PASS | {comparison} |\n"
            )
            site = (
                f"{source_url}\n0123456789abcdef0123456789abcdef01234567\n{sha256}\nevidence.md\nsmall public fixture\n"
                f"{command}\npython3 tools/verify_real_data_evidence.py\n"
            )

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                errors = verify_real_data_evidence.validate_dataset(
                    dataset,
                    readme,
                    site,
                )

            self.assertIn("fixture evidence JSON must be an object", errors)
            self.assertIn("fixture evidence SHA-256 changed", errors)

    def test_manifest_entry_artifact_must_match_checked_manifest_entry(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            evidence_dir = root / "benchmarks" / "real-data" / "fixture" / "evidence"
            evidence_dir.mkdir(parents=True)
            dataset = {
                "id": "fixture",
                "input_path": "benchmarks/real-data/fixture/input.bam",
                "evidence_json": "benchmarks/real-data/fixture/evidence/real-data-comparison.json",
                "evidence_markdown": "benchmarks/real-data/fixture/evidence/real-data-comparison.md",
                "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "sha256": "a" * 64,
                "scope_caveat": "small public fixture",
                "release_tier": "public_smoke",
                "expected_commands": {"ViewSam": "SAM record digest"},
            }
            stale_entry = {**dataset, "scope_caveat": "stale caveat"}
            (evidence_dir / "manifest-entry.json").write_text(
                json.dumps(stale_entry),
                encoding="utf-8",
            )

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                self.assertEqual(
                    verify_real_data_evidence.validate_manifest_entry_artifact(dataset),
                    ["fixture manifest-entry.json does not match manifest entry"],
                )

            (evidence_dir / "manifest-entry.json").write_text(
                json.dumps(dataset),
                encoding="utf-8",
            )
            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                self.assertEqual(
                    verify_real_data_evidence.validate_manifest_entry_artifact(dataset),
                    [],
                )

    def test_release_candidate_requires_manifest_entry_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            evidence_dir = root / "benchmarks" / "real-data" / "fixture" / "evidence"
            evidence_dir.mkdir(parents=True)
            dataset = {
                "id": "fixture",
                "input_path": "benchmarks/real-data/fixture/input.bam",
                "evidence_json": "benchmarks/real-data/fixture/evidence/real-data-comparison.json",
                "evidence_markdown": "benchmarks/real-data/fixture/evidence/real-data-comparison.md",
                "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "sha256": "a" * 64,
                "scope_caveat": "small public fixture",
                "release_tier": "release_candidate",
                "minimum_input_bytes": 1_000_000,
                "expected_commands": {"ViewSam": "SAM record digest"},
            }

            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                self.assertEqual(
                    verify_real_data_evidence.validate_manifest_entry_artifact(dataset),
                    ["fixture release_candidate missing manifest-entry.json"],
                )

            dataset["release_tier"] = "public_smoke"
            with mock.patch.object(verify_real_data_evidence, "ROOT", root):
                self.assertEqual(
                    verify_real_data_evidence.validate_manifest_entry_artifact(dataset),
                    [],
                )

    def test_markduplicates_composite_digest_uses_sidecars(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            artifact = root / "turbo.bam"
            artifact.write_bytes(b"placeholder bam")
            artifact.with_suffix(".view.sam").write_text(
                "@HD\tVN:1.6\n"
                "read-b\t0\tchr1\t10\t60\t4M\t=\t40\t100\tACGT\tFFFF\tDS:i:2\n"
                "read-a\t1024\tchr1\t1\t60\t4M\t=\t10\t100\tACGT\tFFFF\t"
                "DT:Z:LB\tDS:i:2\tDI:i:1\tRX:Z:AAAA\n",
                encoding="utf-8",
            )
            artifact.with_name("turbo.metrics.txt").write_text(
                "# comment ignored\n\nLIBRARY\tUNPAIRED_READ_DUPLICATES\nunknown\t1\n",
                encoding="utf-8",
            )

            digest = verify_real_data_evidence.recomputable_artifact_digest(
                artifact,
                "duplicate-marking semantic digest plus stable metrics digest",
            )

            self.assertIsNotNone(digest)
            assert digest is not None
            duplicate_digest, metrics_digest = digest.split(";metrics=")
            self.assertRegex(duplicate_digest, r"^[0-9a-f]{64}$")
            self.assertRegex(metrics_digest, r"^[0-9a-f]{64}$")

            artifact.with_suffix(".view.sam").write_text(
                "@HD\tVN:1.6\n"
                "read-b\t1024\tchr1\t10\t60\t4M\t=\t40\t100\tACGT\tFFFF\tDS:i:2\n",
                encoding="utf-8",
            )
            self.assertNotEqual(
                digest,
                verify_real_data_evidence.recomputable_artifact_digest(
                    artifact,
                    "duplicate-marking semantic digest plus stable metrics digest",
                ),
            )

    def test_recomputable_digest_handles_validate_sam_summary_exit_code(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            summary = Path(tmp) / "summary.txt"
            summary.write_text(
                "# ignored\n\n"
                "## HISTOGRAM\tjava.lang.String\n"
                "Error Type\tCount\n"
                "ERROR:MATE_NOT_FOUND\t2\n",
                encoding="utf-8",
            )

            digest = verify_real_data_evidence.recomputable_artifact_digest(
                summary,
                "summary validation histogram plus exit code",
                2,
            )

            self.assertEqual(
                digest,
                verify_real_data_evidence.digest_validate_sam_summary(summary, 2),
            )
            self.assertNotEqual(
                digest,
                verify_real_data_evidence.recomputable_artifact_digest(
                    summary,
                    "summary validation histogram plus exit code",
                    0,
                ),
            )
            self.assertIsNone(
                verify_real_data_evidence.recomputable_artifact_digest(
                    summary,
                    "summary validation histogram plus exit code",
                )
            )

    def test_required_markdown_comparison_notes_tracks_digest_types(self) -> None:
        notes = verify_real_data_evidence.required_markdown_comparison_notes(
            set(verify_real_data_evidence.KNOWN_COMPARISONS)
        )
        text = "\n".join(needle for needle, _description in notes)

        self.assertIn("## Comparison details", text)
        self.assertEqual(
            set(verify_real_data_evidence.KNOWN_COMPARISONS),
            set(verify_real_data_evidence.COMPARISON_MARKDOWN_NOTES),
        )
        self.assertEqual(len(notes), len(set(notes)))
        self.assertEqual(
            len(notes),
            len(verify_real_data_evidence.KNOWN_COMPARISONS) + 1,
        )
        self.assertIn("exact BAM index bytes", text)
        self.assertIn("FASTQ outputs byte-for-byte", text)
        self.assertIn("ignores headers", text)
        self.assertIn("sorted @RG header fields", text)
        self.assertIn("after a BAM-writing command", text)
        self.assertIn("RevertSam rewrites aligned records", text)
        self.assertIn("tie-order differences", text)
        self.assertIn("generated headers do not affect parity", text)
        self.assertIn("duplicate flags", text)
        self.assertIn("same Picard and turbo-picard exit code", text)


if __name__ == "__main__":
    unittest.main()
