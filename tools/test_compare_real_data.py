import tempfile
import unittest
from pathlib import Path
import os
import sys
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import compare_real_data


class CompareRealDataTests(unittest.TestCase):
    def test_materialize_alignment_sam_uses_selected_view_entrypoint(self):
        with tempfile.TemporaryDirectory() as tmp:
            input_bam = Path(tmp) / "input.bam"
            output_sam = Path(tmp) / "output.sam"
            input_bam.write_bytes(b"BAM")

            with mock.patch.object(compare_real_data, "run", return_value=0.25) as run_mock:
                elapsed = compare_real_data.materialize_alignment_sam(
                    input_bam,
                    output_sam,
                    ["turbo-picard"],
                    None,
                )

            self.assertEqual(elapsed, 0.25)
            run_mock.assert_called_once_with(
                [
                    "turbo-picard",
                    "ViewSam",
                    f"I={input_bam}",
                    "VALIDATION_STRINGENCY=SILENT",
                    "QUIET=true",
                ],
                stdout=output_sam,
            )

    def test_materialize_alignment_sam_passes_reference_for_cram(self):
        with tempfile.TemporaryDirectory() as tmp:
            input_cram = Path(tmp) / "input.cram"
            reference = Path(tmp) / "reference.fa"
            output_sam = Path(tmp) / "output.sam"
            input_cram.write_bytes(b"CRAM")
            reference.write_text(">chr1\nACGT\n", encoding="utf-8")

            with mock.patch.object(compare_real_data, "run", return_value=0.5) as run_mock:
                compare_real_data.materialize_alignment_sam(
                    input_cram,
                    output_sam,
                    ["picard"],
                    reference,
                )

            run_mock.assert_called_once_with(
                [
                    "picard",
                    "ViewSam",
                    f"I={input_cram}",
                    f"R={reference}",
                    "VALIDATION_STRINGENCY=SILENT",
                    "QUIET=true",
                ],
                stdout=output_sam,
            )

    def test_markduplicates_argument_parser_accepts_options_and_regex_braces(self):
        argument = "READ_NAME_REGEX=(?:[A-Z]+:){4}([0-9]+)"
        self.assertEqual(compare_real_data.parse_markduplicates_arg(argument), argument)
        self.assertEqual(
            compare_real_data.parse_markduplicates_arg("BARCODE_TAG=RX"),
            "BARCODE_TAG=RX",
        )

    def test_markduplicates_argument_parser_reserves_io_fields(self):
        for argument in ("I=input.bam", "O=output.bam", "M=metrics.txt", "broken"):
            with self.assertRaises(Exception):
                compare_real_data.parse_markduplicates_arg(argument)

    def test_sam_record_digest_ignores_headers(self):
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.sam"
            second = Path(tmp) / "second.sam"
            first.write_text(
                "@HD\tVN:1.6\n@SQ\tSN:chr1\tLN:100\nread-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
                encoding="utf-8",
            )
            second.write_text(
                "@HD\tVN:1.5\n@PG\tID:picard\nread-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
                encoding="utf-8",
            )

            self.assertEqual(
                compare_real_data.digest_sam_records(first),
                compare_real_data.digest_sam_records(second),
            )

    def test_sam_record_digest_ignores_optional_tag_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.sam"
            second = Path(tmp) / "second.sam"
            first.write_text(
                "@HD\tVN:1.6\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\tMD:Z:4\tNM:i:0\tRG:Z:1\n",
                encoding="utf-8",
            )
            second.write_text(
                "@HD\tVN:1.6\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:1\tNM:i:0\tMD:Z:4\n",
                encoding="utf-8",
            )

            self.assertEqual(
                compare_real_data.digest_sam_records(first),
                compare_real_data.digest_sam_records(second),
            )

    def test_sam_record_and_read_group_digest_tracks_read_group_header(self):
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.sam"
            second = Path(tmp) / "second.sam"
            third = Path(tmp) / "third.sam"
            first.write_text(
                "@HD\tVN:1.6\n"
                "@RG\tID:rg1\tSM:sample\tPL:ILLUMINA\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
                encoding="utf-8",
            )
            second.write_text(
                "@HD\tVN:1.6\n"
                "@RG\tPL:ILLUMINA\tSM:sample\tID:rg1\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
                encoding="utf-8",
            )
            third.write_text(
                "@HD\tVN:1.6\n"
                "@RG\tID:rg1\tSM:other\tPL:ILLUMINA\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\tRG:Z:rg1\n",
                encoding="utf-8",
            )

            self.assertEqual(
                compare_real_data.digest_sam_records_and_read_groups(first),
                compare_real_data.digest_sam_records_and_read_groups(second),
            )
            self.assertNotEqual(
                compare_real_data.digest_sam_records_and_read_groups(first),
                compare_real_data.digest_sam_records_and_read_groups(third),
            )

    def test_sam_record_digest_normalizes_float_tag_representation(self):
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.sam"
            second = Path(tmp) / "second.sam"
            first.write_text(
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\trq:f:0.0\n",
                encoding="utf-8",
            )
            second.write_text(
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\trq:f:0\n",
                encoding="utf-8",
            )

            self.assertEqual(
                compare_real_data.digest_sam_records(first),
                compare_real_data.digest_sam_records(second),
            )

    def test_coordinate_sorted_multiset_digest_ignores_tie_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.sam"
            second = Path(tmp) / "second.sam"
            first.write_text(
                "@HD\tVN:1.6\tSO:coordinate\n"
                "@SQ\tSN:chr1\tLN:100\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n"
                "read-b\t0\tchr1\t1\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n",
                encoding="utf-8",
            )
            second.write_text(
                "@HD\tVN:1.6\tSO:coordinate\n"
                "@SQ\tSN:chr1\tLN:100\n"
                "read-b\t0\tchr1\t1\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
                encoding="utf-8",
            )

            self.assertEqual(
                compare_real_data.digest_coordinate_sorted_sam_multiset(first),
                compare_real_data.digest_coordinate_sorted_sam_multiset(second),
            )

    def test_coordinate_sorted_multiset_digest_rejects_unsorted_records(self):
        with tempfile.TemporaryDirectory() as tmp:
            sam = Path(tmp) / "unsorted.sam"
            sam.write_text(
                "@HD\tVN:1.6\tSO:coordinate\n"
                "@SQ\tSN:chr1\tLN:100\n"
                "read-b\t0\tchr1\t2\t60\t4M\t*\t0\t0\tTGCA\tFFFF\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
                encoding="utf-8",
            )

            with self.assertRaises(SystemExit):
                compare_real_data.digest_coordinate_sorted_sam_multiset(sam)

    def test_stable_text_digest_ignores_comments_and_blank_lines(self):
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.txt"
            second = Path(tmp) / "second.txt"
            first.write_text("# generated by Picard\n\nA\tB\n1\t2\n", encoding="utf-8")
            second.write_text("# generated by turbo-picard\nA\tB\n1\t2\n\n", encoding="utf-8")

            self.assertEqual(
                compare_real_data.digest_stable_text(first),
                compare_real_data.digest_stable_text(second),
            )

    def test_stable_text_digest_reports_missing_artifact(self):
        with tempfile.TemporaryDirectory() as tmp:
            missing = Path(tmp) / "missing.metrics.txt"

            self.assertEqual(
                compare_real_data.digest_stable_text_or_missing(
                    missing,
                    "Picard metrics",
                ),
                "missing:Picard metrics:missing.metrics.txt",
            )

    def test_digest_files_compares_ordered_file_contents(self):
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "r1.fastq"
            second = Path(tmp) / "r2.fastq"
            renamed = Path(tmp) / "other.fastq"
            first.write_text("@a\nA\n+\nF\n", encoding="utf-8")
            second.write_text("@a\nT\n+\nF\n", encoding="utf-8")
            renamed.write_text("@a\nA\n+\nF\n", encoding="utf-8")

            self.assertEqual(
                compare_real_data.digest_files([first, second]),
                compare_real_data.digest_files([first, second]),
            )
            self.assertEqual(
                compare_real_data.digest_files([first]),
                compare_real_data.digest_files([renamed]),
            )
            self.assertNotEqual(
                compare_real_data.digest_files([first, second]),
                compare_real_data.digest_files([second, first]),
            )

    def test_capture_version_accepts_picard_nonzero_version_output(self):
        completed = mock.Mock(returncode=1, stdout="Version:3.4.0\n")
        with mock.patch.object(compare_real_data.subprocess, "run", return_value=completed):
            self.assertEqual(compare_real_data.capture_version(["picard", "ViewSam", "--version"]), "Version:3.4.0")

    def test_capture_version_extracts_picard_version_after_warnings(self):
        completed = mock.Mock(
            returncode=1,
            stdout="WARNING startup detail\nVersion:3.4.0\n",
        )
        with mock.patch.object(compare_real_data.subprocess, "run", return_value=completed):
            self.assertEqual(
                compare_real_data.capture_version(["picard", "ViewSam", "--version"]),
                "Version:3.4.0",
            )

    def test_input_metadata_can_record_source_citation(self):
        with tempfile.TemporaryDirectory() as tmp:
            input_bam = Path(tmp) / "input.bam"
            input_bam.write_bytes(b"bam")

            metadata = compare_real_data.input_metadata(
                input_bam,
                "https://github.com/samtools/htslib/blob/0123456789abcdef0123456789abcdef01234567/test/range.bam",
                "0123456789abcdef0123456789abcdef01234567",
            )

            self.assertEqual(metadata["source_url"], "https://github.com/samtools/htslib/blob/0123456789abcdef0123456789abcdef01234567/test/range.bam")
            self.assertEqual(metadata["source_commit"], "0123456789abcdef0123456789abcdef01234567")

    def test_manifest_request_rejects_invalid_output_before_running_comparison(self):
        with tempfile.TemporaryDirectory() as tmp:
            input_bam = Path(tmp) / "input.bam"
            input_bam.write_bytes(b"bam")
            args = SimpleNamespace(
                dataset_id="example",
                output_dir=Path(tmp) / "evidence",
                input_source_url="https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                input_source_commit="0123456789abcdef0123456789abcdef01234567",
                input_bam=input_bam,
                release_tier="public_smoke",
                commands=["ViewSam"],
            )

            with self.assertRaisesRegex(
                SystemExit,
                "manifest output directory must be under benchmarks/real-data/<dataset-id>/evidence",
            ):
                compare_real_data.validate_manifest_request(args)

    def test_manifest_request_rejects_missing_citation_before_running_comparison(self):
        with tempfile.TemporaryDirectory() as tmp:
            input_bam = Path(tmp) / "input.bam"
            input_bam.write_bytes(b"bam")
            args = SimpleNamespace(
                dataset_id="example",
                output_dir=Path("benchmarks/real-data/example/evidence"),
                input_source_url=None,
                input_source_commit=None,
                input_bam=input_bam,
                release_tier="public_smoke",
                commands=["ViewSam"],
            )

            with self.assertRaisesRegex(
                SystemExit,
                "manifest entries require input citation fields",
            ):
                compare_real_data.validate_manifest_request(args)

    def test_manifest_request_accepts_valid_release_candidate_shape(self):
        args = SimpleNamespace(
            dataset_id="picard-snvq",
            output_dir=Path("benchmarks/real-data/picard-snvq/evidence"),
            input_source_url="https://github.com/broadinstitute/picard/blob/fc0b08410d38a10afd08e467dab74bf5e2e71310/testdata/picard/sam/snvq_metrics_test.bam",
            input_source_commit="fc0b08410d38a10afd08e467dab74bf5e2e71310",
            input_bam=Path("benchmarks/real-data/picard-snvq/input.bam"),
            release_tier="release_candidate",
            commands=sorted(compare_real_data.RELEASE_CANDIDATE_REQUIRED_COMMANDS),
        )

        compare_real_data.validate_manifest_request(args)

    def test_manifest_entry_is_generated_from_passing_evidence(self):
        summary = {
            "parity": "PASS",
            "input": {
                "path": "benchmarks/real-data/example/input.bam",
                "sha256": "a" * 64,
                "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "size_bytes": 100,
            },
            "commands": [
                {
                    "command": "ViewSam",
                    "status": "PASS",
                    "comparison": "SAM record digest",
                },
                {
                    "command": "CleanSam",
                    "status": "FAIL",
                    "comparison": "post-command SAM record digest",
                },
            ],
        }

        entry = compare_real_data.build_manifest_entry(
            summary=summary,
            dataset_id="example",
            evidence_json=Path("benchmarks/real-data/example/evidence/real-data-comparison.json"),
            evidence_markdown=Path("benchmarks/real-data/example/evidence/real-data-comparison.md"),
            scope_caveat="larger public shard",
            release_tier="public_smoke",
        )

        self.assertEqual(entry["id"], "example")
        self.assertEqual(entry["release_tier"], "public_smoke")
        self.assertEqual(entry["sha256"], "a" * 64)
        self.assertEqual(entry["source_commit"], "0123456789abcdef0123456789abcdef01234567")
        self.assertEqual(entry["expected_commands"], {"ViewSam": "SAM record digest"})
        self.assertNotIn("minimum_input_bytes", entry)

    def test_release_candidate_manifest_entry_requires_broad_passing_evidence(self):
        summary = {
            "parity": "PASS",
            "input": {
                "path": "benchmarks/real-data/example/input.bam",
                "sha256": "a" * 64,
                "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "size_bytes": 2_000_000,
            },
            "commands": [
                {
                    "command": "ViewSam",
                    "status": "PASS",
                    "comparison": "SAM record digest",
                }
            ],
        }

        with self.assertRaisesRegex(
            SystemExit,
            "release_candidate manifest entries require passing evidence for: "
            "CleanSam, CollectAlignmentSummaryMetrics, CollectQualityYieldMetrics, MarkDuplicates",
        ):
            compare_real_data.build_manifest_entry(
                summary=summary,
                dataset_id="example",
                evidence_json=Path("benchmarks/real-data/example/evidence/real-data-comparison.json"),
                evidence_markdown=Path("benchmarks/real-data/example/evidence/real-data-comparison.md"),
                scope_caveat="larger public shard",
                release_tier="release_candidate",
            )

    def test_release_candidate_manifest_entry_requires_large_input(self):
        summary = {
            "parity": "PASS",
            "input": {
                "path": "benchmarks/real-data/example/input.bam",
                "sha256": "a" * 64,
                "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "size_bytes": 100,
            },
            "commands": [
                {
                    "command": command,
                    "status": "PASS",
                    "comparison": comparison,
                }
                for command, comparison in {
                    "ViewSam": "SAM record digest",
                    "CleanSam": "post-command SAM record digest",
                    "CollectQualityYieldMetrics": "stable metrics digest",
                    "CollectAlignmentSummaryMetrics": "stable metrics digest",
                    "MarkDuplicates": "duplicate-marking semantic digest plus stable metrics digest",
                }.items()
            ],
        }

        with self.assertRaisesRegex(
            SystemExit,
            "release_candidate manifest entries require input size >= 1000000 bytes; got 100",
        ):
            compare_real_data.build_manifest_entry(
                summary=summary,
                dataset_id="example",
                evidence_json=Path("benchmarks/real-data/example/evidence/real-data-comparison.json"),
                evidence_markdown=Path("benchmarks/real-data/example/evidence/real-data-comparison.md"),
                scope_caveat="larger public shard",
                release_tier="release_candidate",
            )

    def test_release_candidate_manifest_entry_records_minimum_size_threshold(self):
        summary = {
            "parity": "PASS",
            "input": {
                "path": "benchmarks/real-data/example/input.bam",
                "sha256": "a" * 64,
                "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "size_bytes": 2_000_000,
            },
            "commands": [
                {
                    "command": command,
                    "status": "PASS",
                    "comparison": comparison,
                }
                for command, comparison in {
                    "ViewSam": "SAM record digest",
                    "CleanSam": "post-command SAM record digest",
                    "CollectQualityYieldMetrics": "stable metrics digest",
                    "CollectAlignmentSummaryMetrics": "stable metrics digest",
                    "MarkDuplicates": "duplicate-marking semantic digest plus stable metrics digest",
                }.items()
            ],
        }

        entry = compare_real_data.build_manifest_entry(
            summary=summary,
            dataset_id="example",
            evidence_json=Path("benchmarks/real-data/example/evidence/real-data-comparison.json"),
            evidence_markdown=Path("benchmarks/real-data/example/evidence/real-data-comparison.md"),
            scope_caveat="larger public shard",
            release_tier="release_candidate",
        )

        self.assertEqual(entry["minimum_input_bytes"], 1_000_000)

    def test_manifest_entry_rejects_paths_outside_real_data_tree(self):
        summary = {
            "parity": "PASS",
            "input": {
                "path": "/tmp/input.bam",
                "sha256": "a" * 64,
                "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "size_bytes": 100,
            },
            "commands": [
                {
                    "command": "ViewSam",
                    "status": "PASS",
                    "comparison": "SAM record digest",
                }
            ],
        }

        with self.assertRaisesRegex(
            SystemExit,
            "input path must be repository-relative under benchmarks/real-data",
        ):
            compare_real_data.build_manifest_entry(
                summary=summary,
                dataset_id="bad-path",
                evidence_json=Path("benchmarks/real-data/bad-path/evidence/real-data-comparison.json"),
                evidence_markdown=Path("benchmarks/real-data/bad-path/evidence/real-data-comparison.md"),
                scope_caveat="bad path",
                release_tier="public_smoke",
            )

    def test_manifest_entry_requires_evidence_subdirectory_outputs(self):
        summary = {
            "parity": "PASS",
            "input": {
                "path": "benchmarks/real-data/bad-layout/input.bam",
                "sha256": "a" * 64,
                "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "size_bytes": 100,
            },
            "commands": [
                {
                    "command": "ViewSam",
                    "status": "PASS",
                    "comparison": "SAM record digest",
                }
            ],
        }

        with self.assertRaisesRegex(
            SystemExit,
            "evidence JSON must be written under a dataset evidence/ directory",
        ):
            compare_real_data.build_manifest_entry(
                summary=summary,
                dataset_id="bad-layout",
                evidence_json=Path("benchmarks/real-data/bad-layout/real-data-comparison.json"),
                evidence_markdown=Path("benchmarks/real-data/bad-layout/real-data-comparison.md"),
                scope_caveat="bad layout",
                release_tier="public_smoke",
            )

    def test_manifest_entry_requires_comparator_output_filenames(self):
        summary = {
            "parity": "PASS",
            "input": {
                "path": "benchmarks/real-data/bad-name/input.bam",
                "sha256": "a" * 64,
                "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "size_bytes": 100,
            },
            "commands": [
                {
                    "command": "ViewSam",
                    "status": "PASS",
                    "comparison": "SAM record digest",
                }
            ],
        }

        with self.assertRaisesRegex(
            SystemExit,
            "evidence JSON must be named benchmarks/real-data/bad-name/evidence/real-data-comparison.json",
        ):
            compare_real_data.build_manifest_entry(
                summary=summary,
                dataset_id="bad-name",
                evidence_json=Path("benchmarks/real-data/bad-name/evidence/evidence.json"),
                evidence_markdown=Path("benchmarks/real-data/bad-name/evidence/real-data-comparison.md"),
                scope_caveat="bad name",
                release_tier="public_smoke",
            )

    def test_manifest_entry_requires_source_citation(self):
        summary = {
            "parity": "PASS",
            "input": {"path": "benchmarks/real-data/missing-citation/input.bam", "sha256": "a" * 64},
            "commands": [],
        }

        with self.assertRaises(SystemExit):
            compare_real_data.build_manifest_entry(
                summary=summary,
                dataset_id="missing-citation",
                evidence_json=Path("benchmarks/real-data/missing-citation/evidence/real-data-comparison.json"),
                evidence_markdown=Path("benchmarks/real-data/missing-citation/evidence/real-data-comparison.md"),
                scope_caveat="missing citation",
                release_tier="public_smoke",
            )

    def test_manifest_entry_rejects_malformed_or_duplicate_command_rows(self):
        base_summary = {
            "parity": "PASS",
            "input": {
                "path": "benchmarks/real-data/bad-commands/input.bam",
                "sha256": "a" * 64,
                "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
                "size_bytes": 100,
            },
        }

        with self.assertRaisesRegex(SystemExit, "comparison summary commands must be a list"):
            compare_real_data.build_manifest_entry(
                summary={**base_summary, "commands": "not-a-list"},
                dataset_id="bad-commands",
                evidence_json=Path("benchmarks/real-data/bad-commands/evidence/real-data-comparison.json"),
                evidence_markdown=Path("benchmarks/real-data/bad-commands/evidence/real-data-comparison.md"),
                scope_caveat="bad commands",
                release_tier="public_smoke",
            )
        with self.assertRaisesRegex(SystemExit, "comparison summary command row 0 must be an object"):
            compare_real_data.build_manifest_entry(
                summary={**base_summary, "commands": ["not-a-row"]},
                dataset_id="bad-commands",
                evidence_json=Path("benchmarks/real-data/bad-commands/evidence/real-data-comparison.json"),
                evidence_markdown=Path("benchmarks/real-data/bad-commands/evidence/real-data-comparison.md"),
                scope_caveat="bad commands",
                release_tier="public_smoke",
            )
        with self.assertRaisesRegex(SystemExit, "comparison summary has duplicate command evidence: ViewSam"):
            compare_real_data.build_manifest_entry(
                summary={
                    **base_summary,
                    "commands": [
                        {"command": "ViewSam", "status": "PASS", "comparison": "SAM record digest"},
                        {"command": "ViewSam", "status": "PASS", "comparison": "different digest"},
                    ],
                },
                dataset_id="bad-commands",
                evidence_json=Path("benchmarks/real-data/bad-commands/evidence/real-data-comparison.json"),
                evidence_markdown=Path("benchmarks/real-data/bad-commands/evidence/real-data-comparison.md"),
                scope_caveat="bad commands",
                release_tier="public_smoke",
            )
        with self.assertRaisesRegex(SystemExit, "comparison summary command ViewSam missing comparison"):
            compare_real_data.build_manifest_entry(
                summary={
                    **base_summary,
                    "commands": [{"command": "ViewSam", "status": "PASS"}],
                },
                dataset_id="bad-commands",
                evidence_json=Path("benchmarks/real-data/bad-commands/evidence/real-data-comparison.json"),
                evidence_markdown=Path("benchmarks/real-data/bad-commands/evidence/real-data-comparison.md"),
                scope_caveat="bad commands",
                release_tier="public_smoke",
            )

    def test_manifest_entry_rejects_short_github_source_commit(self):
        summary = {
            "parity": "PASS",
            "input": {
                "path": "benchmarks/real-data/short-commit/input.bam",
                "sha256": "a" * 64,
                "source_url": "https://github.com/example/repo/blob/abc/input.bam",
                "source_commit": "abc",
                "size_bytes": 100,
            },
            "commands": [
                {
                    "command": "ViewSam",
                    "status": "PASS",
                    "comparison": "SAM record digest",
                }
            ],
        }

        with self.assertRaisesRegex(
            SystemExit,
            "short-commit GitHub source_commit must be a full 40-character SHA",
        ):
            compare_real_data.build_manifest_entry(
                summary=summary,
                dataset_id="short-commit",
                evidence_json=Path("benchmarks/real-data/short-commit/evidence/real-data-comparison.json"),
                evidence_markdown=Path("benchmarks/real-data/short-commit/evidence/real-data-comparison.md"),
                scope_caveat="short commit",
                release_tier="public_smoke",
            )

    def test_manifest_entry_rejects_non_github_source_without_identifier_in_url(self):
        summary = {
            "parity": "PASS",
            "input": {
                "path": "benchmarks/real-data/accession/input.bam",
                "sha256": "a" * 64,
                "source_url": "https://example.org/datasets/input.bam",
                "source_commit": "GIAB-HG001-v4.2.1",
                "size_bytes": 100,
            },
            "commands": [
                {
                    "command": "ViewSam",
                    "status": "PASS",
                    "comparison": "SAM record digest",
                }
            ],
        }

        with self.assertRaisesRegex(
            SystemExit,
            "accession non-GitHub source_url must include source_commit/accession identifier",
        ):
            compare_real_data.build_manifest_entry(
                summary=summary,
                dataset_id="accession",
                evidence_json=Path("benchmarks/real-data/accession/evidence/real-data-comparison.json"),
                evidence_markdown=Path("benchmarks/real-data/accession/evidence/real-data-comparison.md"),
                scope_caveat="accession source",
                release_tier="public_smoke",
            )

    def test_manifest_entry_rejects_failing_comparison(self):
        summary = {
            "parity": "FAIL",
            "input": {
                "path": "benchmarks/real-data/failed/input.bam",
                "sha256": "a" * 64,
                "source_url": "https://github.com/example/repo/blob/0123456789abcdef0123456789abcdef01234567/input.bam",
                "source_commit": "0123456789abcdef0123456789abcdef01234567",
            },
            "commands": [],
        }

        with self.assertRaises(SystemExit):
            compare_real_data.build_manifest_entry(
                summary=summary,
                dataset_id="failed",
                evidence_json=Path("benchmarks/real-data/failed/evidence/real-data-comparison.json"),
                evidence_markdown=Path("benchmarks/real-data/failed/evidence/real-data-comparison.md"),
                scope_caveat="failed",
                release_tier="public_smoke",
            )

    def test_compare_command_supports_collect_insert_size_metrics(self):
        with tempfile.TemporaryDirectory() as tmp:
            work_root = Path(tmp)
            input_bam = work_root / "input.bam"
            input_bam.write_bytes(b"bam")
            expected = compare_real_data.CommandEvidence(
                command="CollectInsertSizeMetrics",
                status="PASS",
                turbo_seconds=1.0,
                picard_seconds=2.0,
                speedup=2.0,
                comparison="stable metrics digest with insert-size histogram",
                turbo_artifact="turbo.metrics.txt",
                picard_artifact="picard.metrics.txt",
                turbo_digest="abc",
                picard_digest="abc",
            )

            with mock.patch.object(
                compare_real_data,
                "compare_insert_size_metrics",
                return_value=expected,
            ) as mocked:
                observed = compare_real_data.compare_command(
                    "CollectInsertSizeMetrics",
                    input_bam,
                    work_root,
                    ["turbo-picard"],
                    ["picard"],
                    100,
                    None,
                    input_bam,
                )

            self.assertEqual(observed, expected)
            mocked.assert_called_once_with(
                input_bam,
                work_root / "CollectInsertSizeMetrics",
                ["turbo-picard"],
                ["picard"],
                ["STOP_AFTER=100"],
                None,
            )

    def test_compare_command_passes_explicit_markduplicates_options(self):
        with tempfile.TemporaryDirectory() as tmp:
            work_root = Path(tmp)
            input_bam = work_root / "input.bam"
            input_bam.write_bytes(b"bam")
            expected = compare_real_data.CommandEvidence(
                command="MarkDuplicates",
                status="PASS",
                turbo_seconds=1.0,
                picard_seconds=2.0,
                speedup=2.0,
                comparison="duplicate-marking semantic digest plus stable metrics digest",
                turbo_artifact="turbo.bam",
                picard_artifact="picard.bam",
                turbo_digest="abc",
                picard_digest="abc",
            )

            with mock.patch.object(
                compare_real_data,
                "compare_bam_output",
                return_value=expected,
            ) as mocked:
                observed = compare_real_data.compare_command(
                    "MarkDuplicates",
                    input_bam,
                    work_root,
                    ["turbo-picard"],
                    ["picard"],
                    None,
                    None,
                    input_bam,
                    [
                        "BARCODE_TAG=RX",
                        "READ_NAME_REGEX=(?:[A-Z]+:){4}([0-9]+)",
                    ],
                )

            self.assertEqual(observed, expected)
            mocked.assert_called_once_with(
                "MarkDuplicates",
                input_bam,
                work_root / "MarkDuplicates",
                ["turbo-picard"],
                ["picard"],
                [
                    "M={metrics}",
                    "BARCODE_TAG=RX",
                    "READ_NAME_REGEX=(?:[A-Z]+:){4}([0-9]+)",
                ],
                None,
            )

    def test_compare_command_supports_validate_sam_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            work_root = Path(tmp)
            input_bam = work_root / "input.bam"
            input_bam.write_bytes(b"bam")
            expected = compare_real_data.CommandEvidence(
                command="ValidateSamFile",
                status="PASS",
                turbo_seconds=1.0,
                picard_seconds=2.0,
                speedup=2.0,
                comparison="summary validation histogram plus exit code",
                turbo_artifact="turbo.summary.txt",
                picard_artifact="picard.summary.txt",
                turbo_digest="abc",
                picard_digest="abc",
            )

            with mock.patch.object(
                compare_real_data,
                "compare_validate_sam_file",
                return_value=expected,
            ) as mocked:
                observed = compare_real_data.compare_command(
                    "ValidateSamFile",
                    input_bam,
                    work_root,
                    ["turbo-picard"],
                    ["picard"],
                    None,
                    None,
                    input_bam,
                )

            self.assertEqual(observed, expected)
            mocked.assert_called_once_with(
                input_bam,
                work_root / "ValidateSamFile",
                ["turbo-picard"],
                ["picard"],
                None,
            )

    def test_compare_command_supports_build_bam_index(self):
        with tempfile.TemporaryDirectory() as tmp:
            work_root = Path(tmp)
            input_bam = work_root / "input.bam"
            input_bam.write_bytes(b"bam")
            expected = compare_real_data.CommandEvidence(
                command="BuildBamIndex",
                status="PASS",
                turbo_seconds=1.0,
                picard_seconds=2.0,
                speedup=2.0,
                comparison="BAI binary digest",
                turbo_artifact="turbo.bai",
                picard_artifact="picard.bai",
                turbo_digest="abc",
                picard_digest="abc",
            )

            with mock.patch.object(
                compare_real_data,
                "compare_build_bam_index",
                return_value=expected,
            ) as mocked:
                observed = compare_real_data.compare_command(
                    "BuildBamIndex",
                    input_bam,
                    work_root,
                    ["turbo-picard"],
                    ["picard"],
                    None,
                    None,
                    input_bam,
                )

            self.assertEqual(observed, expected)
            mocked.assert_called_once_with(
                input_bam,
                work_root / "BuildBamIndex",
                ["turbo-picard"],
                ["picard"],
                None,
            )

    def test_compare_command_supports_add_or_replace_read_groups(self):
        with tempfile.TemporaryDirectory() as tmp:
            work_root = Path(tmp)
            input_bam = work_root / "input.bam"
            input_bam.write_bytes(b"bam")
            expected = compare_real_data.CommandEvidence(
                command="AddOrReplaceReadGroups",
                status="PASS",
                turbo_seconds=1.0,
                picard_seconds=2.0,
                speedup=2.0,
                comparison="SAM record digest plus read-group header digest",
                turbo_artifact="turbo.bam",
                picard_artifact="picard.bam",
                turbo_digest="abc",
                picard_digest="abc",
            )

            with mock.patch.object(
                compare_real_data,
                "compare_add_or_replace_read_groups",
                return_value=expected,
            ) as mocked:
                observed = compare_real_data.compare_command(
                    "AddOrReplaceReadGroups",
                    input_bam,
                    work_root,
                    ["turbo-picard"],
                    ["picard"],
                    None,
                    None,
                    input_bam,
                )

            self.assertEqual(observed, expected)
            mocked.assert_called_once_with(
                input_bam,
                work_root / "AddOrReplaceReadGroups",
                ["turbo-picard"],
                ["picard"],
                None,
            )

    def test_compare_command_supports_revertsam(self):
        with tempfile.TemporaryDirectory() as tmp:
            work_root = Path(tmp)
            input_bam = work_root / "input.bam"
            input_bam.write_bytes(b"bam")
            expected = compare_real_data.CommandEvidence(
                command="RevertSam",
                status="PASS",
                turbo_seconds=1.0,
                picard_seconds=2.0,
                speedup=2.0,
                comparison="reverted SAM record digest",
                turbo_artifact="turbo.bam",
                picard_artifact="picard.bam",
                turbo_digest="abc",
                picard_digest="abc",
            )

            with mock.patch.object(
                compare_real_data,
                "compare_revertsam",
                return_value=expected,
            ) as mocked:
                observed = compare_real_data.compare_command(
                    "RevertSam",
                    input_bam,
                    work_root,
                    ["turbo-picard"],
                    ["picard"],
                    None,
                    None,
                    input_bam,
                )

            self.assertEqual(observed, expected)
            mocked.assert_called_once_with(
                input_bam,
                work_root / "RevertSam",
                ["turbo-picard"],
                ["picard"],
                None,
            )

    def test_compare_command_supports_samtofastq(self):
        with tempfile.TemporaryDirectory() as tmp:
            work_root = Path(tmp)
            input_bam = work_root / "input.bam"
            input_bam.write_bytes(b"bam")
            expected = compare_real_data.CommandEvidence(
                command="SamToFastq",
                status="PASS",
                turbo_seconds=1.0,
                picard_seconds=2.0,
                speedup=2.0,
                comparison="FASTQ trio digest",
                turbo_artifact="turbo-r1.fastq",
                picard_artifact="picard-r1.fastq",
                turbo_digest="abc",
                picard_digest="abc",
            )

            with mock.patch.object(
                compare_real_data,
                "compare_samtofastq",
                return_value=expected,
            ) as mocked:
                observed = compare_real_data.compare_command(
                    "SamToFastq",
                    input_bam,
                    work_root,
                    ["turbo-picard"],
                    ["picard"],
                    None,
                    None,
                    input_bam,
                )

            self.assertEqual(observed, expected)
            mocked.assert_called_once_with(
                input_bam,
                work_root / "SamToFastq",
                ["turbo-picard"],
                ["picard"],
                None,
            )

    def test_validate_sam_summary_digest_includes_exit_code(self):
        with tempfile.TemporaryDirectory() as tmp:
            summary = Path(tmp) / "summary.txt"
            summary.write_text(
                "\n# generated\n\n"
                "## HISTOGRAM\tjava.lang.String\n"
                "Error Type\tCount\n"
                "WARNING:MISSING_TAG_NM\t2\n",
                encoding="utf-8",
            )

            ok_digest = compare_real_data.digest_validate_sam_summary(summary, 0)
            failing_digest = compare_real_data.digest_validate_sam_summary(summary, 2)

            self.assertNotEqual(ok_digest, failing_digest)
            self.assertEqual(
                ok_digest,
                compare_real_data.digest_validate_sam_summary(summary, 0),
            )

    def test_command_evidence_dict_omits_missing_exit_codes(self):
        row = compare_real_data.CommandEvidence(
            command="ViewSam",
            status="PASS",
            turbo_seconds=1.0,
            picard_seconds=2.0,
            speedup=2.0,
            comparison="SAM record digest",
            turbo_artifact="turbo.sam",
            picard_artifact="picard.sam",
            turbo_digest="abc",
            picard_digest="abc",
        )

        self.assertNotIn("turbo_exit_code", compare_real_data.command_evidence_dict(row))
        self.assertNotIn("picard_exit_code", compare_real_data.command_evidence_dict(row))

    def test_command_evidence_dict_rewrites_artifacts_relative_to_repo(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            artifact = root / "benchmarks/real-data/fixture/evidence/turbo.sam"
            artifact.parent.mkdir(parents=True, exist_ok=True)
            artifact.write_text("", encoding="utf-8")
            row = compare_real_data.CommandEvidence(
                command="ViewSam",
                status="PASS",
                turbo_seconds=1.0,
                picard_seconds=2.0,
                speedup=2.0,
                comparison="SAM record digest",
                turbo_artifact=str(artifact),
                picard_artifact=str(artifact),
                turbo_digest="abc",
                picard_digest="abc",
            )
            with mock.patch.object(compare_real_data, "ROOT", root):
                data = compare_real_data.command_evidence_dict(row)
            self.assertFalse(Path(data["turbo_artifact"]).is_absolute())
            self.assertFalse(Path(data["picard_artifact"]).is_absolute())
            self.assertTrue(data["turbo_artifact"].startswith("benchmarks/real-data/"))

    def test_command_evidence_dict_keeps_validation_exit_codes(self):
        row = compare_real_data.CommandEvidence(
            command="ValidateSamFile",
            status="PASS",
            turbo_seconds=1.0,
            picard_seconds=2.0,
            speedup=2.0,
            comparison="summary validation histogram plus exit code",
            turbo_artifact="turbo.summary.txt",
            picard_artifact="picard.summary.txt",
            turbo_digest="abc",
            picard_digest="abc",
            turbo_exit_code=2,
            picard_exit_code=2,
        )

        self.assertEqual(compare_real_data.command_evidence_dict(row)["turbo_exit_code"], 2)
        self.assertEqual(compare_real_data.command_evidence_dict(row)["picard_exit_code"], 2)

    def test_comparison_detail_lines_explain_review_digests(self):
        lines = compare_real_data.comparison_detail_lines(
            [
                {"comparison": "SAM record digest"},
                {"comparison": "reverted SAM record digest"},
                {"comparison": "FASTQ trio digest"},
                {"comparison": "SAM record digest plus read-group header digest"},
                {"comparison": "coordinate-sorted SAM record multiset digest"},
                {"comparison": "stable metrics digest with insert-size histogram"},
                {
                    "comparison": "duplicate-marking semantic digest plus stable metrics digest"
                },
                {"comparison": "summary validation histogram plus exit code"},
            ]
        )
        text = "\n".join(lines)

        self.assertIn("ignores headers", text)
        self.assertIn("rewrites aligned records to unmapped output", text)
        self.assertIn("FASTQ outputs byte-for-byte", text)
        self.assertIn("sorted @RG header fields", text)
        self.assertIn("allowing tie-order differences", text)
        self.assertIn("generated headers do not affect parity", text)
        self.assertIn("duplicate flags", text)
        self.assertIn("requires the same Picard and turbo-picard exit code", text)

    def test_artifact_digest_lines_list_artifacts_and_compact_digest(self):
        digest = "0123456789abcdef" * 4
        lines = compare_real_data.artifact_digest_lines(
            [
                {
                    "command": "ViewSam",
                    "turbo_artifact": "work/ViewSam/turbo.sam",
                    "picard_artifact": "work/ViewSam/picard.sam",
                    "turbo_digest": digest,
                    "picard_digest": digest,
                }
            ]
        )
        text = "\n".join(lines)

        self.assertIn("turbo-picard artifact", text)
        self.assertIn("work/ViewSam/turbo.sam", text)
        self.assertIn("0123456789ab...456789abcdef", text)
        self.assertIn("n/a", text)

    def test_artifact_digest_lines_include_validation_exit_codes(self):
        lines = compare_real_data.artifact_digest_lines(
            [
                {
                    "command": "ValidateSamFile",
                    "turbo_artifact": "work/ValidateSamFile/turbo.summary.txt",
                    "picard_artifact": "work/ValidateSamFile/picard.summary.txt",
                    "turbo_digest": "abc",
                    "picard_digest": "abc",
                    "turbo_exit_code": 2,
                    "picard_exit_code": 2,
                }
            ]
        )

        self.assertIn("turbo-picard `2`, Picard `2`", "\n".join(lines))

    def test_digest_summary_marks_mismatches(self):
        self.assertEqual(compare_real_data.digest_summary("a", "b"), "mismatch")

    def test_rscript_shim_env_prepends_workdir_to_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            workdir = Path(tmp)
            with mock.patch.dict(os.environ, {"PATH": "/usr/bin"}, clear=True):
                env = compare_real_data.rscript_shim_env(workdir)

            self.assertEqual(env["PATH"], f"{workdir}{os.pathsep}/usr/bin")

    def test_mamba_picard_prefix_injects_rscript_path_inside_run_command(self):
        with tempfile.TemporaryDirectory() as tmp:
            workdir = Path(tmp)
            prefix = ["/opt/homebrew/bin/mamba", "run", "-p", ".conda", "picard"]
            with mock.patch.dict(os.environ, {"PATH": "/usr/bin"}, clear=True):
                observed = compare_real_data.picard_prefix_with_rscript_shim(
                    prefix,
                    workdir,
                )

            self.assertEqual(
                observed,
                [
                    "/opt/homebrew/bin/mamba",
                    "run",
                    "-p",
                    ".conda",
                    "env",
                    f"PATH={workdir}{os.pathsep}.conda/bin{os.pathsep}/usr/bin",
                    "picard",
                ],
            )

    def test_non_mamba_picard_prefix_uses_process_environment(self):
        prefix = ["/usr/local/bin/picard"]
        self.assertEqual(
            compare_real_data.picard_prefix_with_rscript_shim(prefix, Path("/tmp")),
            prefix,
        )

    def test_write_fake_rscript_creates_executable_success_script(self):
        with tempfile.TemporaryDirectory() as tmp:
            script = Path(tmp) / "Rscript"
            compare_real_data.write_fake_rscript(script)

            self.assertTrue(script.exists())
            self.assertIn("exit 0", script.read_text(encoding="utf-8"))
            self.assertTrue(os.access(script, os.X_OK))

    def test_alignment_io_args_adds_reference_for_cram(self):
        with tempfile.TemporaryDirectory() as tmp:
            cram = Path(tmp) / "reads.cram"
            reference = Path(tmp) / "ref.fa"
            cram.write_text("", encoding="utf-8")
            reference.write_text(">chr1\nACGT\n", encoding="utf-8")
            self.assertEqual(
                compare_real_data.alignment_io_args(cram, reference),
                [f"I={cram}", f"R={reference}"],
            )

    def test_alignment_io_args_requires_reference_for_cram(self):
        with tempfile.TemporaryDirectory() as tmp:
            cram = Path(tmp) / "reads.cram"
            cram.write_text("", encoding="utf-8")
            with self.assertRaises(SystemExit):
                compare_real_data.alignment_io_args(cram, None)

    def test_cram_reference_arg_adds_reference_when_any_path_is_cram(self):
        with tempfile.TemporaryDirectory() as tmp:
            bam = Path(tmp) / "reads.bam"
            cram = Path(tmp) / "reads.cram"
            reference = Path(tmp) / "ref.fa"
            bam.write_text("", encoding="utf-8")
            cram.write_text("", encoding="utf-8")
            reference.write_text(">chr1\nACGT\n", encoding="utf-8")
            self.assertEqual(
                compare_real_data.cram_reference_arg(reference, bam, bam),
                [],
            )
            self.assertEqual(
                compare_real_data.cram_reference_arg(reference, bam, cram),
                [f"R={reference}"],
            )

    def test_reference_io_args_adds_reference_for_bam(self):
        with tempfile.TemporaryDirectory() as tmp:
            bam = Path(tmp) / "reads.bam"
            reference = Path(tmp) / "ref.fa"
            bam.write_text("", encoding="utf-8")
            reference.write_text(">chr1\nACGT\n", encoding="utf-8")
            self.assertEqual(
                compare_real_data.reference_io_args(bam, reference),
                [f"I={bam}", f"R={reference}"],
            )

    def test_require_reference_fasta_rejects_missing_path(self):
        with self.assertRaisesRegex(SystemExit, "SetNmMdAndUqTags requires --reference-fasta"):
            compare_real_data.require_reference_fasta(None, "SetNmMdAndUqTags")

    def test_digest_stable_sam_ignores_pg_header_lines(self):
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.sam"
            second = Path(tmp) / "second.sam"
            first.write_text(
                "@HD\tVN:1.6\tSO:coordinate\n"
                "@PG\tID:picard\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\tPG:Z:picard\n",
                encoding="utf-8",
            )
            second.write_text(
                "@HD\tVN:1.5\tSO:coordinate\n"
                "@PG\tID:turbo\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\tPG:Z:turbo\n",
                encoding="utf-8",
            )
            self.assertEqual(
                compare_real_data.digest_stable_sam(first),
                compare_real_data.digest_stable_sam(second),
            )

    def test_digest_stable_sam_normalizes_read_group_datetime_offsets(self):
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.sam"
            second = Path(tmp) / "second.sam"
            first.write_text(
                "@HD\tVN:1.6\tSO:coordinate\n"
                "@RG\tID:rg1\tDT:2016-02-04T05:00:00+0000\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
                encoding="utf-8",
            )
            second.write_text(
                "@HD\tVN:1.6\tSO:coordinate\n"
                "@RG\tID:rg1\tDT:2016-02-04T00:00:00-0500\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
                encoding="utf-8",
            )
            self.assertEqual(
                compare_real_data.digest_stable_sam(first),
                compare_real_data.digest_stable_sam(second),
            )

    def test_digest_replace_sam_header_tracks_header_and_record_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.sam"
            second = Path(tmp) / "second.sam"
            first.write_text(
                "@HD\tVN:1.6\n"
                "@CO\treplacement\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n"
                "read-b\t0\tchr1\t2\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
                encoding="utf-8",
            )
            second.write_text(
                "@HD\tVN:1.6\n"
                "@CO\treplacement\n"
                "read-b\t0\tchr1\t2\t60\t4M\t*\t0\t0\tACGT\tFFFF\n"
                "read-a\t0\tchr1\t1\t60\t4M\t*\t0\t0\tACGT\tFFFF\n",
                encoding="utf-8",
            )
            self.assertNotEqual(
                compare_real_data.digest_replace_sam_header(first),
                compare_real_data.digest_replace_sam_header(second),
            )

    def test_input_metadata_records_reference_for_cram(self):
        with tempfile.TemporaryDirectory() as tmp:
            cram = Path(tmp) / "reads.cram"
            reference = Path(tmp) / "ref.fa"
            cram.write_bytes(b"\x00")
            reference.write_text(">chr1\nACGT\n", encoding="utf-8")
            metadata = compare_real_data.input_metadata(cram, reference_fasta=reference)
            self.assertEqual(metadata["format"], "CRAM")
            self.assertEqual(metadata["reference_fasta"], str(reference))
            self.assertIn("reference_sha256", metadata)

    def test_markduplicates_semantic_digest_ignores_incidental_tags(self):
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.sam"
            second = Path(tmp) / "second.sam"
            first.write_text(
                "@HD\tVN:1.6\n"
                "@SQ\tSN:chr1\tLN:100\n"
                "read-a\t1024\tchr1\t1\t60\t4M\t=\t10\t100\tACGT\tFFFF\tPG:Z:markdup\tDT:Z:LB\tzz:Z:picard\n",
                encoding="utf-8",
            )
            second.write_text(
                "@HD\tVN:1.6\n"
                "@SQ\tSN:chr1\tLN:100\n"
                "read-a\t1024\tchr1\t1\t60\t4M\t=\t10\t100\tACGT\tFFFF\tPG:Z:markdup\tDT:Z:LB\tzz:Z:turbo\n",
                encoding="utf-8",
            )

            self.assertNotEqual(
                compare_real_data.digest_sam_records(first),
                compare_real_data.digest_sam_records(second),
            )
            self.assertEqual(
                compare_real_data.digest_markduplicates_semantics(first),
                compare_real_data.digest_markduplicates_semantics(second),
            )

    def test_shareable_report_omits_private_paths_hashes_and_arguments(self):
        with tempfile.TemporaryDirectory() as tmp:
            report = Path(tmp) / "shareable-trial-report.md"
            summary = {
                "parity": "PASS",
                "turbo_picard_version": "turbo-picard 0.1.11",
                "picard_version": "Version:3.4.0",
                "input": {
                    "path": "/private/clinical/cohort.bam",
                    "format": "BAM",
                    "size_bytes": 2 * 1024 * 1024,
                    "sha256": "private-input-hash",
                    "source_url": "https://private.example/cohort.bam",
                    "source_commit": "private-revision",
                },
                "commands": [
                    {
                        "command": "MarkDuplicates",
                        "status": "PASS",
                        "comparison": "duplicate-marking semantic digest",
                        "turbo_seconds": 1.25,
                        "picard_seconds": 5.0,
                        "speedup": 4.0,
                        "turbo_artifact": "/private/turbo/marked.bam",
                        "picard_artifact": "/private/picard/marked.bam",
                    }
                ],
            }

            compare_real_data.write_shareable_markdown(report, summary)
            text = report.read_text(encoding="utf-8")

            self.assertIn("Overall parity: `PASS`", text)
            self.assertIn("Input shape: `BAM`, about 2.0 MiB", text)
            self.assertIn("MarkDuplicates", text)
            self.assertIn("4.00x", text)
            self.assertNotIn("/private/clinical/cohort.bam", text)
            self.assertNotIn("private-input-hash", text)
            self.assertNotIn("private.example", text)
            self.assertNotIn("private-revision", text)
            self.assertNotIn("marked.bam", text)

    def test_shareable_report_can_include_explicitly_public_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            report = Path(tmp) / "shareable-trial-report.md"
            summary = {
                "parity": "PASS",
                "turbo_picard_version": "turbo-picard 0.1.11",
                "picard_version": "Version:3.4.0",
                "input": {
                    "format": "CRAM",
                    "size_bytes": 512,
                    "source_url": "https://example.org/reads/fixture-abc.cram",
                    "source_commit": "fixture-abc",
                },
                "commands": [],
            }

            compare_real_data.write_shareable_markdown(
                report,
                summary,
                include_public_source=True,
            )
            text = report.read_text(encoding="utf-8")

            self.assertIn("https://example.org/reads/fixture-abc.cram", text)
            self.assertIn("fixture-abc", text)
            self.assertIn("Input shape: `CRAM`, about 512 bytes", text)


if __name__ == "__main__":
    unittest.main()
