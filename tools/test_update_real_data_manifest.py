#!/usr/bin/env python3
"""Tests for real-data manifest update helper."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("update_real_data_manifest.py")
SPEC = importlib.util.spec_from_file_location("update_real_data_manifest", MODULE_PATH)
assert SPEC is not None
update_real_data_manifest = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["update_real_data_manifest"] = update_real_data_manifest
SPEC.loader.exec_module(update_real_data_manifest)


def manifest_entry(dataset_id: str, *, release_tier: str = "public_smoke") -> dict:
    return {
        "id": dataset_id,
        "input_path": f"benchmarks/real-data/{dataset_id}/input.bam",
        "evidence_json": f"benchmarks/real-data/{dataset_id}/real-data-comparison.json",
        "evidence_markdown": f"benchmarks/real-data/{dataset_id}/real-data-comparison.md",
        "source_url": "https://github.com/example/repo/blob/abc/input.bam",
        "source_commit": "abc",
        "sha256": "a" * 64,
        "scope_caveat": "small public fixture",
        "release_tier": release_tier,
        "expected_commands": {"ViewSam": "SAM record digest"},
    }


class UpdateRealDataManifestTests(unittest.TestCase):
    def test_merge_entry_appends_new_dataset(self) -> None:
        manifest = {"datasets": [manifest_entry("existing")]}
        merged = update_real_data_manifest.merge_entry(
            manifest,
            manifest_entry("new", release_tier="release_candidate"),
        )

        self.assertEqual([row["id"] for row in merged["datasets"]], ["existing", "new"])
        self.assertEqual(merged["datasets"][1]["release_tier"], "release_candidate")

    def test_merge_entry_rejects_duplicate_without_replace(self) -> None:
        manifest = {"datasets": [manifest_entry("existing")]}

        with self.assertRaisesRegex(ValueError, "dataset id already exists"):
            update_real_data_manifest.merge_entry(manifest, manifest_entry("existing"))

    def test_merge_entry_can_replace_existing_dataset(self) -> None:
        manifest = {"datasets": [manifest_entry("existing")]}
        replacement = manifest_entry("existing", release_tier="release_candidate")

        merged = update_real_data_manifest.merge_entry(
            manifest,
            replacement,
            replace=True,
        )

        self.assertEqual(len(merged["datasets"]), 1)
        self.assertEqual(merged["datasets"][0]["release_tier"], "release_candidate")

    def test_main_writes_valid_merged_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest_path = root / "manifest.json"
            entry_path = root / "manifest-entry.json"
            manifest_path.write_text(
                json.dumps({"datasets": [manifest_entry("existing")]}),
                encoding="utf-8",
            )
            entry_path.write_text(
                json.dumps(manifest_entry("new")),
                encoding="utf-8",
            )

            with redirect_stdout(StringIO()):
                self.assertEqual(
                    update_real_data_manifest.main(
                        ["--manifest", str(manifest_path), "--entry", str(entry_path)]
                    ),
                    0,
                )
            merged = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertEqual([row["id"] for row in merged["datasets"]], ["existing", "new"])

    def test_main_rejects_invalid_merged_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest_path = root / "manifest.json"
            entry_path = root / "manifest-entry.json"
            manifest_path.write_text('{"datasets": []}', encoding="utf-8")
            invalid = manifest_entry("new")
            invalid["release_tier"] = "not-a-tier"
            entry_path.write_text(json.dumps(invalid), encoding="utf-8")

            with redirect_stderr(StringIO()):
                self.assertEqual(
                    update_real_data_manifest.main(
                        ["--manifest", str(manifest_path), "--entry", str(entry_path)]
                    ),
                    1,
                )


if __name__ == "__main__":
    unittest.main()
