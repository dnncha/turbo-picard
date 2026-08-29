#!/usr/bin/env python3
"""Tests for the production evidence manifest adapter."""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("build_production_manifest.py")
SPEC = importlib.util.spec_from_file_location("build_production_manifest", MODULE_PATH)
assert SPEC is not None
build_production_manifest = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(build_production_manifest)


def tool_report(version: str, command: str) -> dict[str, object]:
    return {
        "status": "complete",
        "version": {"text": version, "exit_code": 0},
        "command_template": [command, "MarkDuplicates", "I={input}", "O={output}"],
        "environment_template": {"TURBO_PICARD_THREADS": "4"},
        "summary": {
            "successful_repeats": 5,
            "wall_seconds": {"median": 2.0, "p95": 2.5, "min": 1.8, "max": 2.7},
            "peak_rss_bytes": {"median": 100.0, "p95": 110.0, "min": 90.0, "max": 120.0},
            "temporary_disk_peak_bytes": {"median": 200.0, "p95": 220.0, "min": 180.0, "max": 240.0},
        },
        "parity": {"status": "REFERENCE" if command == "picard" else "PASS", "comparator": "exact"},
    }


def report_fixture() -> dict[str, object]:
    return {
        "schema_version": 1,
        "input": {
            "path": "/tmp/HG002.bam",
            "format": "BAM",
            "bytes": 1234,
            "sha256": "a" * 64,
            "source_url": "https://example.org/HG002.bam",
            "source_revision": "accession-123",
        },
        "protocol": {"threads": 4, "repeats": 5, "warmups": 1, "profile": "wgs_30x"},
        "host": {
            "os": "Linux",
            "architecture": "x86_64",
            "cpu_model": "test CPU",
            "logical_cpus": 8,
            "memory_bytes": 1024,
            "storage_note": "ext4 on test disk",
        },
        "tools": {
            "turbo-picard": tool_report("turbo-picard 0.1.11", "/repo/picard"),
            "picard": tool_report("Picard version 3.4.0", "/env/bin/picard"),
        },
    }


def args_fixture(**overrides: object) -> Namespace:
    values = {
        "dataset_id": "HG002-markduplicates",
        "scope_caveat": "coordinate-sorted HG002 shard only",
        "turbo_picard_commit": "b" * 40,
        "read_count": 100,
        "tier": "production_scale",
        "compatibility_level": "B",
        "independent_status": "not_run",
        "reviewer": None,
        "independent_host_profile": None,
        "independent_turbo_picard_commit": None,
        "independent_input_sha256": None,
        "independent_arguments_sha256": None,
        "evidence_url": None,
        "reference_fasta_sha256": None,
    }
    values.update(overrides)
    return Namespace(**values)


class BuildProductionManifestTests(unittest.TestCase):
    def test_builds_manifest_with_resource_and_provenance_fields(self) -> None:
        manifest = build_production_manifest.build_manifest(args_fixture(), report_fixture())

        self.assertEqual(manifest["tier"], "production_scale")
        self.assertEqual(manifest["input"]["read_count"], 100)
        self.assertEqual(manifest["software"]["turbo_picard_commit"], "b" * 40)
        command = manifest["commands"][0]
        self.assertEqual(command["parity"], "PASS")
        self.assertEqual(command["repeats"], 5)
        self.assertEqual(command["wall_seconds"]["turbo_picard_p95"], 2.5)
        self.assertEqual(command["peak_rss_bytes"]["picard_max"], 120.0)
        self.assertEqual(manifest["independent_reproduction"]["status"], "not_run")

    def test_writes_json_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "manifest-entry.json"
            args = args_fixture()
            manifest = build_production_manifest.build_manifest(args, report_fixture())
            output.write_text("{}\n", encoding="utf-8")
            output.write_text(json.dumps(manifest), encoding="utf-8")
            self.assertEqual(json.loads(output.read_text())["dataset_id"], "HG002-markduplicates")

    def test_rejects_incomplete_candidate(self) -> None:
        report = report_fixture()
        report["tools"]["turbo-picard"]["status"] = "incomplete"
        with self.assertRaises(SystemExit):
            build_production_manifest.build_manifest(args_fixture(), report)

    def test_production_manifest_requires_profile(self) -> None:
        report = report_fixture()
        del report["protocol"]["profile"]
        with self.assertRaisesRegex(SystemExit, "require protocol.profile"):
            build_production_manifest.build_manifest(args_fixture(), report)

    def test_production_manifest_rejects_zero_read_count(self) -> None:
        with self.assertRaisesRegex(SystemExit, "read-count greater than zero"):
            build_production_manifest.build_manifest(args_fixture(read_count=0), report_fixture())

    def test_accepts_picard_version_probe_that_returns_one(self) -> None:
        report = report_fixture()
        report["tools"]["picard"]["version"] = {
            "text": "Version:3.4.0",
            "exit_code": 1,
        }
        manifest = build_production_manifest.build_manifest(args_fixture(), report)
        self.assertEqual(manifest["software"]["picard_version"], "Version:3.4.0")

    def test_failed_parity_requires_a_difference(self) -> None:
        report = report_fixture()
        report["tools"]["turbo-picard"]["parity"] = {"status": "FAIL", "comparator": "exact"}
        with self.assertRaises(SystemExit):
            build_production_manifest.build_manifest(args_fixture(), report)

    def test_cram_manifest_carries_reported_reference_hash(self) -> None:
        report = report_fixture()
        report["input"]["format"] = "CRAM"
        report["input"]["reference_fasta"] = {"sha256": "c" * 64}
        manifest = build_production_manifest.build_manifest(args_fixture(), report)
        self.assertEqual(manifest["input"]["format"], "CRAM")
        self.assertEqual(manifest["input"]["reference_fasta_sha256"], "c" * 64)

    def test_cram_manifest_requires_reference_hash(self) -> None:
        report = report_fixture()
        report["input"]["format"] = "CRAM"
        with self.assertRaises(SystemExit):
            build_production_manifest.build_manifest(args_fixture(), report)

    def test_manifest_carries_profile_from_runner_protocol(self) -> None:
        report = report_fixture()
        report["protocol"].update({"profile": "umi_panel", "barcode_tag": "RX"})
        manifest = build_production_manifest.build_manifest(args_fixture(), report)
        self.assertEqual(manifest["commands"][0]["profile"], "umi_panel")
        self.assertEqual(manifest["commands"][0]["barcode_tags"], {"barcode_tag": "RX"})

    def test_umi_profile_requires_a_barcode_tag(self) -> None:
        report = report_fixture()
        report["protocol"]["profile"] = "umi_panel"
        with self.assertRaises(SystemExit):
            build_production_manifest.build_manifest(args_fixture(), report)

    def test_independent_reproduction_requires_matching_provenance(self) -> None:
        report = report_fixture()
        protocol_hash = build_production_manifest.build_manifest(
            args_fixture(), report
        )["commands"][0]["arguments_sha256"]
        manifest = build_production_manifest.build_manifest(
            args_fixture(
                independent_status="pass",
                reviewer="reviewer",
                independent_host_profile="independent-linux-x86_64",
                independent_turbo_picard_commit="b" * 40,
                independent_input_sha256="a" * 64,
                independent_arguments_sha256=protocol_hash,
                evidence_url="https://example.org/independent-bundle",
            ),
            report,
        )
        self.assertEqual(
            manifest["independent_reproduction"]["host_profile"],
            "independent-linux-x86_64",
        )

    def test_independent_reproduction_rejects_different_commit(self) -> None:
        with self.assertRaises(SystemExit):
            build_production_manifest.build_manifest(
                args_fixture(
                    independent_status="pass",
                    reviewer="reviewer",
                    independent_host_profile="independent-linux-x86_64",
                    independent_turbo_picard_commit="c" * 40,
                    independent_input_sha256="a" * 64,
                    independent_arguments_sha256="c" * 64,
                    evidence_url="https://example.org/independent-bundle",
                ),
                report_fixture(),
            )


if __name__ == "__main__":
    unittest.main()
